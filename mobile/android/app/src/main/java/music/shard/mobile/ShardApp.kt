@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package music.shard.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.media3.common.Player
import androidx.work.WorkManager
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
    MaterialTheme(colorScheme = darkColorScheme(primary = Color(accent), background = Color(bg), surface = Color(bg), onPrimary = Color(0xff101412))) {
        val context = LocalContext.current
        var tab by rememberSaveableState(0)
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
        LaunchedEffect(shared) { if (shared != null) addMusic = true }
        LaunchedEffect(sync.optLong("revision")) { if (sync.optLong("revision") > 0) model.refresh() }
        LaunchedEffect(syncError) { syncError?.let { model.message = it; SyncService.error.value = null } }
        BackHandler(playlist != null) { playlist = null }
        val title = when {
            playlist == -1L -> t("Избранное", "Favorites")
            playlist != null -> snapshot?.playlists?.find { it.id == playlist }?.name.orEmpty()
            tab == 1 -> t("Плейлисты", "Playlists")
            tab == 2 -> t("Статистика", "Statistics")
            tab == 3 -> t("Настройки", "Settings")
            else -> "Shard"
        }
        Scaffold(
            topBar = {
                TopAppBar(title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.Bold) },
                    navigationIcon = { if (playlist != null) IconButton(onClick = { playlist = null }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, t("Назад", "Back")) } },
                    actions = {
                        if (playlist != null && playlist != -1L) IconButton(onClick = { editPlaylist = snapshot?.playlists?.find { it.id == playlist } }) { Icon(Icons.Default.Edit, t("Оформление плейлиста", "Edit playlist")) }
                        IconButton(onClick = { model.refresh() }) { Icon(Icons.Default.Refresh, t("Обновить", "Refresh")) }
                        IconButton(onClick = { addMusic = true }) { Icon(Icons.Default.Add, t("Добавить музыку", "Add music")) }
                    })
            },
            bottomBar = {
                Column {
                    MiniPlayer(model, t) { showPlayer = true }
                    NavigationBar {
                        listOf(Icons.Default.LibraryMusic to t("Музыка", "Music"), Icons.Default.Album to t("Плейлисты", "Playlists"),
                            Icons.Default.BarChart to t("Статистика", "Statistics"), Icons.Default.Settings to t("Настройки", "Settings")).forEachIndexed { index, pair ->
                            NavigationBarItem(selected = tab == index, onClick = { tab = index; playlist = null; query = ""; if (index == 2) model.refresh() },
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
                    tab == 3 && playlist == null -> SettingsScreen(model, snapshot, sync, t, onNotify) { scope -> imageTarget = "background_set" to scope; imagePicker.launch(arrayOf("image/*")) }
                    tab == 2 && playlist == null -> StatisticsScreen(snapshot, t)
                    tab == 1 && playlist == null -> {
                        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            item { FilledTonalButton(onClick = { createPlaylist = true }, Modifier.fillMaxWidth()) { Icon(Icons.Default.Add, null); Text(t("Создать плейлист", "Create playlist")) } }
                            item { PlaylistRow(t("Избранное", "Favorites"), snapshot.favorites.size, null, Icons.Default.Favorite) { playlist = -1L } }
                            items(snapshot.playlists, key = { it.id }) { p -> PlaylistRow(p.name, p.count, p.cover, Icons.Default.Album) { playlist = p.id } }
                        }
                    }
                    else -> {
                        val visible = remember(snapshot, playlist, query) { snapshot.tracksIn(playlist).filter { query.isBlank() || "${it.title} ${it.artist}".contains(query, ignoreCase = true) } }
                        Column {
                            snapshot.playlists.find { it.id == playlist }?.let { p ->
                                Row(Modifier.fillMaxWidth().background(p.color?.let { Color(it or 0xff000000) } ?: MaterialTheme.colorScheme.surfaceVariant).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                    if (p.cover != null) AsyncImage(File(Core.root, p.cover), t("Обложка", "Cover"), Modifier.size(64.dp), contentScale = ContentScale.Crop)
                                    else Icon(Icons.Default.Album, null, Modifier.size(48.dp))
                                    Text(p.name, Modifier.padding(start = 16.dp), style = MaterialTheme.typography.titleLarge, maxLines = 2)
                                }
                            }
                            OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp), singleLine = true,
                                placeholder = { Text(t("Название или исполнитель", "Title or artist")) }, leadingIcon = { Icon(Icons.Default.Search, null) })
                            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text("${visible.size} ${t("треков", "tracks")}", Modifier.weight(1f), style = MaterialTheme.typography.labelLarge)
                                IconButton(onClick = { visible.firstOrNull()?.let { model.play(visible, it.id, false) } }, enabled = visible.isNotEmpty()) { Icon(Icons.Default.PlayArrow, t("Слушать", "Play")) }
                                IconButton(onClick = { visible.randomOrNull()?.let { model.play(visible, it.id, true) } }, enabled = visible.isNotEmpty()) { Icon(Icons.Default.Shuffle, t("Перемешать", "Shuffle")) }
                                IconButton(onClick = { onNotify(); SyncService.start(context, if (playlist == null) "library" else "playlist", playlist ?: 0) }) { Icon(Icons.Default.Sync, t("Отправить на устройство", "Send to device")) }
                            }
                            if (visible.isEmpty()) {
                                Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                                    Icon(Icons.Default.Headphones, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary)
                                    Text(t("Ваша музыка. На вашем устройстве.", "Your music. On your device."), Modifier.padding(vertical = 12.dp))
                                    Text(t("Добавьте файлы, вставьте ссылку или передайте музыку с ПК через синхронизацию.", "Import files, paste a link, or transfer music from your PC using LAN sync."), style = MaterialTheme.typography.bodyMedium)
                                    TextButton(onClick = { importer.launch(arrayOf("audio/*", "video/mp4")) }) { Text(t("Выбрать файлы", "Choose files")) }
                                }
                            }
                            LazyColumn(Modifier.weight(1f)) {
                                items(visible, key = { it.id }) { track ->
                                    TrackRow(track, track.id in snapshot.favorites, t,
                                        onPlay = { model.play(visible, track.id) },
                                        onFavorite = { model.command("favorite", "id" to track.id, "enabled" to (track.id !in snapshot.favorites)) },
                                        onQueue = { model.controller?.addMediaItem(track.mediaItem()) },
                                        onEdit = { editTrack = track }, onAdd = { choosePlaylist = track },
                                        onSend = { onNotify(); SyncService.start(context, "track", track.id) },
                                        onRemove = if (playlist != null) { { model.command("playlist_remove", "id" to playlist, "track" to track.id) } } else null,
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
                LazyColumn { items(snapshot?.playlists.orEmpty(), key = { it.id }) { p -> TextButton(onClick = { model.command("playlist_add", "id" to p.id, "track" to track.id); choosePlaylist = null }) { Text(p.name) } }
                    if (snapshot?.playlists.isNullOrEmpty()) item { Text(t("Сначала создайте плейлист во вкладке «Плейлисты».", "Create a playlist in the Playlists tab first.")) } }
            }, confirmButton = { TextButton(onClick = { choosePlaylist = null }) { Text(t("Закрыть", "Close")) } })
        }
        editPlaylist?.let { p ->
            PlaylistEditDialog(p, snapshot!!, model, t, { editPlaylist = null },
                image = { op -> imageTarget = op to p.id; imagePicker.launch(arrayOf("image/*")) },
                delete = { confirm = t("Удалить плейлист? Треки останутся в библиотеке.", "Delete playlist? Tracks will stay in the library.") to {
                    model.command("playlist_delete", "id" to p.id); editPlaylist = null; playlist = null
                } })
        }
        confirm?.let { pair -> AlertDialog(onDismissRequest = { confirm = null }, text = { Text(pair.first) },
            confirmButton = { TextButton(onClick = { pair.second(); confirm = null }) { Text(t("Удалить", "Delete")) } },
            dismissButton = { TextButton(onClick = { confirm = null }) { Text(t("Отмена", "Cancel")) } }) }
        model.message?.let { message -> AlertDialog(onDismissRequest = { model.message = null }, title = { Text("Shard") }, text = { LazyColumn { item { Text(message) } } }, confirmButton = { TextButton(onClick = { model.message = null }) { Text("OK") } }) }
        if (showPlayer) PlayerSheet(model, t) { showPlayer = false }
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
            confirmButton = { TextButton(onClick = { model.command("sync_decide", "choices" to org.json.JSONArray().apply { duplicateCopies.forEach { put(JSONObject().put("index", it).put("copy", true)) } }) }) { Text(t("Применить", "Apply")) } },
            dismissButton = { TextButton(onClick = { model.command("sync_decide", "copy" to true) }) { Text(t("Создать копии", "Copy all")) } })
    }
}

