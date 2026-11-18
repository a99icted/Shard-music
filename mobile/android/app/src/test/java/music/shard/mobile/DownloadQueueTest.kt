package music.shard.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class DownloadQueueTest {
    private fun plan(count: Int) = JSONObject().put("entries", JSONArray((0 until count).map {
        JSONObject().put("index", it).put("state", "pending").put("attempts", 0)
    })).put("duplicates", JSONArray())

    @Test fun failuresMoveBehindEveryFirstAttempt(): Unit = runBlocking {
        val plan = plan(4)
        val attempts = mutableListOf<Int>()
        processDownloads(plan, {}, { entry ->
            val index = entry.getInt("index"); attempts += index
            if (index == 1 && entry.getInt("attempts") == 1) error("temporary")
            index + 10L
        }, {})
        assertEquals(listOf(0, 1, 2, 3, 1), attempts)
        assertTrue(plan.getJSONArray("entries").objects().all { it.getString("state") == "done" })
    }

    @Test fun permanentFailureHasOnlyTwoAttemptsAndDoesNotBlockOthers(): Unit = runBlocking {
        val plan = plan(3)
        val attempts = mutableListOf<Int>()
        processDownloads(plan, {}, { entry ->
            val index = entry.getInt("index"); attempts += index
            if (index == 0) error("403")
            index + 10L
        }, {})
        assertEquals(listOf(0, 1, 2, 0), attempts)
        assertEquals("error", plan.getJSONArray("entries").getJSONObject(0).getString("state"))
        assertEquals("done", plan.getJSONArray("entries").getJSONObject(2).getString("state"))
    }

    @Test fun duplicatesReuseLocalAndEarlierAudioEvenAfterRetry(): Unit = runBlocking {
        val plan = plan(3).put("duplicates", JSONArray()
            .put(JSONObject().put("index", 1).put("existing_id", 99).put("earlier_index", JSONObject.NULL))
            .put(JSONObject().put("index", 2).put("existing_id", JSONObject.NULL).put("earlier_index", 0)))
        val downloads = mutableListOf<Int>()
        val attached = mutableListOf<Long>()
        processDownloads(plan, {}, { entry ->
            downloads += entry.getInt("index")
            if (entry.getInt("attempts") == 1) error("temporary")
            42L
        }, { attached += it })
        assertEquals(listOf(0, 0), downloads)
        assertEquals(listOf(99L, 42L, 42L), attached)
        assertEquals(42L, plan.getJSONArray("entries").getJSONObject(2).getLong("track_id"))
    }

    @Test fun explicitCopyDownloadsAgainWhileFinishedEntriesSurviveRestart(): Unit = runBlocking {
        val plan = plan(3).put("duplicates", JSONArray()
            .put(JSONObject().put("index", 1).put("existing_id", 99).put("earlier_index", JSONObject.NULL)))
        plan.getJSONArray("entries").getJSONObject(0).put("state", "done").put("track_id", 10)
        plan.getJSONArray("entries").getJSONObject(1).put("copy", true)
        val downloads = mutableListOf<Int>()
        processDownloads(plan, {}, { downloads += it.getInt("index"); 50L + it.getInt("index") }, {})
        assertEquals(listOf(1, 2), downloads)
        processDownloads(JSONObject(plan.toString()), {}, { error("Completed audio was downloaded twice") }, {})
    }

    @Test fun cancellationStopsImmediatelyAndCanResume(): Unit = runBlocking {
        val plan = plan(2)
        try {
            processDownloads(plan, {}, { throw CancellationException("cancel") }, {})
            fail("Cancellation was swallowed")
        } catch (_: CancellationException) { }
        assertFalse(plan.getJSONArray("entries").getJSONObject(0).has("error"))
        val resumed = mutableListOf<Int>()
        processDownloads(plan, {}, { resumed += it.getInt("index"); 1L }, {})
        assertEquals(listOf(1, 0), resumed)
    }
}
