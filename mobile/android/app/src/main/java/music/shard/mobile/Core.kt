package music.shard.mobile

import android.app.Application
import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

object NativeBridge {
    init { System.loadLibrary("shard_mobile") }
    @JvmStatic external fun initialize(root: String): String
    @JvmStatic external fun dispatch(request: String): String
}

class ShardApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        Core.context = this
        Notifications.createChannels(this)
    }
}

object Core {
    lateinit var context: Application
    val root: File get() = File(context.filesDir, "shard")
    private val initMutex = Mutex()
    private val favoriteMutex = Mutex()
    private val changes = MutableStateFlow(0L)
    val libraryChanges = changes.asStateFlow()
    @Volatile private var ready = false
    private suspend fun initialize() = initMutex.withLock {
        if (!ready) {
            decode(NativeBridge.initialize(root.absolutePath))
            ready = true
        }
    }
    suspend fun call(op: String, vararg args: Pair<String, Any?>): Any? = withContext(Dispatchers.IO) {
        initialize()
        val result = decode(NativeBridge.dispatch(JSONObject().put("op", op).apply {
            args.forEach { (key, value) -> put(key, value ?: JSONObject.NULL) }
        }.toString()))
        if (op == "favorite" || op == "preferences") changes.update { it + 1 }
        result
    }
    suspend fun toggleFavorite(id: Long) = favoriteMutex.withLock {
        call("favorite", "id" to id, "enabled" to (id !in snapshot().favorites))
    }
    private fun decode(response: String): Any? {
        val json = JSONObject(response)
        check(json.getBoolean("ok")) { json.optString("error", "Native operation failed") }
        return json.opt("value").takeUnless { it == JSONObject.NULL }
    }
    suspend fun snapshot() = withContext(Dispatchers.IO) { Snapshot(call("snapshot") as JSONObject) }

    /** SAF streams are copied sequentially with a bounded buffer; no storage permission. */
    suspend fun stage(uri: Uri, image: Boolean = false): File = withContext(Dispatchers.IO) {
        initialize()
        val resolver = context.contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }.orEmpty()
        val extension = name.substringAfterLast('.', "").lowercase().takeIf { it.matches(Regex("[a-z0-9]{1,8}")) }
            ?: if (image) "png" else when (resolver.getType(uri)) {
                "audio/mpeg" -> "mp3"; "audio/flac" -> "flac"; "audio/mp4" -> "m4a"
                "audio/ogg" -> "ogg"; "audio/wav", "audio/x-wav" -> "wav"; else -> error("Unsupported audio format")
            }
        val target = File(root, "staging/${UUID.randomUUID()}.$extension")
        try {
            resolver.openInputStream(uri).use { input ->
                requireNotNull(input) { "Cannot read selected file" }
                target.outputStream().buffered().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var total = 0L
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        total += count
                        check(total <= if (image) 25L * 1024 * 1024 else 2L * 1024 * 1024 * 1024) { "File is too large" }
                        output.write(buffer, 0, count)
                    }
                }
            }
            if (image) {
                // Decode a sampled image before Rust processes it. A 40 MP phone
                // photo must not allocate its full-resolution pixels twice.
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(target.absolutePath, bounds)
                require(bounds.outWidth > 0 && bounds.outHeight > 0) { "Unsupported image" }
                var sample = 1
                while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2048) sample *= 2
                val bitmap = BitmapFactory.decodeFile(target.absolutePath, BitmapFactory.Options().apply { inSampleSize = sample })
                    ?: error("Cannot decode image")
                val resized = File(root, "staging/${UUID.randomUUID()}.png")
                try {
                    resized.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) { "Cannot save image" } }
                    target.delete()
                    resized
                } catch (e: Exception) { resized.delete(); throw e } finally { bitmap.recycle() }
            } else target
        } catch (e: Exception) { target.delete(); throw e }
    }
}

fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }
fun JSONArray.ids(): List<Long> = (0 until length()).map { getLong(it) }

data class Track(val id: Long, val title: String, val artist: String, val path: String, val duration: Long) {
    constructor(j: JSONObject) : this(j.getLong("id"), j.getString("title"), j.getString("artist"), j.getString("file_path"), j.optLong("duration_ms"))
}
data class Playlist(val id: Long, val name: String, val count: Int, val cover: String?, val color: Long?) {
    constructor(j: JSONObject) : this(j.getLong("id"), j.getString("name"), j.getInt("track_count"), j.optString("cover_path").takeUnless { it == "null" || it.isEmpty() },
        if (!j.isNull("header_color")) j.getLong("header_color") else if (!j.isNull("cover_color")) j.getLong("cover_color") else null)
}
class Snapshot(val json: JSONObject) {
    val tracks = json.getJSONArray("tracks").objects().map(::Track)
    val playlists = json.getJSONArray("playlists").objects().map(::Playlist)
    val favorites = json.getJSONArray("favorites").ids().toSet()
    val preferences = json.getJSONObject("preferences")
    val statistics = json.getJSONObject("statistics")
    val backgrounds = json.getJSONArray("backgrounds").objects()
    fun tracksIn(id: Long?): List<Track> = when (id) {
        null -> tracks
        -1L -> tracks.filter { it.id in favorites }
        else -> {
            val byId = tracks.associateBy { it.id }
            (json.getJSONObject("members").optJSONArray(id.toString())?.ids() ?: emptyList()).mapNotNull(byId::get)
        }
    }
    fun background(id: Long?): File? {
        val found = backgrounds.find { it.getLong("scope") == id } ?: backgrounds.find { it.getLong("scope") == 0L }
        return found?.let { File(Core.root, it.getString("display_path")) }
    }
}
