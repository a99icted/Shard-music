@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package music.shard.mobile

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.media3.common.Player
import coil.compose.AsyncImage
import kotlinx.coroutines.delay
import org.json.JSONObject
import java.io.File

@Composable
fun ShardApp(model: LibraryModel, onNotify: () -> Unit) {
    val snapshot = model.library
    val en = snapshot?.preferences?.optString("language") == "english"
    val t: (String, String) -> String = { ru, eng -> if (en) eng else ru }
    val accent = when (snapshot?.preferences?.optString("theme")) {
        "onyx" -> 0xffced0e0; "pick_me" -> 0xfff2a8ce; "cosmic" -> 0xffb3a2ff; else -> 0xffb5ed80
    }
    val bg = when (snapshot?.preferences?.optString("theme")) {
        "onyx" -> 0xff090a0c; "pick_me" -> 0xff20141d; "cosmic" -> 0xff0c0e20; else -> 0xff101412
    }
    MaterialTheme(colorScheme = darkColorScheme(primary = Color(accent), background = Color(bg), surface = Color(bg),
        surfaceVariant = Color(0xff242829), secondaryContainer = Color(accent).copy(alpha = 0.16f),
        onSecondaryContainer = Color(accent), onPrimary = Color(0xff101412)),
        shapes = Shapes(small = RoundedCornerShape(12.dp), medium = RoundedCornerShape(20.dp), large = RoundedCornerShape(28.dp))) {
        val context = LocalContext.current
        var tab by rememberSaveableState(0)
        var libraryTab by rememberSaveableState(0)
        var settingsTab by rememberSaveableState(0)
        var playlist by remember { mutableStateOf<Long?>(null) }
        var query by remember { mutableStateOf("") }
        var addMusic by remember { mutableStateOf(false) }
        var createPlaylist by remember { mutableStateOf(false) }
        var editPlaylist by remember { mutableStateOf<Playlist?>(null) }
        var editTrack by remember { mutableStateOf<Track?>(null) }
        var choosePlaylist by remember { mutableStateOf<Track?>(null) }
        var confirm by remember { mutableStateOf<Pair<String, () -> Unit>?>(null) }
        var showPlayer by remember { mutableStateOf(false) }
        var imageTarget by remember { mutableStateOf("background_set" to 0L) }
        val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { if (it.isNotEmpty()) model.importFiles(it, playlist) }
        val imagePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> uri?.let { model.image(it, imageTarget.first, imageTarget.second) } }
        val sync by SyncService.state.collectAsStateWithLifecycle()
        val syncError by SyncService.error.collectAsStateWithLifecycle()
        val shared by SharedLink.url.collectAsStateWithLifecycle()
        val playerRequested by PlayerNavigation.requested.collectAsStateWithLifecycle()
        LaunchedEffect(playerRequested, model.controller) {
            if (playerRequested && model.controller != null) {
                showPlayer = model.controller?.currentMediaItem != null
                PlayerNavigation.requested.value = false
            }
        }
        LaunchedEffect(shared) { if (shared != null) addMusic = true }
        LaunchedEffect(sync.optLong("revision")) { if (sync.optLong("revision") > 0) model.refresh() }
        LaunchedEffect(syncError) { syncError?.let { model.message = it; SyncService.error.value = null } }
        BackHandler(playlist != null || tab != 0 || libraryTab != 0) {
            when { playlist != null -> playlist = null; tab != 0 -> tab = 0; else -> libraryTab = 0 }
        }
        val title = when {
            playlist == -1L -> t("Избранное", "Favorites")
            playlist != null -> snapshot?.playlists?.find { it.id == playlist }?.name.orEmpty()
            tab == 1 -> t("Настройки", "Settings")
            else -> t("Моя музыка", "Your library")
        }
        Scaffold(
            topBar = {
                TopAppBar(title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.Bold) },
                    navigationIcon = { if (playlist != null) IconButton(onClick = { playlist = null }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, t("Назад", "Back")) } },
                    actions = {
                        if (playlist != null && playlist != -1L) IconButton(onClick = { editPlaylist = snapshot?.playlists?.find { it.id == playlist } }) { Icon(Icons.Default.Edit, t("Оформление плейлиста", "Edit playlist")) }
                        if (tab == 0) FilledTonalIconButton(onClick = { addMusic = true }, modifier = Modifier.padding(end = 16.dp)) { Icon(Icons.Default.Add, t("Добавить музыку", "Add music")) }
                    })
            },
            bottomBar = {
                Column {
                    if (model.downloadMessage.isNotBlank() && tab == 0) Surface(
                        onClick = { playlist = null; settingsTab = 1; tab = 1 },
                        color = MaterialTheme.colorScheme.secondaryContainer,
                        shape = RoundedCornerShape(16.dp), modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp)) {
                        Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            if (model.downloadActive) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                            else Icon(Icons.Default.Download, null, Modifier.size(20.dp))
                            Text(model.downloadMessage, Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall)
                            Icon(Icons.Default.ChevronRight, t("Открыть загрузки", "Open downloads"))
                        }
                    }
                    MiniPlayer(model, t, !showPlayer, onNotify) { showPlayer = true }
                    NavigationBar(containerColor = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
                        listOf(Icons.Default.LibraryMusic to t("Библиотека", "Library"), Icons.Default.Settings to t("Настройки", "Settings")).forEachIndexed { index, pair ->
                            NavigationBarItem(selected = tab == index, onClick = { tab = index; playlist = null; query = ""; if (index == 1) model.refresh() },
                                icon = { Icon(pair.first, pair.second) }, label = { Text(pair.second, maxLines = 1) })
                        }
                    }
                }
            }
        ) { padding ->
            Box(Modifier.fillMaxSize().padding(padding)) {
                snapshot?.background(playlist)?.let { AsyncImage(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop, alpha = 0.18f) }
                if (snapshot == null) {
                    Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                        CircularProgressIndicator(); Text(t("Открываем библиотеку…", "Opening library…"), Modifier.padding(16.dp))
                    }
                } else when {
                    tab == 1 && playlist == null -> SettingsScreen(model, snapshot, sync, t, onNotify, settingsTab, { settingsTab = it }) { scope -> imageTarget = "background_set" to scope; imagePicker.launch(arrayOf("image/*")) }
                    playlist == null && libraryTab == 0 -> LibraryHome(snapshot, t, libraryTab,
                        selectTab = { libraryTab = it; query = "" }, open = { playlist = it },
                        allTracks = { libraryTab = 2 }, create = { createPlaylist = true }, add = { addMusic = true })
                    else -> {
                        val scope = playlist ?: if (libraryTab == 1) -1L else null
                        val visible = remember(snapshot, scope, query) { snapshot.tracksIn(scope).filter { query.isBlank() || "${it.title} ${it.artist}".contains(query, ignoreCase = true) } }
                        Column {
                            if (playlist == null) LibraryTabs(libraryTab, t) { libraryTab = it; query = "" }
                            snapshot.playlists.find { it.id == playlist }?.let { p ->
                                Row(Modifier.fillMaxWidth().background(p.color?.let { Color(it or 0xff000000) } ?: MaterialTheme.colorScheme.surfaceVariant).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                    if (p.cover != null) AsyncImage(File(Core.root, p.cover), t("Обложка", "Cover"), Modifier.size(64.dp), contentScale = ContentScale.Crop)
                                    else Icon(Icons.Default.Album, null, Modifier.size(48.dp))
                                    Text(p.name, Modifier.padding(start = 16.dp), style = MaterialTheme.typography.titleLarge, maxLines = 2)
                                }
                            }
                            OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp), singleLine = true,
                                placeholder = { Text(t("Название или исполнитель", "Title or artist")) }, leadingIcon = { Icon(Icons.Default.Search, null) })
                            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Button(onClick = { visible.firstOrNull()?.let { onNotify(); model.play(visible, it.id, false) } }, enabled = visible.isNotEmpty(), modifier = Modifier.weight(1f)) { Icon(Icons.Default.PlayArrow, null); Spacer(Modifier.width(6.dp)); Text(t("Слушать", "Play")) }
                                FilledTonalButton(onClick = { visible.randomOrNull()?.let { onNotify(); model.play(visible, it.id, true) } }, enabled = visible.isNotEmpty()) { Icon(Icons.Default.Shuffle, t("Перемешать", "Shuffle")) }
                                OutlinedIconButton(onClick = { onNotify(); SyncService.start(context, if (scope == null) "library" else "playlist", scope ?: 0) }) { Icon(Icons.Default.Sync, t("Отправить на устройство", "Send to device")) }
                            }
                            Text("${visible.size} ${t("треков", "tracks")}", Modifier.padding(start = 20.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            if (visible.isEmpty()) {
                                Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                                    Icon(Icons.Default.Headphones, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary)
                                    Text(t("Ваша музыка. На вашем устройстве.", "Your music. On your device."), Modifier.padding(vertical = 12.dp))
                                    Text(t("Добавьте файлы, вставьте ссылку или передайте музыку с ПК через синхронизацию.", "Import files, paste a link, or transfer music from your PC using LAN sync."), style = MaterialTheme.typography.bodyMedium)
                                    FilledTonalButton(onClick = { importer.launch(arrayOf("audio/*", "video/mp4")) }) { Text(t("Выбрать файлы", "Choose files")) }
                                }
                            }
                            LazyColumn(Modifier.weight(1f)) {
                                items(visible, key = { it.id }) { track ->
                                    TrackRow(track, track.id in snapshot.favorites, t,
                                        onPlay = { onNotify(); model.play(visible, track.id) },
                                        onFavorite = { model.toggleFavorite(track.id) },
                                        onQueue = { model.controller?.addMediaItem(track.mediaItem()) },
                                        onEdit = { editTrack = track }, onAdd = { choosePlaylist = track },
                                        onSend = { onNotify(); SyncService.start(context, "track", track.id) },
                                        onRemove = if (scope != null) { {
                                            if (scope == -1L) model.command("favorite", "id" to track.id, "enabled" to false)
                                            else model.command("playlist_remove", "id" to scope, "track" to track.id)
                                        } } else null,
                                        removeLabel = if (scope == -1L) t("Убрать из избранного", "Remove from favorites") else t("Убрать из плейлиста", "Remove from playlist"),
                                        onDelete = { confirm = t("Удалить трек и его локальный файл?", "Delete the track and its local file?") to { model.deleteTrack(track.id) } })
                                }
                            }
                        }
                    }
                }
                if (model.busy > 0) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter))
            }
        }
        if (addMusic) DownloadDialog(model, t, shared, onNotify,
            onFiles = { addMusic = false; importer.launch(arrayOf("audio/*", "video/mp4")) }, onDismiss = { addMusic = false; SharedLink.url.value = null })
        if (createPlaylist) TextDialog(t("Новый плейлист", "New playlist"), t("Название", "Name"), "", t, { createPlaylist = false }) { name -> model.command("playlist_create", "name" to name); createPlaylist = false }
        editTrack?.let { track -> TrackEditDialog(track, t, { editTrack = null }) { titleValue, artist -> model.command("track_edit", "id" to track.id, "title" to titleValue, "artist" to artist); editTrack = null } }
        choosePlaylist?.let { track ->
            AlertDialog(onDismissRequest = { choosePlaylist = null }, title = { Text(t("Добавить в плейлист", "Add to playlist")) }, text = {
                LazyColumn { items(snapshot?.playlists.orEmpty(), key = { it.id }) { p -> OutlinedButton(onClick = { model.command("playlist_add", "id" to p.id, "track" to track.id); choosePlaylist = null }) { Text(p.name) } }
                    if (snapshot?.playlists.isNullOrEmpty()) item { Text(t("Сначала создайте плейлист во вкладке «Плейлисты».", "Create a playlist in the Playlists tab first.")) } }
            }, confirmButton = { Button(onClick = { choosePlaylist = null }) { Text(t("Закрыть", "Close")) } })
        }
        editPlaylist?.let { p ->
            PlaylistEditDialog(p, snapshot!!, model, t, { editPlaylist = null },
                image = { op -> imageTarget = op to p.id; imagePicker.launch(arrayOf("image/*")) },
                delete = { confirm = t("Удалить плейлист? Треки останутся в библиотеке.", "Delete playlist? Tracks will stay in the library.") to {
                    model.command("playlist_delete", "id" to p.id); editPlaylist = null; playlist = null
                } })
        }
        confirm?.let { pair -> AlertDialog(onDismissRequest = { confirm = null }, text = { Text(pair.first) },
            confirmButton = { Button(onClick = { pair.second(); confirm = null }) { Text(t("Удалить", "Delete")) } },
            dismissButton = { OutlinedButton(onClick = { confirm = null }) { Text(t("Отмена", "Cancel")) } }) }
        model.message?.let { message -> AlertDialog(onDismissRequest = { model.message = null }, title = { Text("Shard") }, text = { LazyColumn { item { Text(message) } } }, confirmButton = { Button(onClick = { model.message = null }) { Text("OK") } }) }
        if (showPlayer) PlayerSheet(model, t, onNotify) { showPlayer = false }
        val duplicates = sync.optJSONArray("duplicates")
        var duplicateCopies by remember(duplicates?.toString()) { mutableStateOf(emptySet<Int>()) }
        if (duplicates != null && duplicates.length() > 0) AlertDialog(onDismissRequest = { SyncService.stop(context) },
            title = { Text(t("Найдены дубликаты", "Duplicates found")) },
            text = { Column {
                Text(t("Отметьте треки, для которых нужна отдельная копия. Остальные будут пропущены.", "Select tracks to copy again. The others will be reused."))
                LazyColumn { items(duplicates.objects()) { entry ->
                    val index = entry.getInt("index")
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(index in duplicateCopies, { duplicateCopies = if (it) duplicateCopies + index else duplicateCopies - index })
                        Text("${entry.optString("artist")} — ${entry.optString("title")}", Modifier.padding(vertical = 4.dp))
                    }
                } }
            } },
            confirmButton = { Button(onClick = { model.command("sync_decide", "choices" to org.json.JSONArray().apply { duplicateCopies.forEach { put(JSONObject().put("index", it).put("copy", true)) } }) }) { Text(t("Применить", "Apply")) } },
            dismissButton = { OutlinedButton(onClick = { model.command("sync_decide", "copy" to true) }) { Text(t("Создать копии", "Copy all")) } })
    }
}

