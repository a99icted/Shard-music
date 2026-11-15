package music.shard.mobile

import android.content.Context
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import com.yausername.youtubedl_android.YoutubeDLResponse
import com.yausername.ffmpeg.FFmpeg
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File

/** Updates and extraction share a lock: never replace yt-dlp under a running process.
 * No periodic worker or startup traffic; check only on an explicit download/update.
 */
object DownloaderRuntime {
    private val mutex = Mutex()
    private var lastRepairProcessId: String? = null
    private const val DAY = 24 * 60 * 60 * 1000L
    private const val FAILED_CHECK_BACKOFF = 60 * 60 * 1000L

    suspend fun prepare(context: Context, force: Boolean = false): String = withContext(Dispatchers.IO) {
        mutex.withLock {
            initialize(context)
            updateLocked(context, force)
            version(context)
        }
    }

    private fun initialize(context: Context) {
        YoutubeDL.init(context)
        FFmpeg.init(context)
    }

    private suspend fun updateLocked(context: Context, force: Boolean) {
        val prefs = context.getSharedPreferences("downloader", Context.MODE_PRIVATE)
        val now = System.currentTimeMillis()
        val last = prefs.getLong("last_success", 0)
        val attempted = prefs.getLong("last_attempt", 0)
        if (!force && last > 0 && now - last in 0 until DAY) return
        if (!force && attempted > 0 && now - attempted in 0 until FAILED_CHECK_BACKOFF) return
        prefs.edit().putLong("last_attempt", now).apply()
        try {
            runInterruptible { YoutubeDL.updateYoutubeDL(context, YoutubeDL.UpdateChannel.STABLE) }
            prefs.edit().putLong("last_success", now).remove("update_error").putString("version", version(context)).apply()
        } catch (error: Exception) {
            if (error is kotlinx.coroutines.CancellationException) throw error
            prefs.edit().putString("update_error", error.message.orEmpty()).apply()
            // An offline update endpoint must not prevent playback or using a working extractor.
            // An explicit update reports its failure; automatic checks retain the bundled version.
            if (force) throw error
        }
    }

    fun version(context: Context): String = YoutubeDL.version(context).orEmpty()

    suspend fun execute(context: Context, request: YoutubeDLRequest, processId: String): YoutubeDLResponse = withContext(Dispatchers.IO) {
        mutex.withLock {
            initialize(context)
            // Modern YouTube signature solving uses the packaged QuickJS runtime.
            // Allow the official yt-dlp EJS solver when the bundled copy is outdated.
            request.addOption("--remote-components", "ejs:github")
            // Reuse signatures and the solver between tracks instead of fetching each time.
            // Android may reclaim this cache; it contains no music library files.
            request.addOption("--cache-dir", File(context.cacheDir, "yt-dlp").absolutePath)
            try {
                withTransientDownloadRetry { runInterruptible { YoutubeDL.execute(request, processId, null) } }
            } catch (error: Exception) {
                if (error is kotlinx.coroutines.CancellationException) throw error
                if (!DownloadFailure.needsExtractorRefresh(error.message.orEmpty())) throw error
                // A blocked playlist must not query GitHub again for every failed track.
                if (lastRepairProcessId == processId) throw error
                lastRepairProcessId = processId
                val before = version(context)
                // One repair attempt, only when the extractor changed. Retrying an unchanged
                // blocked recording cannot solve geographic/account restrictions.
                try {
                    updateLocked(context, force = true)
                } catch (updateError: Exception) {
                    if (updateError is kotlinx.coroutines.CancellationException) throw updateError
                    error.addSuppressed(updateError)
                    throw error
                }
                if (version(context) == before) throw error
                withTransientDownloadRetry { runInterruptible { YoutubeDL.execute(request, processId, null) } }
            }
        }
    }
}

// yt-dlp's media retries do not cover every EJS/player-script failure. Retry the
// extraction once for a transport failure, even if a later line reports HTTP 403.
// A plain 403 is not a transient network error and does not enter this path.
internal suspend fun <T> withTransientDownloadRetry(block: suspend () -> T): T {
    return try {
        block()
    } catch (error: Exception) {
        if (error is CancellationException || !DownloadFailure.isTransientNetworkFailure(error.message.orEmpty())) throw error
        delay(1_000)
        block()
    }
}

internal object DownloadFailure {
    fun isTransientNetworkFailure(raw: String): Boolean = listOf(
        "timed out", "TimeoutError", "UNEXPECTED_EOF_WHILE_READING",
        "Connection reset", "RemoteDisconnected", "Temporary failure in name resolution"
    ).any { raw.contains(it, ignoreCase = true) }

    fun needsExtractorRefresh(raw: String): Boolean = raw.contains("403") || raw.contains("signature", true) || raw.contains("nsig", true) || raw.contains("SABR", true)

    fun summary(raw: String, english: Boolean): String {
        fun t(ru: String, en: String) = if (english) en else ru
        return when {
            isTransientNetworkFailure(raw) -> t("Соединение с сервисом оборвалось или истекло время ожидания. Попробуйте другую сеть и запустите загрузку снова.",
                "The connection to the service was interrupted or timed out. Try another network and start the download again.")
            raw.contains("403") -> t("YouTube отклонил загрузку (403). Обновите загрузчик во вкладке «Загрузки» и попробуйте снова. Если он уже обновлён, проверьте доступность записи и сеть.",
                "YouTube rejected the download (403). Update the downloader in Downloads and try again. If it is current, check the recording's availability and your network.")
            raw.contains("Sign in", true) || raw.contains("age-restricted", true) || raw.contains("Private video", true) -> t("Для этой записи YouTube требует вход или ограничивает доступ. Попробуйте открытую запись.", "YouTube requires sign-in or restricts this recording. Try a public recording.")
            raw.contains("timed out", true) || raw.contains("Unable to download", true) || raw.contains("UnknownHost", true) -> t("Не удалось связаться с сервисом. Проверьте интернет и повторите загрузку.", "Could not reach the service. Check your internet connection and try again.")
            raw.contains("unavailable", true) || raw.contains("not available", true) -> t("Запись недоступна или удалена.", "This recording is unavailable or has been removed.")
            else -> {
                val error = raw.lineSequence().map { it.trim() }.filter { it.isNotBlank() }
                    .firstOrNull { it.startsWith("ERROR:") }?.removePrefix("ERROR:")?.trim()
                (error ?: raw.lineSequence().firstOrNull { it.isNotBlank() && !it.startsWith("WARNING:") }
                    ?: t("Не удалось скачать музыку. Откройте технические подробности.", "Could not download music. Open technical details.")).take(280)
            }
        }
    }
}