@Composable private fun rememberSaveableState(initial: Int) = androidx.compose.runtime.saveable.rememberSaveable { mutableIntStateOf(initial) }

@Composable private fun PlaylistRow(name: String, count: Int, cover: String?, icon: ImageVector, click: () -> Unit) {
    Card(onClick = click, Modifier.fillMaxWidth()) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            if (cover != null) AsyncImage(File(Core.root, cover), null, Modifier.size(48.dp), contentScale = ContentScale.Crop)
            else Icon(icon, null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
            Column(Modifier.padding(start = 16.dp)) { Text(name, fontWeight = FontWeight.SemiBold); Text(count.toString(), style = MaterialTheme.typography.labelMedium) }
        }
    }
}

@Composable private fun TrackRow(track: Track, favorite: Boolean, t: (String, String) -> String, onPlay: () -> Unit, onFavorite: () -> Unit,
    onQueue: () -> Unit, onEdit: () -> Unit, onAdd: () -> Unit, onSend: () -> Unit, onRemove: (() -> Unit)?, onDelete: () -> Unit) {
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
                onRemove?.let { DropdownMenuItem(text = { Text(t("Убрать из плейлиста", "Remove from playlist")) }, onClick = { menu = false; it() }) }
                DropdownMenuItem(text = { Text(t("Удалить из библиотеки", "Delete from library")) }, onClick = { menu = false; onDelete() })
            }
        }
    }
}