@Composable private fun rememberSaveableState(initial: Int) = androidx.compose.runtime.saveable.rememberSaveable { mutableIntStateOf(initial) }

@Composable private fun TrackRow(track: Track, favorite: Boolean, t: (String, String) -> String, onPlay: () -> Unit, onFavorite: () -> Unit,
    onQueue: () -> Unit, onEdit: () -> Unit, onAdd: () -> Unit, onSend: () -> Unit, onRemove: (() -> Unit)?, removeLabel: String, onDelete: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clickable(onClick = onPlay).padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f).padding(vertical = 12.dp)) {
            Text(track.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(track.artist.ifBlank { "—" }, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onClick = onFavorite) { Icon(if (favorite) Icons.Default.Favorite else Icons.Default.FavoriteBorder, t("Избранное", "Favorite"), tint = if (favorite) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant) }
        Box {
            IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, t("Действия с треком", "Track actions")) }
            DropdownMenu(menu, { menu = false }) {
                listOf(t("В очередь", "Add to queue") to onQueue, t("В плейлист", "Add to playlist") to onAdd,
                    t("Редактировать", "Edit metadata") to onEdit, t("Отправить на устройство", "Send to device") to onSend).forEach { (label, action) ->
                    DropdownMenuItem(text = { Text(label) }, onClick = { menu = false; action() })
                }
                onRemove?.let { DropdownMenuItem(text = { Text(removeLabel) }, onClick = { menu = false; it() }) }
                DropdownMenuItem(text = { Text(t("Удалить из библиотеки", "Delete from library")) }, onClick = { menu = false; onDelete() })
            }
        }
    }
}

