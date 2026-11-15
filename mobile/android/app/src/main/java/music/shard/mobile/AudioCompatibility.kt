package music.shard.mobile

import android.net.Uri
import androidx.media3.common.MediaItem
import com.yausername.ffmpeg.FFmpeg
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File

/** AIFF is accepted by desktop, but not by Media3's built-in extractors.
 * Convert it once, losslessly, on demand. Keep the original in the shared library.
 * Normal Android audio formats never start FFmpeg.
 */
object AudioCompatibility {
    private val mutex = Mutex()
    suspend fun item(track: Track): MediaItem = withContext(Dispatchers.IO) {
        val original = File(Core.root, track.path)
        if (original.extension.lowercase() !in setOf("aif", "aiff")) return@withContext track.mediaItem()
        mutex.withLock {
            FFmpeg.init(Core.context)
            val cache = File(Core.context.cacheDir, "compatible-audio").apply { mkdirs() }
            val target = File(cache, "${original.nameWithoutExtension}-${original.length()}-${original.lastModified()}.flac")
            if (!target.isFile) {
                val partial = File(cache, target.name + ".part")
                try {
                    val binary = File(Core.context.applicationInfo.nativeLibraryDir, "libffmpeg.so")
                    val libraries = File(Core.context.noBackupFilesDir, "youtubedl-android/packages/ffmpeg/usr/lib")
                    val process = ProcessBuilder(binary.absolutePath, "-nostdin", "-v", "error", "-y", "-i", original.absolutePath,
                        "-vn", "-map_metadata", "0", "-c:a", "flac", "-f", "flac", partial.absolutePath)
                        .redirectErrorStream(true).apply { environment()["LD_LIBRARY_PATH"] = libraries.absolutePath }.start()
                    try {
                        val error = runInterruptible { process.inputStream.bufferedReader().use { it.readText().take(4000) } }
                        check(runInterruptible { process.waitFor() } == 0) { "Cannot decode AIFF: $error" }
                    } finally { process.destroy() }
                    check(partial.renameTo(target)) { "Cannot save decoded audio cache" }
                } finally { partial.delete() }
                // Bound disk use. Never evict the item currently being prepared.
                var total = cache.listFiles().orEmpty().sumOf { it.length() }
                for (file in cache.listFiles().orEmpty().filter { it != target }.sortedBy { it.lastModified() }) {
                    if (total <= 256L * 1024 * 1024) break
                    val bytes = file.length()
                    if (file.delete()) total -= bytes
                }
            }
            track.mediaItem().buildUpon().setUri(Uri.fromFile(target)).build()
        }
    }
}