@Composable private fun MiniPlayer(model: LibraryModel, t: (String, String) -> String, open: () -> Unit) {
    val revision = model.playerRevision
    val player = model.controller ?: return
    val media = player.currentMediaItem ?: return
    Surface(tonalElevation = 8.dp, modifier = Modifier.fillMaxWidth().clickable(onClick = open)) {
        Row(Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) { Text(media.mediaMetadata.title?.toString().orEmpty(), maxLines = 1, overflow = TextOverflow.Ellipsis); Text(media.mediaMetadata.artist?.toString().orEmpty(), style = MaterialTheme.typography.bodySmall, maxLines = 1) }
            IconButton(onClick = { if (player.isPlaying) player.pause() else player.play() }) { Icon(if (player.isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow, t("Пауза / играть", "Pause / play")) }
            IconButton(onClick = { player.seekToNextMediaItem() }) { Icon(Icons.Default.SkipNext, t("Следующий", "Next")) }
        }
    }
}

@Composable private fun PlayerSheet(model: LibraryModel, t: (String, String) -> String, dismiss: () -> Unit) {
    val revision = model.playerRevision
    val player = model.controller ?: return
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var position by remember { mutableLongStateOf(player.currentPosition) }
    var dragging by remember { mutableStateOf(false) }
    var volume by remember { mutableFloatStateOf(player.volume) }
    var queue by remember { mutableStateOf(false) }
    LaunchedEffect(player, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) { if (!dragging) position = player.currentPosition; delay(1000) }
        }
    }
    ModalBottomSheet(onDismissRequest = dismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).navigationBarsPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(Icons.Default.MusicNote, null, Modifier.size(80.dp), tint = MaterialTheme.colorScheme.primary)
            Text(player.mediaMetadata.title?.toString().orEmpty(), style = MaterialTheme.typography.headlineSmall, maxLines = 2)
            Text(player.mediaMetadata.artist?.toString().orEmpty(), style = MaterialTheme.typography.bodyLarge)
            val duration = player.duration.coerceAtLeast(1)
            Slider(position.toFloat().coerceIn(0f, duration.toFloat()), { dragging = true; position = it.toLong() },
                valueRange = 0f..duration.toFloat(), onValueChangeFinished = { player.seekTo(position); dragging = false })
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text(time(position)); Text(time(player.duration.coerceAtLeast(0))) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { player.shuffleModeEnabled = !player.shuffleModeEnabled; model.preferences("shuffle" to player.shuffleModeEnabled) }) { Icon(Icons.Default.Shuffle, t("Перемешивание", "Shuffle"), tint = if (player.shuffleModeEnabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant) }
                IconButton(onClick = { if (player.currentPosition > 3000) player.seekTo(0) else player.seekToPreviousMediaItem() }) { Icon(Icons.Default.SkipPrevious, t("Предыдущий", "Previous")) }
                FilledIconButton(onClick = { if (player.isPlaying) player.pause() else player.play() }, Modifier.size(64.dp)) { Icon(if (player.isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow, t("Пауза / играть", "Pause / play"), Modifier.size(36.dp)) }
                IconButton(onClick = { player.seekToNextMediaItem() }) { Icon(Icons.Default.SkipNext, t("Следующий", "Next")) }
                IconButton(onClick = {
                    player.repeatMode = when (player.repeatMode) { Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL; Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE; else -> Player.REPEAT_MODE_OFF }
                    model.preferences("repeat" to when (player.repeatMode) { Player.REPEAT_MODE_ALL -> "all"; Player.REPEAT_MODE_ONE -> "one"; else -> "off" })
                }) { Icon(if (player.repeatMode == Player.REPEAT_MODE_ONE) Icons.Default.RepeatOne else Icons.Default.Repeat, t("Повтор", "Repeat"), tint = if (player.repeatMode != Player.REPEAT_MODE_OFF) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.AutoMirrored.Filled.VolumeUp, t("Громкость", "Volume"))
                Slider(volume, { volume = it; player.volume = it }, Modifier.weight(1f), onValueChangeFinished = { model.preferences("volume" to volume) })
                val id = player.currentMediaItem?.mediaId?.toLongOrNull()
                val favorite = id in model.library?.favorites.orEmpty()
                IconButton(onClick = { if (id != null) model.command("favorite", "id" to id, "enabled" to !favorite) }) { Icon(if (favorite) Icons.Default.Favorite else Icons.Default.FavoriteBorder, t("Избранное", "Favorite")) }
            }
            TextButton(onClick = { queue = !queue }) { Icon(Icons.AutoMirrored.Filled.QueueMusic, null); Text(t("Очередь", "Queue")) }
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

@Composable private fun StatisticsScreen(snapshot: Snapshot, t: (String, String) -> String) {
    LazyColumn(contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { Text(t("Время прослушивания", "Listening time"), style = MaterialTheme.typography.headlineSmall) }
        item { Text("${t("Этот месяц", "This month")}: ${snapshot.statistics.optLong("month_ms") / 60000} ${t("мин", "min")}", style = MaterialTheme.typography.titleLarge) }
        item { Text("${t("Этот год", "This year")}: ${snapshot.statistics.optLong("year_ms") / 60000} ${t("мин", "min")}", style = MaterialTheme.typography.titleLarge) }
        item { Text(t("Любимые треки", "Top tracks"), style = MaterialTheme.typography.titleMedium) }
        items(snapshot.statistics.getJSONArray("top").objects()) { track ->
            Text("${track.optString("title")}\n${track.optString("artist")} · ${track.optLong("plays")}")
        }
    }
}

@Composable private fun TextDialog(title: String, label: String, initial: String, t: (String, String) -> String, dismiss: () -> Unit, save: (String) -> Unit) {
    var value by remember { mutableStateOf(initial) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(title) }, text = { OutlinedTextField(value, { value = it }, label = { Text(label) }, singleLine = true) },
        confirmButton = { TextButton(onClick = { save(value) }, enabled = value.isNotBlank()) { Text(t("Сохранить", "Save")) } }, dismissButton = { TextButton(onClick = dismiss) { Text(t("Отмена", "Cancel")) } })
}

@Composable private fun TrackEditDialog(track: Track, t: (String, String) -> String, dismiss: () -> Unit, save: (String, String) -> Unit) {
    var title by remember { mutableStateOf(track.title) }
    var artist by remember { mutableStateOf(track.artist) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(t("Редактировать трек", "Edit track")) }, text = {
        Column { OutlinedTextField(title, { title = it }, label = { Text(t("Название", "Title")) }); OutlinedTextField(artist, { artist = it }, label = { Text(t("Исполнитель", "Artist")) }) }
    }, confirmButton = { TextButton(onClick = { save(title, artist) }, enabled = title.isNotBlank()) { Text(t("Сохранить", "Save")) } }, dismissButton = { TextButton(onClick = dismiss) { Text(t("Отмена", "Cancel")) } })
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
            item { TextButton(onClick = { image("cover_set") }) { Text(t("Выбрать обложку", "Choose cover")) } }
            item { TextButton(onClick = { model.command("cover_remove", "id" to p.id) }) { Text(t("Убрать обложку", "Remove cover")) } }
            item { TextButton(onClick = { image("background_set") }) { Text(t("Выбрать фон", "Choose background")) } }
            if (background != null) {
                item { Text("${t("Размытие", "Blur")}: ${blur.toInt()}"); Slider(blur, { blur = it }, valueRange = 0f..40f, onValueChangeFinished = { model.command("background_blur", "id" to p.id, "blur" to blur.toInt()) }) }
                item { TextButton(onClick = { model.command("background_remove", "id" to p.id) }) { Text(t("Убрать фон", "Remove background")) } }
            }
            item { TextButton(onClick = delete) { Text(t("Удалить плейлист", "Delete playlist"), color = MaterialTheme.colorScheme.error) } }
        }
    }, confirmButton = { TextButton(onClick = {
        model.perform { Core.call("playlist_rename", "id" to p.id, "name" to name.ifBlank { p.name }); Core.call("header_color", "id" to p.id, "color" to color) }; dismiss()
    }) { Text(t("Сохранить", "Save")) } }, dismissButton = { TextButton(onClick = dismiss) { Text(t("Закрыть", "Close")) } })
}