// Sample only while this UI is visible and music is playing. Player events also
// refresh the position immediately after seeking, pausing or changing tracks.
@Composable private fun playbackPosition(player: Player, revision: Int, active: Boolean = true): Long {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var position by remember(player) { mutableLongStateOf(player.currentPosition) }
    LaunchedEffect(player, revision, lifecycle, active) {
        if (active) lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            do {
                position = player.currentPosition
                if (!player.isPlaying) break
                delay(1000)
            } while (true)
        }
    }
    return position
}

@Composable private fun MiniPlayer(model: LibraryModel, t: (String, String) -> String, active: Boolean, notify: () -> Unit, open: () -> Unit) {
    val revision = model.playerRevision
    val player = model.controller ?: return
    val media = player.currentMediaItem ?: return
    val position = playbackPosition(player, revision, active)
    val fraction = if (player.duration > 0) (position.toFloat() / player.duration).coerceIn(0f, 1f) else 0f
    Surface(color = MaterialTheme.colorScheme.surfaceVariant, shape = RoundedCornerShape(28.dp),
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp)) {
        Box {
            Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { if (player.isPlaying) player.pause() else { notify(); player.play() } }) {
                    Icon(if (player.isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                        if (player.isPlaying) t("Приостановить", "Pause") else t("Продолжить", "Resume"),
                        Modifier.size(30.dp), tint = MaterialTheme.colorScheme.primary)
                }
                Text(buildAnnotatedString {
                    withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(media.mediaMetadata.title?.toString().orEmpty()) }
                    media.mediaMetadata.artist?.toString()?.takeIf { it.isNotBlank() }?.let { artist ->
                        withStyle(SpanStyle(color = MaterialTheme.colorScheme.onSurfaceVariant)) { append(" — $artist") }
                    }
                }, Modifier.weight(1f).clickable(onClickLabel = t("Открыть плеер", "Open player"), onClick = open).padding(vertical = 12.dp),
                    maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyLarge)
                val currentId = media.mediaId.toLongOrNull()
                val favorite = currentId in model.library?.favorites.orEmpty()
                IconToggleButton(checked = favorite, onCheckedChange = { currentId?.let { model.toggleFavorite(it) } }, enabled = currentId != null) {
                    Icon(if (favorite) Icons.Default.Favorite else Icons.Default.FavoriteBorder,
                        if (favorite) t("Убрать из избранного", "Unlike track") else t("В избранное", "Like track"),
                        tint = if (favorite) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
                }
                IconButton(onClick = { player.stop(); player.clearMediaItems() }) {
                    Icon(Icons.Default.Close, t("Закрыть плеер", "Close player"), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Box(Modifier.fillMaxWidth().height(2.dp).align(Alignment.BottomCenter)
                .semantics { progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f) }
                .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f))) {
                Box(Modifier.fillMaxWidth(fraction).fillMaxHeight().background(MaterialTheme.colorScheme.primary))
            }
        }
    }
}

