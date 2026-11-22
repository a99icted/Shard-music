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
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
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
    var downloadPending by mutableStateOf<JSONObject?>(null); private set
    var artworkBusy by mutableStateOf(false); private set
    internal var failedDownloads by mutableStateOf(emptyList<FailedDownload>()); private set
    private val listener = object : Player.Listener {
        override fun onEvents(player: Player, events: Player.Events) { playerRevision++ }
        override fun onPlayerError(error: androidx.media3.common.PlaybackException) { message = error.localizedMessage }
    }
    init {
        viewModelScope.launch { Core.libraryChanges.collectLatest { refresh() } }
        viewModelScope.launch {
            var finishedCount = -1
            combine(WorkManager.getInstance(app).getWorkInfosForUniqueWorkFlow(DownloadWorker.CHAIN), DownloadStore.changes) { jobs, _ -> jobs }.collectLatest { jobs ->
                val active = jobs.firstOrNull { it.state == androidx.work.WorkInfo.State.RUNNING }
                    ?: jobs.firstOrNull { !it.state.isFinished }
                val preferences = app.getSharedPreferences("downloader", android.content.Context.MODE_PRIVATE)
                downloadActive = active != null
                downloadDetails = if (active != null) "" else preferences.getString("details", "").orEmpty()
                downloaderVersion = preferences.getString("version", "").orEmpty()
                downloadPending = DownloadStore.pending()
                failedDownloads = DownloadStore.failures()
                downloadMessage = active?.progress?.getString("message")
                    ?: if (active != null) {
                        val english = library?.preferences?.optString("language") == "english"
                        if (active.state == androidx.work.WorkInfo.State.RUNNING) {
                            if (english) "Preparing download…" else "Подготовка загрузки…"
                        } else if (english) "Waiting for network / charging" else "Ожидание сети / зарядки"
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
    fun toggleIgnored(id: Long) = perform {
        Core.call("ignored", "id" to id, "enabled" to (id !in Core.snapshot().ignored))
    }
    fun refreshArtwork() {
        if (artworkBusy) return
        artworkBusy = true
        perform {
            try {
                val count = Core.call("artwork_refresh") as Number
                message = if (library?.preferences?.optString("language") == "english") "Covers updated: $count" else "Обновлено обложек: $count"
            } finally { artworkBusy = false }
        }
    }
    fun deleteIgnored() = perform {
        // Snapshot IDs once: tracks ignored later are not part of this confirmation.
        val ids = Core.snapshot().ignored
        for (id in ids) Core.call("track_delete", "id" to id)
    }
    fun dismissDownloadReport() = DownloadStore.dismissReport()
    fun retryDownload(id: String) = perform(false) { DownloadWorker.enqueuePlan(getApplication(), DownloadStore.retry(id)) }
    fun resolveDownloads(id: String, copies: Set<Int>) = perform(false) { DownloadWorker.enqueuePlan(getApplication(), DownloadStore.resolve(id, copies)) }
    fun cancelPendingDownload(id: String) = perform(false) { DownloadStore.cancel(id); DownloadStore.dismissReport() }
    fun forgetDownload(id: String) = perform(false) { DownloadStore.delete(id) }
    fun cancelDownloads() = perform(false) {
        withContext(Dispatchers.IO) { WorkManager.getInstance(getApplication()).cancelUniqueWork(DownloadWorker.CHAIN).result.get() }
        DownloadStore.cancelActive()
    }
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
        val allowed = playbackSelection(tracks, library?.ignored.orEmpty(), selected)
        if (allowed.isEmpty()) return
        shuffle?.let { player.shuffleModeEnabled = it }
        player.setMediaItems(allowed.map { it.mediaItem(direct = it.id == selected) }, allowed.indexOfFirst { it.id == selected }.coerceAtLeast(0), 0)
        player.prepare(); player.play()
    }
    fun enqueue(track: Track) {
        if (track.id !in library?.ignored.orEmpty()) controller?.addMediaItem(track.mediaItem())
    }
    fun deleteTrack(id: Long) {
        controller?.let { p -> (p.mediaItemCount - 1 downTo 0).filter { p.getMediaItemAt(it).mediaId == id.toString() }.forEach { p.removeMediaItem(it) } }
        command("track_delete", "id" to id)
    }
    override fun onCleared() { controller?.removeListener(listener); super.onCleared() }
}
