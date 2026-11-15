package music.shard.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.yausername.youtubedl_android.YoutubeDLRequest
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** Opt-in live test; ordinary offline CI does not depend on YouTube availability. */
@RunWith(AndroidJUnit4::class)
class DownloaderNetworkTest {
    @Test fun updateAndDownloadYtDlpTestRecording() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("downloaderNetwork") == "true")
        Core.snapshot()
        val version = DownloaderRuntime.prepare(Core.context, force = true)
        assertTrue("Extractor version: $version", version.isNotBlank())
        assertFalse("Bundled stale extractor still installed", version.startsWith("2025.11.12"))
        val directory = File(Core.root, "staging/network-test-${UUID.randomUUID()}").apply { mkdirs() }
        try {
            // Public five-second recording used by current yt-dlp extraction tests.
            val request = YoutubeDLRequest("https://www.youtube.com/watch?v=gHKT4uU8Zng").apply {
                addOption("--no-playlist"); addOption("-f", "bestaudio")
                addOption("--socket-timeout", "20"); addOption("--retries", "1")
                addOption("-o", File(directory, "test.%(ext)s").absolutePath)
            }
            DownloaderRuntime.execute(Core.context, request, "network-test")
            assertTrue("No downloaded audio", directory.listFiles().orEmpty().any { it.length() > 1000 && it.extension != "part" })
        } finally { directory.deleteRecursively() }
    }
}