@Composable private fun PlayerSheet(model: LibraryModel, t: (String, String) -> String, notify: () -> Unit, dismiss: () -> Unit) {
    val revision = model.playerRevision
    val player = model.controller ?: return
    val sampledPosition = playbackPosition(player, revision)
    var seekPosition by remember(player.currentMediaItem?.mediaId) { mutableLongStateOf(player.currentPosition) }
    var dragging by remember { mutableStateOf(false) }
    var queue by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    if (player.currentMediaItem == null) { LaunchedEffect(Unit) { dismiss() }; return }
    val position = if (dragging) seekPosition else sampledPosition
    val repeat = player.repeatMode
    val repeatLabel = when (repeat) { Player.REPEAT_MODE_ALL -> t("Повтор: все", "Repeat all"); Player.REPEAT_MODE_ONE -> t("Повтор: один", "Repeat one"); else -> t("Без повтора", "Repeat off") }
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surfaceVariant,
        dragHandle = { Box(Modifier.padding(top = 10.dp, bottom = 18.dp).size(width = 32.dp, height = 3.dp)
            .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f), CircleShape)) }) {
        SheetSystemBars()
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).navigationBarsPadding()) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                Column(Modifier.weight(1f)) {
                    Text(player.mediaMetadata.title?.toString().orEmpty(), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Spacer(Modifier.height(4.dp))
                    Text(player.mediaMetadata.artist?.toString().orEmpty(), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Box(Modifier.size(56.dp).clip(RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)), contentAlignment = Alignment.Center) {
                    Icon(Icons.Default.MusicNote, null, Modifier.size(32.dp), tint = MaterialTheme.colorScheme.primary)
                }
            }
            Spacer(Modifier.height(12.dp))
            val duration = player.duration.coerceAtLeast(1)
            Slider(position.toFloat().coerceIn(0f, duration.toFloat()), { dragging = true; seekPosition = it.toLong() },
                modifier = Modifier.fillMaxWidth().semantics { contentDescription = t("Позиция воспроизведения", "Playback position") },
                valueRange = 0f..duration.toFloat(), enabled = player.duration > 0 && player.isCurrentMediaItemSeekable,
                thumb = { Box(Modifier.size(16.dp).background(MaterialTheme.colorScheme.primary, CircleShape)) },
                track = { state -> SliderDefaults.Track(state, Modifier.height(4.dp), thumbTrackGapSize = 0.dp, drawStopIndicator = null) },
                onValueChangeFinished = { player.seekTo(seekPosition); dragging = false })
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text(time(position), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(time(player.duration.coerceAtLeast(0)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { player.repeatMode = nextRepeatMode(repeat) }) {
                    Icon(if (repeat == Player.REPEAT_MODE_ONE) Icons.Default.RepeatOne else Icons.Default.Repeat, repeatLabel,
                        Modifier.size(26.dp), tint = if (repeat == Player.REPEAT_MODE_OFF) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary)
                }
                IconButton(onClick = { if (player.currentPosition > 3000) player.seekTo(0) else player.seekToPreviousMediaItem() }) { Icon(Icons.Default.SkipPrevious, t("Предыдущий", "Previous"), Modifier.size(32.dp)) }
                IconButton(onClick = { if (player.isPlaying) player.pause() else { notify(); player.play() } }, Modifier.size(64.dp)) {
                    Icon(if (player.isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                        if (player.isPlaying) t("Приостановить", "Pause") else t("Продолжить", "Resume"), Modifier.size(48.dp))
                }
                IconButton(onClick = { player.seekToNextMediaItem() }) { Icon(Icons.Default.SkipNext, t("Следующий", "Next"), Modifier.size(32.dp)) }
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, t("Настройки воспроизведения", "Playback options")) }
                    DropdownMenu(menu, { menu = false }) {
                        DropdownMenuItem(text = { Text(if (player.shuffleModeEnabled) t("Перемешивание: вкл.", "Shuffle: on") else t("Перемешивание: выкл.", "Shuffle: off")) },
                            leadingIcon = { Icon(Icons.Default.Shuffle, null) },
                            trailingIcon = { if (player.shuffleModeEnabled) Icon(Icons.Default.Check, null) },
                            onClick = { player.shuffleModeEnabled = !player.shuffleModeEnabled; menu = false })
                        DropdownMenuItem(text = { Text(t("Очередь", "Queue")) }, leadingIcon = { Icon(Icons.AutoMirrored.Filled.QueueMusic, null) },
                            onClick = { queue = !queue; menu = false })
                    }
                }
            }
            Text(repeatLabel + if (player.shuffleModeEnabled) t(" · Вперемешку", " · Shuffle") else "",
                Modifier.fillMaxWidth().padding(bottom = 12.dp), style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            val currentId = player.currentMediaItem?.mediaId?.toLongOrNull()
            val favorite = currentId in model.library?.favorites.orEmpty()
            Button(onClick = { currentId?.let { model.toggleFavorite(it) } }, enabled = currentId != null,
                modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp), shape = RoundedCornerShape(28.dp)) {
                Icon(if (favorite) Icons.Default.Favorite else Icons.Default.FavoriteBorder, null)
                Spacer(Modifier.width(8.dp)); Text(if (favorite) t("В избранном", "Liked") else t("Добавить в избранное", "Add to favorites"))
            }
            Spacer(Modifier.height(8.dp))
            if (queue) LazyColumn(Modifier.heightIn(max = 240.dp)) {
                items(player.mediaItemCount, key = { it }) { index ->
                    val item = player.getMediaItemAt(index)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(item.mediaMetadata.title?.toString().orEmpty(), Modifier.weight(1f).clickable { player.seekTo(index, 0); player.play() }.padding(12.dp), maxLines = 1)
                        IconButton(onClick = { player.removeMediaItem(index) }) { Icon(Icons.Default.Close, t("Убрать из очереди", "Remove from queue")) }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}

private fun time(ms: Long): String = "%d:%02d".format(ms / 60000, (ms / 1000) % 60)

@Composable internal fun StatisticsScreen(snapshot: Snapshot, t: (String, String) -> String) {
    val top = snapshot.statistics.optJSONArray("top")?.objects().orEmpty()
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { SettingsCard(t("Время прослушивания", "Listening time"), icon = Icons.Default.Headphones) {
            listOf("month_ms" to t("Этот месяц", "This month"), "year_ms" to t("Этот год", "This year")).forEach { (key, label) ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text("${snapshot.statistics.optLong(key) / 60000} ${t("мин", "min")}", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                }
            }
        } }
        item { Text(t("Любимые треки", "Top tracks"), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold) }
        if (top.isEmpty()) item { Text(t("Здесь появятся треки, которые вы слушаете чаще всего.", "Your most played tracks will appear here."), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        items(top) { track ->
            Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)) {
                Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Icon(Icons.Default.MusicNote, null, tint = MaterialTheme.colorScheme.primary)
                    Column(Modifier.weight(1f)) {
                        Text(track.optString("title"), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(track.optString("artist"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Text(track.optLong("plays").toString(), fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

@Composable private fun TextDialog(title: String, label: String, initial: String, t: (String, String) -> String, dismiss: () -> Unit, save: (String) -> Unit) {
    var value by remember { mutableStateOf(initial) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(title) }, text = { OutlinedTextField(value, { value = it }, label = { Text(label) }, singleLine = true) },
        confirmButton = { Button(onClick = { save(value) }, enabled = value.isNotBlank()) { Text(t("Сохранить", "Save")) } }, dismissButton = { OutlinedButton(onClick = dismiss) { Text(t("Отмена", "Cancel")) } })
}

@Composable private fun TrackEditDialog(track: Track, t: (String, String) -> String, dismiss: () -> Unit, save: (String, String) -> Unit) {
    var title by remember { mutableStateOf(track.title) }
    var artist by remember { mutableStateOf(track.artist) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(t("Редактировать трек", "Edit track")) }, text = {
        Column { OutlinedTextField(title, { title = it }, label = { Text(t("Название", "Title")) }); OutlinedTextField(artist, { artist = it }, label = { Text(t("Исполнитель", "Artist")) }) }
    }, confirmButton = { Button(onClick = { save(title, artist) }, enabled = title.isNotBlank()) { Text(t("Сохранить", "Save")) } }, dismissButton = { OutlinedButton(onClick = dismiss) { Text(t("Отмена", "Cancel")) } })
}

@Composable private fun PlaylistEditDialog(p: Playlist, snapshot: Snapshot, model: LibraryModel, t: (String, String) -> String, dismiss: () -> Unit, image: (String) -> Unit, delete: () -> Unit) {
    var name by remember { mutableStateOf(p.name) }
    var color by remember { mutableStateOf(p.color?.let { "#%06X".format(it) }.orEmpty()) }
    val background = snapshot.backgrounds.find { it.getLong("scope") == p.id }
    var blur by remember { mutableFloatStateOf(background?.optInt("blur", 12)?.toFloat() ?: 12f) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(t("Оформление плейлиста", "Playlist appearance")) }, text = {
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item { OutlinedTextField(name, { name = it }, label = { Text(t("Название", "Name")) }) }
            item { OutlinedTextField(color, { color = it }, label = { Text(t("Цвет шапки: #RRGGBB", "Header color: #RRGGBB")) }, singleLine = true) }
            item { OutlinedButton(onClick = { image("cover_set") }) { Text(t("Выбрать обложку", "Choose cover")) } }
            item { OutlinedButton(onClick = { model.command("cover_remove", "id" to p.id) }) { Text(t("Убрать обложку", "Remove cover")) } }
            item { OutlinedButton(onClick = { image("background_set") }) { Text(t("Выбрать фон", "Choose background")) } }
            if (background != null) {
                item { Text("${t("Размытие", "Blur")}: ${blur.toInt()}"); Slider(blur, { blur = it }, valueRange = 0f..40f, onValueChangeFinished = { model.command("background_blur", "id" to p.id, "blur" to blur.toInt()) }) }
                item { OutlinedButton(onClick = { model.command("background_remove", "id" to p.id) }) { Text(t("Убрать фон", "Remove background")) } }
            }
            item { OutlinedButton(onClick = delete) { Text(t("Удалить плейлист", "Delete playlist"), color = MaterialTheme.colorScheme.error) } }
        }
    }, confirmButton = { Button(onClick = {
        model.perform { Core.call("playlist_rename", "id" to p.id, "name" to name.ifBlank { p.name }); Core.call("header_color", "id" to p.id, "color" to color) }; dismiss()
    }) { Text(t("Сохранить", "Save")) } }, dismissButton = { OutlinedButton(onClick = dismiss) { Text(t("Закрыть", "Close")) } })
}

@Composable private fun DownloadDialog(model: LibraryModel, t: (String, String) -> String, shared: String?, notify: () -> Unit, onFiles: () -> Unit, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val settings = remember { context.getSharedPreferences("mobile", Context.MODE_PRIVATE) }
    var url by remember { mutableStateOf(shared.orEmpty()) }
    var wifi by remember { mutableStateOf(settings.getBoolean("wifi_only", true)) }
    var charging by remember { mutableStateOf(settings.getBoolean("charging_only", false)) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        SheetSystemBars()
        Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(t("Добавить музыку", "Add music"), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            FilledTonalButton(onClick = onFiles, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                Icon(Icons.Default.FolderOpen, null); Spacer(Modifier.width(8.dp)); Text(t("Выбрать файлы на телефоне", "Choose files on your phone"))
            }
            HorizontalDivider()
            Text(t("Скачать по ссылке", "Download from a link"), style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(url, { url = it }, modifier = Modifier.fillMaxWidth(), label = { Text(t("Вставьте ссылку", "Paste a link")) }, supportingText = { Text("YouTube · Spotify · Яндекс Музыка") }, maxLines = 3)
            ToggleRow(t("Только Wi-Fi", "Unmetered network only"), wifi) { wifi = it; settings.edit().putBoolean("wifi_only", it).apply() }
            ToggleRow(t("Только на зарядке", "Only while charging"), charging) { charging = it; settings.edit().putBoolean("charging_only", it).apply() }
            Text(t("Загрузки идут по одной и ждут, если батарея разряжена.", "Downloads run one at a time and wait when the battery is low."), style = MaterialTheme.typography.bodySmall)
            Button(onClick = {
                model.perform(false) {
                    // Validation is local; do not fetch metadata twice just to enqueue work.
                    val uri = java.net.URI(url.trim())
                    require(uri.scheme == "https" && uri.host != null) { t("Нужна HTTPS-ссылка", "An HTTPS link is required") }
                    DownloadWorker.enqueue(context, url.trim(), wifi, charging)
                }
                notify(); onDismiss()
            }, enabled = url.isNotBlank(), modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                Icon(Icons.Default.Download, null); Spacer(Modifier.width(8.dp)); Text(t("Скачать музыку", "Download music"))
            }
        }
    }
}

@Composable private fun ToggleRow(label: String, checked: Boolean, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) { Text(label, Modifier.weight(1f)); Switch(checked, change) }
}

// The app's themes are dark even when Android itself uses a light theme.
@Composable private fun SheetSystemBars() {
    val view = LocalView.current
    SideEffect {
        (view.parent as? DialogWindowProvider)?.window?.let { window ->
            WindowCompat.getInsetsController(window, view).isAppearanceLightStatusBars = false
            WindowCompat.getInsetsController(window, view).isAppearanceLightNavigationBars = false
        }
    }
}
