package music.shard.mobile

import android.content.Context
import android.content.pm.ServiceInfo
import androidx.work.*
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import com.yausername.ffmpeg.FFmpeg
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import kotlin.coroutines.coroutineContext

/** One unique WorkManager chain, one yt-dlp process and one audio download at a time. */
class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private val processId = id.toString()
    override suspend fun getForegroundInfo() = ForegroundInfo(201,
        Notifications.transfer(applicationContext, "Loading music / Загрузка музыки", WorkManager.getInstance(applicationContext).createCancelPendingIntent(id)),
        if (android.os.Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0)

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val errors = mutableListOf<String>()
        var added = 0
        var directory: File? = null
        try {
            setForeground(getForegroundInfo())
            val url = inputData.getString("url") ?: error("Missing URL")
            val metadata = Core.call("metadata", "url" to url) as JSONObject // strict desktop URL validation
            coroutineContext.ensureActive()
            YoutubeDL.init(applicationContext)
            FFmpeg.init(applicationContext)
            directory = File(Core.root, "staging/download-${UUID.randomUUID()}").apply { mkdirs() }
            var playlistId: Long? = null
            val entries: List<JSONObject>
            if (metadata.has("youtube")) {
                val source = metadata.getString("youtube")
                val request = YoutubeDLRequest(source).apply {
                    addOption("--flat-playlist"); addOption("--dump-single-json"); addOption("--skip-download")
                    addOption("--socket-timeout", "20"); addOption("--retries", "2")
                }
                val info = JSONObject(runInterruptible { YoutubeDL.execute(request, processId, null) }.out)
                if (info.has("entries")) {
                    playlistId = (Core.call("playlist_create", "name" to info.optString("title", "YouTube").take(100).ifBlank { "YouTube" }) as Number).toLong()
                    entries = info.getJSONArray("entries").objects().map { entry ->
                        val entryUrl = entry.optString("webpage_url").ifBlank { entry.optString("url") }
                        JSONObject().put("url", if (entryUrl.startsWith("https://")) entryUrl else "https://www.youtube.com/watch?v=${entry.optString("id")}")
                            .put("label", entry.optString("title", "YouTube"))
                    }
                } else entries = listOf(JSONObject().put("url", source).put("label", info.optString("title", "YouTube")))
            } else {
                entries = metadata.getJSONArray("tracks").objects()
                if (metadata.optBoolean("collection")) playlistId = (Core.call("playlist_create", "name" to metadata.getString("title").take(100).ifBlank { "Music" }) as Number).toLong()
            }
            for ((index, entry) in entries.withIndex()) {
                coroutineContext.ensureActive()
                if (isStopped) break
                val label = entry.optString("label").ifBlank { "${entry.optString("artist")} — ${entry.optString("title")}" }
                setProgress(workDataOf("message" to "${index + 1}/${entries.size}: $label"))
                try {
                    val attempt = File(directory, "track-$index").apply { mkdirs() }
                    val source = entry.getString("url")
                    val target = if (entry.has("artist")) findMatch(entry) else source
                    val request = YoutubeDLRequest(target).apply {
                        addOption("--no-playlist"); addOption("-f", "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio")
                        // Keep the original codec. MP4/M4A, Opus and Vorbis are decoded by Media3.
                        // Remux only when needed; no energy-intensive MP3 transcoding.
                        addOption("--extract-audio"); addOption("--audio-format", "best"); addOption("--write-info-json")
                        addOption("--socket-timeout", "20"); addOption("--retries", "2"); addOption("--fragment-retries", "2")
                        addOption("--concurrent-fragments", "1")
                        addOption("-o", File(attempt, "audio.%(ext)s").absolutePath)
                    }
                    runInterruptible { YoutubeDL.execute(request, processId, null) }
                    coroutineContext.ensureActive()
                    val audio = attempt.listFiles()?.firstOrNull { it.extension.lowercase() in setOf("m4a", "mp3", "opus", "ogg", "flac", "wav", "aac", "mp4") }
                        ?: error("No completed audio file")
                    val info = attempt.listFiles()?.firstOrNull { it.name.endsWith(".info.json") }?.let { JSONObject(it.readText()) }
                    val track = Core.call("import", "path" to audio.absolutePath, "source" to source,
                        "title" to entry.optString("title").ifBlank { info?.optString("track")?.ifBlank { info.optString("title") } },
                        "artist" to entry.optString("artist").ifBlank { info?.optString("artist")?.ifBlank { info.optString("uploader") } }) as JSONObject
                    playlistId?.let { Core.call("playlist_add", "id" to it, "track" to track.getLong("id")) }
                    added++
                    attempt.deleteRecursively()
                } catch (e: Exception) {
                    coroutineContext.ensureActive()
                    if (isStopped) break
                    errors += "$label: ${e.message}"
                }
            }
            val report = "Added / Добавлено: $added. Errors / Ошибок: ${errors.size}\n${errors.joinToString("\n")}".take(7000)
            Result.success(workDataOf("message" to report))
        } catch (e: Exception) {
            coroutineContext.ensureActive()
            Result.failure(workDataOf("message" to (e.message ?: "Download failed").take(7000)))
        } finally {
            YoutubeDL.destroyProcessById(processId)
            directory?.deleteRecursively()
        }
    }

    private suspend fun findMatch(entry: JSONObject): String {
        val query = "${entry.getString("artist")} ${entry.getString("title")} audio"
        val request = YoutubeDLRequest("ytsearch5:$query").apply {
            addOption("--flat-playlist"); addOption("--dump-single-json"); addOption("--skip-download")
            addOption("--socket-timeout", "20"); addOption("--retries", "2")
        }
        val data = runInterruptible { YoutubeDL.execute(request, processId, null) }.out
        val candidates = Core.call("match", "data" to data, "title" to entry.getString("title"),
            "artist" to entry.getString("artist"), "duration_ms" to entry.optLong("duration_ms")) as JSONArray
        return candidates.getString(0)
    }

    companion object {
        const val CHAIN = "shard-downloads"
        fun enqueue(context: Context, url: String, wifiOnly: Boolean, chargingOnly: Boolean) {
            val request = OneTimeWorkRequestBuilder<DownloadWorker>().setInputData(workDataOf("url" to url))
                .setConstraints(Constraints.Builder().setRequiredNetworkType(if (wifiOnly) NetworkType.UNMETERED else NetworkType.CONNECTED)
                    .setRequiresCharging(chargingOnly).setRequiresBatteryNotLow(true).build()).build()
            WorkManager.getInstance(context).enqueueUniqueWork(CHAIN, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
    }
}
