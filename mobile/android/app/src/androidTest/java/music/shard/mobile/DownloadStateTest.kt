package music.shard.mobile

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class DownloadStateTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun dismissPersistsAndRetryKeepsSuccessfulAudio(): Unit = runBlocking {
        val original = Core.snapshot().preferences.optString("language")
        Core.call("preferences", "patch" to JSONObject().put("language", "russian"))
        val plan = DownloadStore.create("https://www.youtube.com/watch?v=test", true, false)
        val id = plan.getString("id")
        plan.put("prepared", true).put("status", "finished").put("title", "Retry state test")
            .put("entries", JSONArray()
                .put(JSONObject().put("index", 0).put("state", "done").put("track_id", 42).put("attempts", 1))
                .put(JSONObject().put("index", 1).put("state", "error").put("attempts", 2).put("error", "403")))
        DownloadStore.save(plan)
        DownloadStore.report("Тест ошибки загрузки", "ERROR: 403")
        try {
            ActivityScenario.launch(MainActivity::class.java).use { activity ->
                compose.waitUntil(15000) { compose.onAllNodesWithContentDescription("Убрать отчёт").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithContentDescription("Убрать отчёт").performClick()
                compose.waitUntil(15000) { compose.onAllNodesWithText("Тест ошибки загрузки").fetchSemanticsNodes().isEmpty() }
                activity.recreate()
                compose.waitForIdle()
                compose.onNodeWithText("Тест ошибки загрузки").assertDoesNotExist()
                assertEquals(1, DownloadStore.failures().find { it.id == id }!!.count)
            }
            val resumed = DownloadStore.retry(id)
            assertEquals("done", resumed.getJSONArray("entries").getJSONObject(0).getString("state"))
            assertEquals(42L, resumed.getJSONArray("entries").getJSONObject(0).getLong("track_id"))
            assertEquals("pending", resumed.getJSONArray("entries").getJSONObject(1).getString("state"))
            assertEquals(0, DownloadStore.load(id).getJSONArray("entries").getJSONObject(1).getInt("attempts"))
            assertFalse(DownloadStore.failures().any { it.id == id })
        } finally {
            DownloadStore.delete(id); DownloadStore.dismissReport()
            Core.call("preferences", "patch" to JSONObject().put("language", original))
        }
    }

    @Test fun duplicateChoicesSurviveReloadAndCanBeCancelled(): Unit = runBlocking {
        val original = Core.snapshot().preferences.optString("language")
        Core.call("preferences", "patch" to JSONObject().put("language", "russian"))
        val plan = DownloadStore.create("https://www.youtube.com/playlist?list=test", true, false)
        val id = plan.getString("id")
        val duplicate = JSONObject().put("index", 0).put("existing_id", 42).put("earlier_index", JSONObject.NULL)
            .put("existing_title", "Existing recording").put("kind", "same_source")
        plan.put("prepared", true).put("status", "awaiting").put("title", "Duplicate state test")
            .put("entries", JSONArray().put(JSONObject().put("index", 0).put("state", "pending")
                .put("title", "Existing recording").put("duplicate", duplicate)))
        DownloadStore.save(plan)
        try {
            assertEquals(1, DownloadStore.load(id).getJSONArray("duplicates").length())
            val resumed = DownloadStore.resolve(id, setOf(0))
            assertTrue(resumed.getJSONArray("entries").getJSONObject(0).getBoolean("copy"))
            assertTrue(DownloadStore.load(id).getBoolean("decided"))
            resumed.put("status", "awaiting"); DownloadStore.save(resumed)
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.waitUntil(15000) { compose.onAllNodesWithText("Повторяющиеся треки").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Отменить загрузку").performClick()
                compose.waitUntil(15000) { compose.onAllNodesWithText("Повторяющиеся треки").fetchSemanticsNodes().isEmpty() }
            }
            assertEquals("cancelled", DownloadStore.load(id).getString("status"))
        } finally {
            DownloadStore.delete(id); DownloadStore.dismissReport()
            Core.call("preferences", "patch" to JSONObject().put("language", original))
        }
    }

    @Test fun workerReusesAudioThroughRustWithoutNetworkDownload(): Unit = runBlocking {
        Core.snapshot()
        val wave = File(Core.root, "staging/${UUID.randomUUID()}.wav")
        val pcm = ByteArray(16000)
        wave.writeBytes(ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
            .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
            .put("data".toByteArray()).putInt(pcm.size).array() + pcm)
        val source = "https://www.youtube.com/watch?v=${UUID.randomUUID()}"
        val track = (Core.call("import", "path" to wave.absolutePath, "source" to source) as JSONObject).getLong("id")
        val playlist = (Core.call("playlist_create", "name" to "Worker reuse test") as Number).toLong()
        val before = Core.snapshot().tracks.map { it.id }.toSet()
        val plan = DownloadStore.create(source, false, false)
        val id = plan.getString("id")
        plan.put("prepared", true).put("collection", true).put("playlist_id", playlist).put("decided", true)
            .put("entries", JSONArray().put(JSONObject().put("index", 0).put("url", source)
                .put("state", "pending").put("attempts", 0).put("track_id", Long.MAX_VALUE).put("duplicate", JSONObject()
                    .put("index", 0).put("existing_id", track).put("earlier_index", JSONObject.NULL).put("kind", "same_source"))))
        DownloadStore.save(plan)
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                DownloadWorker.enqueuePlan(Core.context, plan)
                withTimeout(60000) {
                    while (DownloadStore.load(id).optString("status") !in setOf("finished", "failed")) delay(200)
                }
                val completed = DownloadStore.load(id)
                assertEquals(completed.toString(), "finished", completed.getString("status"))
                assertTrue(completed.getJSONArray("entries").getJSONObject(0).getBoolean("reused"))
                assertEquals(before, Core.snapshot().tracks.map { it.id }.toSet())
                assertEquals(listOf(track), Core.snapshot().tracksIn(playlist).map { it.id })
            }
        } finally {
            DownloadStore.delete(id); DownloadStore.dismissReport()
            Core.call("playlist_delete", "id" to playlist)
            Core.call("track_delete", "id" to track)
            wave.delete()
        }
    }
}
