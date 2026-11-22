package music.shard.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Opt-in live test; ordinary offline CI does not depend on YouTube availability. */
@RunWith(AndroidJUnit4::class)
class DownloaderNetworkTest {
    @Test fun updateAndDownloadYtDlpTestRecording() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("downloaderNetwork") == "true")
        val before = Core.snapshot().tracks.map { it.id }.toSet()
        val version = DownloaderRuntime.prepare(Core.context, force = true)
        assertTrue("Extractor version: $version", version.isNotBlank())
        assertFalse("Bundled stale extractor still installed", version.startsWith("2025.11.12"))
        val plans = mutableListOf<String>()
        var imported: Long? = null
        suspend fun waitFor(id: String, statuses: Set<String>): JSONObject = withTimeout(180000) {
            var plan = DownloadStore.load(id)
            while (plan.optString("status") !in statuses) { delay(250); plan = DownloadStore.load(id) }
            plan
        }
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                // Public five-second recording used by current yt-dlp extraction tests.
                val source = "https://www.youtube.com/watch?v=gHKT4uU8Zng"
                val first = DownloadStore.create(source, false, false)
                plans += first.getString("id")
                DownloadWorker.enqueuePlan(Core.context, first)
                var completed = waitFor(plans[0], setOf("finished", "failed", "awaiting"))
                if (completed.optString("status") == "awaiting") {
                    // Preserve an older test/user copy already on the device.
                    DownloadWorker.enqueuePlan(Core.context, DownloadStore.resolve(plans[0], setOf(0)))
                    completed = waitFor(plans[0], setOf("finished", "failed"))
                }
                assertEquals(completed.toString(), "finished", completed.getString("status"))
                val entry = completed.getJSONArray("entries").getJSONObject(0)
                assertEquals(completed.toString(), "done", entry.getString("state"))
                imported = entry.getLong("track_id")
                assertFalse("First attempt unexpectedly reused pre-existing audio", imported in before)
                val audio = Core.snapshot().tracks.single { it.id == imported }
                assertTrue(java.io.File(Core.root, audio.path).length() > 1000)
                val count = Core.snapshot().tracks.size

                val second = DownloadStore.create(source, false, false)
                plans += second.getString("id")
                DownloadWorker.enqueuePlan(Core.context, second)
                val pending = waitFor(plans[1], setOf("finished", "failed", "awaiting"))
                assertEquals(pending.toString(), "awaiting", pending.getString("status"))
                assertEquals(imported!!, pending.getJSONArray("duplicates").getJSONObject(0).getLong("existing_id"))
                DownloadWorker.enqueuePlan(Core.context, DownloadStore.resolve(plans[1], emptySet()))
                val reused = waitFor(plans[1], setOf("finished", "failed"))
                assertEquals(reused.toString(), "finished", reused.getString("status"))
                assertTrue(reused.getJSONArray("entries").getJSONObject(0).getBoolean("reused"))
                assertEquals(count, Core.snapshot().tracks.size)
            }
        } finally {
            for (id in plans) DownloadStore.delete(id)
            DownloadStore.dismissReport()
            imported?.takeUnless { it in before }?.let { Core.call("track_delete", "id" to it) }
        }
    }
}
