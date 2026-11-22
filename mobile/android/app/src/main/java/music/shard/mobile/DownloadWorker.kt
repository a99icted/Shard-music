package music.shard.mobile

import android.content.Context
import android.content.pm.ServiceInfo
import androidx.work.*
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import kotlin.coroutines.coroutineContext as currentCoroutineContext

/** One WorkManager chain. Waiting for a duplicate decision finishes this worker;
 * confirmation appends the persisted plan to the queue without holding a wake lock.
 */
class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private val processId = id.toString()
    override suspend fun getForegroundInfo() = ForegroundInfo(201,
        Notifications.transfer(applicationContext, "Loading music / Загрузка музыки", WorkManager.getInstance(applicationContext).createCancelPendingIntent(id)),
        if (android.os.Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0)

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        var plan: JSONObject? = null
        var directory: File? = null
        var english = false
        try {
            setForeground(getForegroundInfo())
            english = Core.snapshot().preferences.optString("language") == "english"
            if (inputData.getBoolean("update_only", false)) {
                val version = DownloaderRuntime.prepare(applicationContext, force = true)
                return@withContext finish(if (english) "Downloader updated: $version" else "Загрузчик обновлён: $version", "")
            }
            val task = inputData.getString("plan")?.let { DownloadStore.load(it) }
                // Support jobs queued by an older installed version.
                ?: DownloadStore.create(inputData.getString("url") ?: error("Missing URL"), true, false)
            plan = task
            if (task.optString("status") in setOf("cancelled", "awaiting")) return@withContext Result.success()
            task.put("status", "running"); DownloadStore.save(task)
            if (!task.optBoolean("prepared")) prepare(task, english)
            val entries = task.getJSONArray("entries").objects()
            val validIds = Core.snapshot().tracks.filter { File(Core.root, it.path).isFile }.map { it.id }.toSet()
            for (entry in entries) {
                val existing = entry.optJSONObject("duplicate")?.optLong("existing_id", 0) ?: 0
                if (existing > 0 && existing !in validIds) entry.remove("duplicate")
                // A user may delete audio while a failed job waits for a manual retry.
                if (entry.optString("state") != "done" && entry.has("track_id") && entry.getLong("track_id") !in validIds)
                    entry.remove("track_id")
            }
            // A collection added to the library while this job waited may now overlap too.
            val inspected = Core.call("download_duplicates", "entries" to task.getJSONArray("entries")) as JSONArray
            var ask = false
            for (duplicate in inspected.objects()) {
                val entry = entries[duplicate.getInt("index")]
                if (entry.optString("state") != "done" && !entry.has("track_id") && !entry.has("duplicate")) {
                    entry.put("duplicate", duplicate); ask = true
                }
            }
            task.put("duplicates", JSONArray(entries.mapNotNull { it.optJSONObject("duplicate") }))
            if (ask || (!task.optBoolean("decided") && task.getJSONArray("duplicates").length() > 0)) {
                task.put("status", "awaiting"); DownloadStore.save(task)
                return@withContext finish(if (english) "Choose how to handle duplicate tracks" else "Нужно выбрать, что делать с дубликатами", "")
            }
            if (task.optBoolean("collection") && !task.has("playlist_id")) {
                task.put("playlist_id", (Core.call("playlist_create", "name" to task.getString("title").take(100).ifBlank { "Music" }) as Number).toLong())
                DownloadStore.save(task)
            }
            directory = File(Core.root, "staging/download-${UUID.randomUUID()}").apply { mkdirs() }
            val staging = directory
            var prepared = false
            processDownloads(task,
                persist = { DownloadStore.save(task, it) },
                download = { entry ->
                    currentCoroutineContext.ensureActive()
                    if (!prepared) { DownloaderRuntime.prepare(applicationContext); prepared = true }
                    val label = entry.downloadLabel()
                    val prefix = if (entry.optInt("attempts") > 1) (if (english) "Retry" else "Повтор") else (if (english) "Downloading" else "Загрузка")
                    setProgress(workDataOf("message" to "$prefix ${entry.getInt("index") + 1}/${entries.size}: ${label.take(500)}"))
                    download(entry, File(staging, "track-${entry.getInt("index")}"))
                },
                attach = { track -> task.optLong("playlist_id", 0).takeIf { it > 0 }?.let { Core.call("playlist_add", "id" to it, "track" to track) }; Unit })
            task.optLong("playlist_id", 0).takeIf { it > 0 }?.let { playlist ->
                Core.call("playlist_order", "id" to playlist, "tracks" to JSONArray(entries.filter { it.optString("state") == "done" }.map { it.getLong("track_id") }.distinct()))
            }
            val completed = entries.count { it.optString("state") == "done" }
            val reused = entries.count { it.optString("state") == "done" && it.optBoolean("reused") }
            val failures = entries.filter { it.optString("state") != "done" }
            task.put("status", "finished"); DownloadStore.save(task)
            val report = if (english) "Added: ${completed - reused}. Reused: $reused. Errors: ${failures.size}." else "Добавлено: ${completed - reused}. Уже в библиотеке: $reused. Ошибок: ${failures.size}."
            finish(report + if (failures.isNotEmpty()) "\n" + DownloadFailure.summary(failures.first().optString("error"), english) else "",
                failures.take(100).joinToString("\n\n") { "${it.downloadLabel()}: ${it.optString("error")}" })
        } catch (error: Exception) {
            if (error is CancellationException) throw error
            currentCoroutineContext.ensureActive()
            val raw = error.message ?: "Download failed"
            plan?.let { it.put("status", "failed").put("fatal_error", raw); DownloadStore.save(it) }
            finish(DownloadFailure.summary(raw, english), raw)
        } finally {
            YoutubeDL.destroyProcessById(processId)
            directory?.deleteRecursively()
        }
    }

    private suspend fun prepare(plan: JSONObject, english: Boolean) {
        setProgress(workDataOf("message" to if (english) "Reading collection…" else "Читаем список треков…"))
        DownloaderRuntime.prepare(applicationContext)
        val metadata = Core.call("metadata", "url" to plan.getString("url")) as JSONObject
        val entries: List<JSONObject>
        if (metadata.has("youtube")) {
            val source = metadata.getString("youtube")
            val request = YoutubeDLRequest(source).apply {
                addOption("--flat-playlist"); addOption("--dump-single-json"); addOption("--skip-download")
                addOption("--socket-timeout", "20"); addOption("--retries", "2")
            }
            val info = JSONObject(DownloaderRuntime.execute(applicationContext, request, processId).out)
            plan.put("collection", info.has("entries")).put("title", info.optString("title", "YouTube").take(100))
            entries = (info.optJSONArray("entries")?.objects() ?: listOf(info)).map { item ->
                val address = item.optString("webpage_url").ifBlank { item.optString("url") }
                JSONObject().put("url", if (address.startsWith("https://")) address else if (!info.has("entries")) source else "https://www.youtube.com/watch?v=${item.getString("id")}")
                    .put("title", item.optString("track").ifBlank { item.optString("title", "YouTube") })
                    .put("artist", item.optString("artist").ifBlank { item.optString("uploader").ifBlank { item.optString("channel") } })
                    .put("duration_ms", item.optDouble("duration", 0.0).takeIf { it.isFinite() && it > 0 }?.let { (it * 1000).toLong() })
            }
        } else {
            entries = metadata.getJSONArray("tracks").objects().onEach { it.put("match_search", true) }
            plan.put("collection", metadata.optBoolean("collection")).put("title", metadata.optString("title", "Music").take(100))
        }
        require(entries.isNotEmpty()) { "No tracks found / В ссылке не найдены треки" }
        entries.forEachIndexed { index, entry -> entry.put("index", index).put("state", "pending").put("attempts", 0) }
        plan.put("entries", JSONArray(entries)).put("prepared", true)
        DownloadStore.save(plan)
    }

    private suspend fun download(entry: JSONObject, attempt: File): Long {
        attempt.mkdirs()
        try {
            val source = entry.getString("url")
            val target = if (entry.optBoolean("match_search")) findMatch(entry) else source
            val request = YoutubeDLRequest(target).apply {
                addOption("--no-playlist"); addOption("-f", "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio")
                addOption("--extract-audio"); addOption("--audio-format", "best"); addOption("--write-info-json")
                addOption("--socket-timeout", "20"); addOption("--retries", "2"); addOption("--fragment-retries", "2")
                addOption("--concurrent-fragments", "1")
                addOption("-o", File(attempt, "audio.%(ext)s").absolutePath)
            }
            DownloaderRuntime.execute(applicationContext, request, processId)
            currentCoroutineContext.ensureActive()
            val audio = attempt.listFiles()?.firstOrNull { it.extension.lowercase() in setOf("m4a", "mp3", "opus", "ogg", "flac", "wav", "aac", "mp4") }
                ?: error("No completed audio file")
            val info = attempt.listFiles()?.firstOrNull { it.name.endsWith(".info.json") }?.let { JSONObject(it.readText()) }
            val track = Core.call("import", "path" to audio.absolutePath, "source" to source,
                "title" to entry.optString("title").ifBlank { info?.optString("track")?.ifBlank { info.optString("title") } },
                "artist" to entry.optString("artist").ifBlank { info?.optString("artist")?.ifBlank { info.optString("uploader") } },
                "thumbnail" to entry.optString("thumbnail").takeUnless { it.isBlank() || it == "null" }.orEmpty().ifBlank { info?.optString("thumbnail").orEmpty() }) as JSONObject
            return track.getLong("id")
        } finally { attempt.deleteRecursively() }
    }

    private fun finish(message: String, raw: String): Result {
        DownloadStore.report(message, raw)
        // A handled track/metadata failure must not cancel later jobs in the chain.
        return Result.success(workDataOf("message" to utf8Excerpt(message)))
    }

    private suspend fun findMatch(entry: JSONObject): String {
        val request = YoutubeDLRequest("ytsearch5:${entry.getString("artist")} ${entry.getString("title")} audio").apply {
            addOption("--flat-playlist"); addOption("--dump-single-json"); addOption("--skip-download")
            addOption("--socket-timeout", "20"); addOption("--retries", "2")
        }
        val data = DownloaderRuntime.execute(applicationContext, request, processId).out
        val candidates = Core.call("match", "data" to data, "title" to entry.getString("title"),
            "artist" to entry.getString("artist"), "duration_ms" to entry.optLong("duration_ms")) as JSONArray
        return candidates.getString(0)
    }

    companion object {
        const val CHAIN = "shard-downloads"
        fun update(context: Context) {
            val request = OneTimeWorkRequestBuilder<DownloadWorker>().setInputData(workDataOf("update_only" to true))
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()).build()
            WorkManager.getInstance(context).enqueueUniqueWork(CHAIN, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
        suspend fun enqueue(context: Context, url: String, wifiOnly: Boolean, chargingOnly: Boolean) {
            enqueuePlan(context, DownloadStore.create(url, wifiOnly, chargingOnly))
        }
        internal fun enqueuePlan(context: Context, plan: JSONObject) {
            val request = OneTimeWorkRequestBuilder<DownloadWorker>().setInputData(workDataOf("plan" to plan.getString("id")))
                .setConstraints(Constraints.Builder().setRequiredNetworkType(if (plan.optBoolean("wifi")) NetworkType.UNMETERED else NetworkType.CONNECTED)
                    .setRequiresCharging(plan.optBoolean("charging")).setRequiresBatteryNotLow(true).build()).build()
            WorkManager.getInstance(context).enqueueUniqueWork(CHAIN, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
    }
}

internal fun JSONObject.downloadLabel(): String = optString("label").ifBlank {
    listOf(optString("artist"), optString("title")).filter { it.isNotBlank() }.joinToString(" — ")
}

// WorkManager Data has a 10 KiB UTF-8 limit, including serialized overhead.
internal fun utf8Excerpt(raw: String): String {
    val out = StringBuilder()
    var bytes = 0
    val iterator = raw.codePoints().iterator()
    while (iterator.hasNext()) {
        val point = iterator.nextInt()
        val size = when { point <= 0x7f -> 1; point <= 0x7ff -> 2; point <= 0xffff -> 3; else -> 4 }
        if (bytes + size > 6000) { out.append("\n…"); break }
        out.appendCodePoint(point); bytes += size
    }
    return out.toString()
}
