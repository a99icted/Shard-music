package music.shard.mobile

import android.app.Application
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.work.WorkManager
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import org.json.JSONObject

class LibraryModel(app: Application) : AndroidViewModel(app) {
    var library by mutableStateOf<Snapshot?>(null); private set
    var message by mutableStateOf<String?>(null)
    var busy by mutableIntStateOf(0); private set
    var controller by mutableStateOf<MediaController?>(null)
    var playerRevision by mutableIntStateOf(0); private set
    var downloadMessage by mutableStateOf(""); private set
    var downloadDetails by mutableStateOf(""); private set
    var downloadActive by mutableStateOf(false); private set
    var downloaderVersion by mutableStateOf(""); private set
    private val listener = object : Player.Listener {
        override fun onEvents(player: Player, events: Player.Events) { playerRevision++ }
        override fun onPlayerError(error: androidx.media3.common.PlaybackException) { message = error.localizedMessage }
    }
    init {
        refresh()
        viewModelScope.launch { Core.libraryChanges.collectLatest { refresh() } }
        viewModelScope.launch {
            var finishedCount = -1
            WorkManager.getInstance(app).getWorkInfosForUniqueWorkFlow(DownloadWorker.CHAIN).collectLatest { jobs ->
                val active = jobs.firstOrNull { it.state == androidx.work.WorkInfo.State.RUNNING }
                    ?: jobs.firstOrNull { !it.state.isFinished }
                val preferences = app.getSharedPreferences("downloader", android.content.Context.MODE_PRIVATE)
                downloadActive = active != null
                downloadDetails = if (active != null) "" else preferences.getString("details", "").orEmpty()
                downloaderVersion = preferences.getString("version", "").orEmpty()
                downloadMessage = active?.progress?.getString("message")
                    ?: if (active != null) {
                        if (library?.preferences?.optString("language") == "english") "Waiting for network / charging" else "Ожидание сети / зарядки"
                    }
                    else preferences.getString("report", "").orEmpty()
                val count = jobs.count { it.state.isFinished }
                if (count != finishedCount) { finishedCount = count; refresh() }
            }
        }
    }
    fun bind(controller: MediaController) {
        this.controller?.removeListener(listener)
        this.controller = controller
        controller.addListener(listener)
        playerRevision++
    }
    fun perform(refresh: Boolean = true, block: suspend () -> Unit) {
        viewModelScope.launch {
            busy++
            try { block(); if (refresh) library = Core.snapshot() }
            catch (e: Exception) { if (e is CancellationException) throw e; message = e.message ?: e.toString() }
            finally { busy-- }
        }
    }
    fun refresh() = perform(false) { library = Core.snapshot() }
    fun command(op: String, vararg args: Pair<String, Any?>) = perform { Core.call(op, *args) }
    fun preferences(vararg args: Pair<String, Any?>) = command("preferences", "patch" to JSONObject().apply { args.forEach { put(it.first, it.second) } })
    fun toggleFavorite(id: Long) = perform(false) { Core.toggleFavorite(id) }
    fun importFiles(uris: List<Uri>, playlist: Long? = null) = perform {
        val errors = mutableListOf<String>()
        var count = 0
        for (uri in uris) {
            try {
                val staged = Core.stage(uri)
                try {
                    val track = Core.call("import", "path" to staged.absolutePath) as JSONObject
                    playlist?.let { Core.call("playlist_add", "id" to it, "track" to track.getLong("id")) }
                    count++
                } finally { staged.delete() }
            } catch (e: Exception) { if (e is CancellationException) throw e; errors += e.message.orEmpty() }
        }
        message = "Added / Добавлено: $count${if (errors.isEmpty()) "" else "\n" + errors.joinToString("\n")}"
    }
    fun image(uri: Uri, op: String, id: Long) = perform {
        val staged = Core.stage(uri, image = true)
        try { Core.call(op, "path" to staged.absolutePath, "id" to id) } finally { staged.delete() }
    }
    fun play(tracks: List<Track>, selected: Long, shuffle: Boolean? = null) {
        val player = controller ?: return
        if (tracks.isEmpty()) return
        shuffle?.let { player.shuffleModeEnabled = it }
        player.setMediaItems(tracks.map { it.mediaItem() }, tracks.indexOfFirst { it.id == selected }.coerceAtLeast(0), 0)
        player.prepare(); player.play()
    }
    fun deleteTrack(id: Long) {
        controller?.let { p -> (p.mediaItemCount - 1 downTo 0).filter { p.getMediaItemAt(it).mediaId == id.toString() }.forEach { p.removeMediaItem(it) } }
        command("track_delete", "id" to id)
    }
    override fun onCleared() { controller?.removeListener(listener); super.onCleared() }
}