@Composable private fun DownloadDialog(model: LibraryModel, t: (String, String) -> String, shared: String?, notify: () -> Unit, onFiles: () -> Unit, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val settings = remember { context.getSharedPreferences("mobile", Context.MODE_PRIVATE) }
    var url by remember { mutableStateOf(shared.orEmpty()) }
    var wifi by remember { mutableStateOf(settings.getBoolean("wifi_only", true)) }
    var charging by remember { mutableStateOf(settings.getBoolean("charging_only", false)) }
    AlertDialog(onDismissRequest = onDismiss, title = { Text(t("Добавить музыку", "Add music")) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(url, { url = it }, label = { Text("YouTube / Spotify / Яндекс Музыка") }, maxLines = 3)
            ToggleRow(t("Только Wi-Fi", "Unmetered network only"), wifi) { wifi = it; settings.edit().putBoolean("wifi_only", it).apply() }
            ToggleRow(t("Только на зарядке", "Only while charging"), charging) { charging = it; settings.edit().putBoolean("charging_only", it).apply() }
            Text(t("Загрузки идут по одной и ждут, если батарея разряжена.", "Downloads run one at a time and wait when the battery is low."), style = MaterialTheme.typography.bodySmall)
            TextButton(onClick = onFiles) { Text(t("Выбрать локальные файлы", "Choose local files")) }
        }
    }, confirmButton = { TextButton(onClick = {
        model.perform(false) {
            // Validation is local; do not fetch metadata twice just to enqueue work.
            val uri = java.net.URI(url.trim())
            require(uri.scheme == "https" && uri.host != null) { t("Нужна HTTPS-ссылка", "An HTTPS link is required") }
            DownloadWorker.enqueue(context, url.trim(), wifi, charging)
        }
        notify(); onDismiss()
    }, enabled = url.isNotBlank()) { Text(t("Скачать", "Download")) } }, dismissButton = { TextButton(onClick = onDismiss) { Text(t("Отмена", "Cancel")) } })
}

