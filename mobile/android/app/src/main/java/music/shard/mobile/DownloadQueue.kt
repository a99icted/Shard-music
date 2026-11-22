package music.shard.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ensureActive
import org.json.JSONObject
import kotlin.coroutines.coroutineContext

/** First attempts in source order, then at most one second attempt for failures.
 * Completed entries survive worker/process restarts and manual retry requests.
 */
internal suspend fun processDownloads(
    plan: JSONObject,
    persist: suspend (JSONObject) -> Unit,
    download: suspend (JSONObject) -> Long,
    attach: suspend (Long) -> Unit
) {
    val entries = plan.getJSONArray("entries").objects()
    val duplicates = plan.optJSONArray("duplicates")?.objects().orEmpty().associateBy { it.getInt("index") }
    val queue = ArrayDeque(entries.filter { it.optString("state") != "done" && it.optInt("attempts") < 2 &&
        (it.optBoolean("copy") || duplicates[it.getInt("index")]?.isNull("earlier_index") != false) }.sortedBy { it.optInt("attempts") })
    while (queue.isNotEmpty()) {
        coroutineContext.ensureActive()
        val entry = queue.removeFirst()
        val duplicate = duplicates[entry.getInt("index")]
        entry.put("attempts", entry.optInt("attempts") + 1)
        persist(entry) // Account for an attempt even if the OS kills the worker mid-transfer.
        try {
            val existing = duplicate?.takeUnless { entry.optBoolean("copy") }?.optLong("existing_id", 0) ?: 0
            val id = entry.optLong("track_id", 0).takeIf { it > 0 }
                ?: existing.takeIf { it > 0 } ?: download(entry)
            entry.put("track_id", id)
            persist(entry) // Do not download again if linking into the playlist fails.
            attach(id)
            entry.put("state", "done").put("reused", existing > 0).remove("error")
        } catch (error: Exception) {
            if (error is CancellationException) throw error
            coroutineContext.ensureActive()
            entry.put("state", "error").put("error", utf8Excerpt(error.message ?: "Download failed"))
            if (entry.optInt("attempts") < 2) queue.addLast(entry)
        }
        persist(entry)
    }
    for (entry in entries) if (entry.optString("state") != "done" && entry.optInt("attempts") >= 2) {
        entry.put("state", "error")
        if (!entry.has("error")) entry.put("error", "Transfer interrupted / Загрузка прервана")
        persist(entry)
    }
    // Aliases are resolved after retries, so a repeated playlist entry never causes
    // another download while its original entry is waiting at the tail of the queue.
    for (entry in entries) {
        val duplicate = duplicates[entry.getInt("index")] ?: continue
        if (entry.optString("state") == "done" || entry.optBoolean("copy") || duplicate.isNull("earlier_index")) continue
        coroutineContext.ensureActive()
        val earlier = entries[duplicate.getInt("earlier_index")]
        if (earlier.optString("state") == "done") {
            val id = earlier.getLong("track_id")
            try {
                attach(id)
                entry.put("track_id", id).put("state", "done").put("reused", true).remove("error")
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                coroutineContext.ensureActive()
                entry.put("state", "error").put("error", utf8Excerpt(error.message.orEmpty()))
            }
        } else entry.put("state", "error").put("error", "Original entry failed / Не удалось скачать исходный трек")
        persist(entry)
    }
}
