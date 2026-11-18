package music.shard.mobile

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

internal data class FailedDownload(val id: String, val title: String, val count: Int)

/** Durable Android job state, separate from the shared Rust music database.
 * Only the changed entry is written after a track, even in a large playlist.
 */
internal object DownloadStore {
    private val helper by lazy { object : SQLiteOpenHelper(Core.context, "downloads.db", null, 1) {
        override fun onConfigure(db: SQLiteDatabase) { db.setForeignKeyConstraintsEnabled(true) }
        override fun onCreate(db: SQLiteDatabase) {
            db.execSQL("CREATE TABLE plans(id TEXT PRIMARY KEY, status TEXT NOT NULL, data TEXT NOT NULL, created INTEGER NOT NULL)")
            db.execSQL("CREATE TABLE entries(plan TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE, position INTEGER NOT NULL, state TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(plan,position))")
            db.execSQL("CREATE INDEX entries_state ON entries(state,plan)")
        }
        override fun onUpgrade(db: SQLiteDatabase, old: Int, new: Int) = Unit
    }.apply { setWriteAheadLoggingEnabled(true) } }
    private val revision = MutableStateFlow(0L)
    val changes = revision.asStateFlow()
    private val preferences get() = Core.context.getSharedPreferences("downloader", Context.MODE_PRIVATE)

    suspend fun create(url: String, wifi: Boolean, charging: Boolean): JSONObject {
        val plan = JSONObject().put("id", UUID.randomUUID().toString()).put("url", url)
            .put("wifi", wifi).put("charging", charging).put("status", "queued").put("title", url)
        save(plan)
        dismissReport()
        return plan
    }

    suspend fun load(id: String): JSONObject = withContext(Dispatchers.IO) {
        val db = helper.readableDatabase
        val plan = db.rawQuery("SELECT data FROM plans WHERE id=?", arrayOf(id)).use {
            check(it.moveToFirst()) { "Download task not found" }; JSONObject(it.getString(0))
        }
        val entries = JSONArray()
        db.rawQuery("SELECT data FROM entries WHERE plan=? ORDER BY position", arrayOf(id)).use {
            while (it.moveToNext()) entries.put(JSONObject(it.getString(0)))
        }
        if (plan.optBoolean("prepared")) plan.put("entries", entries)
        plan.put("duplicates", JSONArray(entries.objects().mapNotNull { it.optJSONObject("duplicate") }))
        plan
    }

    suspend fun save(plan: JSONObject, entry: JSONObject? = null) = withContext(Dispatchers.IO) {
        val db = helper.writableDatabase
        val id = plan.getString("id")
        val data = JSONObject().apply { plan.keys().forEach { if (it != "entries" && it != "duplicates") put(it, plan.get(it)) } }
        db.beginTransaction()
        try {
            db.execSQL("INSERT OR IGNORE INTO plans(id,status,data,created) VALUES(?,?,?,?)",
                arrayOf(id, plan.getString("status"), data.toString(), System.currentTimeMillis()))
            db.execSQL("UPDATE plans SET status=?,data=? WHERE id=?", arrayOf(plan.getString("status"), data.toString(), id))
            val entries = if (entry != null) listOf(entry) else plan.optJSONArray("entries")?.objects().orEmpty()
            for (item in entries) db.execSQL("INSERT OR REPLACE INTO entries(plan,position,state,data) VALUES(?,?,?,?)",
                arrayOf(id, item.getInt("index"), item.optString("state", "pending"), item.toString()))
            if (entry == null && plan.getString("status") == "finished") db.execSQL(
                "DELETE FROM plans WHERE id IN (SELECT id FROM plans p WHERE status='finished' AND NOT EXISTS(SELECT 1 FROM entries e WHERE e.plan=p.id AND e.state!='done') ORDER BY created DESC LIMIT -1 OFFSET 20)")
            db.setTransactionSuccessful()
        } finally { db.endTransaction() }
        revision.update { it + 1 }
    }

    suspend fun pending(): JSONObject? = withContext(Dispatchers.IO) {
        val id = helper.readableDatabase.rawQuery("SELECT id FROM plans WHERE status='awaiting' ORDER BY created LIMIT 1", null).use {
            if (it.moveToFirst()) it.getString(0) else null
        }
        id?.let { load(it) }
    }

    suspend fun failures(): List<FailedDownload> = withContext(Dispatchers.IO) {
        val result = mutableListOf<FailedDownload>()
        helper.readableDatabase.rawQuery("SELECT p.id,p.data,COUNT(e.position) FROM plans p LEFT JOIN entries e ON e.plan=p.id AND e.state='error' WHERE p.status IN ('finished','cancelled','failed') GROUP BY p.id HAVING COUNT(e.position)>0 OR p.status='failed' ORDER BY p.created", null).use {
            while (it.moveToNext()) result += FailedDownload(it.getString(0), JSONObject(it.getString(1)).optString("title"), it.getInt(2).coerceAtLeast(1))
        }
        result
    }

    suspend fun resolve(id: String, copies: Set<Int>): JSONObject {
        val plan = load(id)
        check(plan.getString("status") == "awaiting") { "No pending duplicate choices" }
        for (entry in plan.getJSONArray("entries").objects()) entry.put("copy", entry.getInt("index") in copies)
        plan.put("status", "queued").put("decided", true)
        save(plan)
        return plan
    }

    suspend fun retry(id: String): JSONObject {
        val plan = load(id)
        check(plan.optString("status") in setOf("finished", "cancelled", "failed")) { "Download is already queued" }
        for (entry in plan.optJSONArray("entries")?.objects().orEmpty()) if (entry.optString("state") == "error") {
            entry.put("state", "pending").put("attempts", 0).remove("error")
        }
        plan.put("status", "queued").remove("fatal_error")
        save(plan)
        dismissReport()
        return plan
    }

    suspend fun cancel(id: String) { val plan = load(id); plan.put("status", "cancelled"); save(plan) }
    suspend fun cancelActive() = withContext(Dispatchers.IO) {
        val ids = mutableListOf<String>()
        helper.readableDatabase.rawQuery("SELECT id FROM plans WHERE status IN ('queued','running','awaiting')", null).use {
            while (it.moveToNext()) ids += it.getString(0)
        }
        for (id in ids) cancel(id)
    }
    suspend fun delete(id: String) = withContext(Dispatchers.IO) {
        helper.writableDatabase.delete("plans", "id=?", arrayOf(id)); revision.update { it + 1 }
    }
    fun report(message: String, details: String) {
        preferences.edit().putString("report", message).putString("details", utf8Excerpt(details)).apply()
        revision.update { it + 1 }
    }
    fun dismissReport() {
        preferences.edit().remove("report").remove("details").apply()
        revision.update { it + 1 }
    }
}