@Composable private fun ToggleRow(label: String, checked: Boolean, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) { Text(label, Modifier.weight(1f)); Switch(checked, change) }
}

@Composable private fun SettingsScreen(model: LibraryModel, snapshot: Snapshot, sync: JSONObject, t: (String, String) -> String, notify: () -> Unit, image: (Long) -> Unit) {
    val context = LocalContext.current
    var code by remember { mutableStateOf("") }
    var deviceName by remember { mutableStateOf(snapshot.preferences.optString("sync_device_name")) }
    var globalBlur by remember { mutableFloatStateOf(snapshot.backgrounds.find { it.getLong("scope") == 0L }?.optInt("blur", 12)?.toFloat() ?: 12f) }
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { Text(t("Оформление", "Appearance"), style = MaterialTheme.typography.titleLarge) }
        item {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                listOf("forest" to t("Лес", "Forest"), "onyx" to t("Оникс", "Onyx"), "pick_me" to t("Пикми", "Pick me"), "cosmic" to t("Космос", "Cosmic")).forEach { (id, label) ->
                    FilterChip(selected = snapshot.preferences.optString("theme") == id, onClick = { model.preferences("theme" to id) }, label = { Text(label, style = MaterialTheme.typography.labelSmall) })
                }
            }
        }
        item { Row { TextButton(onClick = { model.preferences("language" to "russian") }) { Text("Русский") }; TextButton(onClick = { model.preferences("language" to "english") }) { Text("English") } } }
        item { TextButton(onClick = { image(0) }) { Text(t("Фон приложения", "App background")) } }
        if (snapshot.backgrounds.any { it.getLong("scope") == 0L }) {
            item { Text("${t("Размытие", "Blur")}: ${globalBlur.toInt()}"); Slider(globalBlur, { globalBlur = it }, valueRange = 0f..40f, onValueChangeFinished = { model.command("background_blur", "id" to 0, "blur" to globalBlur.toInt()) }) }
            item { TextButton(onClick = { model.command("background_remove", "id" to 0) }) { Text(t("Убрать фон", "Remove background")) } }
        }
        item { HorizontalDivider(); Text(t("Синхронизация с ПК / Android", "Sync with PC / Android"), style = MaterialTheme.typography.titleLarge) }
        item { Text(t("Оба устройства должны быть в одной локальной сети. Откройте синхронизацию на ПК и вставьте его код ниже.", "Both devices must be on the same LAN. Open sync on your PC and paste its code below."), style = MaterialTheme.typography.bodyMedium) }
        item { ToggleRow(t("Включить синхронизацию", "Enable LAN sync"), sync.optBoolean("enabled")) { if (it) { notify(); SyncService.start(context) } else SyncService.stop(context) } }
        item { Text(t("В фоне синхронизация выключается после окончания передачи. Для нового подключения включите её снова.", "In the background, sync closes after the transfer finishes. Enable it again for a new connection."), style = MaterialTheme.typography.bodySmall) }
        item { OutlinedTextField(deviceName, { deviceName = it }, label = { Text(t("Имя устройства", "Device name")) }, singleLine = true) }
        item { TextButton(onClick = { model.preferences("sync_device_name" to deviceName) }) { Text(t("Сохранить имя", "Save device name")) } }
        if (sync.optBoolean("enabled")) {
            item { TextButton(onClick = {
                (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Shard pairing", sync.optString("code")))
            }) { Text(t("Скопировать код этого устройства", "Copy this device's code")) } }
            item { OutlinedTextField(code, { code = it }, label = { Text(t("Код другого устройства", "Other device's code")) }, maxLines = 4) }
            item { Button(onClick = { model.command("sync_pair", "code" to code) }, enabled = code.isNotBlank()) { Text(t("Подключиться", "Connect")) } }
            item { Text(sync.optString("peer").takeUnless { it == "null" }.orEmpty()) }
            item { Button(onClick = { notify(); SyncService.start(context, "library") }, enabled = !sync.optBoolean("transferring")) { Text(t("Отправить всю библиотеку", "Send entire library")) } }
            sync.optJSONObject("progress")?.let { progress -> item {
                Text(progress.optString("title")); LinearProgressIndicator(progress = { (progress.optLong("bytes").toFloat() / progress.optLong("total").coerceAtLeast(1)).coerceIn(0f, 1f) }, Modifier.fillMaxWidth())
            } }
            sync.optString("error").takeUnless { it == "null" || it.isEmpty() }?.let { error -> item { Text(error, color = MaterialTheme.colorScheme.error) } }
        }
        item { ToggleRow(t("Предупреждать о дубликатах", "Ask about duplicates"), snapshot.preferences.optBoolean("sync_warn_duplicates")) { model.preferences("sync_warn_duplicates" to it) } }
        item { ToggleRow(t("Игнорировать дубликаты", "Reuse duplicates"), snapshot.preferences.optBoolean("sync_ignore_duplicates")) { model.preferences("sync_ignore_duplicates" to it) } }
        item { HorizontalDivider(); Text(t("Загрузки", "Downloads"), style = MaterialTheme.typography.titleLarge) }
        item { Text(model.downloadMessage.ifEmpty { t("Нет активных загрузок", "No active downloads") }) }
        item { TextButton(onClick = { WorkManager.getInstance(context).cancelUniqueWork(DownloadWorker.CHAIN) }) { Text(t("Остановить загрузки", "Cancel downloads")) } }
        item { Text("Shard Android 0.1.0 · Rust + Android Media3", style = MaterialTheme.typography.bodySmall) }
    }
}
