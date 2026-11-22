package music.shard.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import org.json.JSONObject

@Composable internal fun IgnoredSettings(snapshot: Snapshot, model: LibraryModel, t: (String, String) -> String, notify: () -> Unit) {
    val tracks = snapshot.tracks.filter { it.id in snapshot.ignored }
    var deleting by remember { mutableStateOf<Track?>(null) }
    var deleteAll by remember { mutableStateOf(false) }
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { SettingsCard(t("Игнорирование", "Ignoring"), icon = Icons.Default.Block) {
            Text(t("Эти треки остаются в библиотеке и плейлистах, но не попадают в очередь. Их можно запустить напрямую. При добавлении в игнор трек убирается из избранного.",
                "These tracks stay in your library and playlists, but are excluded from the queue. You can play them directly. Ignoring a track removes it from favorites."))
            OutlinedButton(onClick = { deleteAll = true }, enabled = tracks.isNotEmpty() && model.busy == 0) {
                Icon(Icons.Default.Delete, null); Spacer(Modifier.width(8.dp))
                Text(t("Удалить все игнорируемые треки", "Delete all ignored tracks"))
            }
            if (tracks.isEmpty()) Text(t("Игнорируемых треков нет", "No ignored tracks"))
        } }
        items(tracks, key = { it.id }) { track ->
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    snapshot.artwork(track.id)?.let { AsyncImage(it, null, Modifier.size(48.dp), contentScale = ContentScale.Crop) }
                    Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                        Text(track.title, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Text(track.artist, style = MaterialTheme.typography.bodySmall)
                        Text(t("Игнор", "Ignored"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                    }
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    IconButton(onClick = { notify(); model.play(listOf(track), track.id) }) { Icon(Icons.Default.PlayArrow, t("Послушать трек", "Play track")) }
                    IconButton(onClick = { model.toggleIgnored(track.id) }) { Icon(Icons.Default.Close, t("Убрать из игнора", "Stop ignoring")) }
                    IconButton(onClick = { deleting = track }) { Icon(Icons.Default.Delete, t("Удалить из хранилища", "Delete from storage")) }
                }
            }
        }
    }
    if (deleteAll || deleting != null) AlertDialog(onDismissRequest = { deleteAll = false; deleting = null },
        title = { Text(t("Удалить из хранилища?", "Delete from storage?")) },
        text = { Text(if (deleteAll) t("Будут удалены все игнорируемые треки, их аудиофайлы и записи в плейлистах.", "All ignored tracks, their audio files and playlist entries will be deleted.")
            else "${deleting?.title}\n" + t("Аудиофайл и записи в плейлистах будут удалены.", "The audio file and playlist entries will be deleted.")) },
        confirmButton = { Button(onClick = {
            if (deleteAll) model.deleteIgnored() else deleting?.let { model.deleteTrack(it.id) }
            deleteAll = false; deleting = null
        }) { Text(t("Удалить", "Delete")) } },
        dismissButton = { OutlinedButton(onClick = { deleteAll = false; deleting = null }) { Text(t("Отмена", "Cancel")) } })
}

@Composable internal fun ConnectedDevices(snapshot: Snapshot, sync: JSONObject, t: (String, String) -> String, notify: () -> Unit) {
    val context = LocalContext.current
    val peer = sync.optJSONObject("paired_device")
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { SettingsCard(t("Это устройство", "This device"), icon = Icons.Default.Smartphone) {
            Text(snapshot.preferences.optString("sync_device_name"))
            Text(snapshot.preferences.optString("sync_device_id"), style = MaterialTheme.typography.bodySmall)
        } }
        item { SettingsCard(t("Подключённые устройства", "Connected devices"), icon = Icons.Default.Devices) {
            if (peer == null) Text(t("Устройств пока нет. Подключите устройство кодом или QR во вкладке «Синхронизация».", "No devices yet. Pair with a code or QR in the Sync tab."))
            else {
                Text(peer.optString("name"), style = MaterialTheme.typography.titleMedium)
                Text(t("Сопряжено в текущей сессии", "Paired in this session"), color = MaterialTheme.colorScheme.primary)
                Text(peer.optString("address"))
                Text("ID: ${peer.optString("device_id")}", style = MaterialTheme.typography.bodySmall)
                Text("${t("Протокол", "Protocol")}: ${peer.optInt("protocol_version")}")
                Button(onClick = { notify(); SyncService.start(context, "library") }, enabled = !sync.optBoolean("transferring")) {
                    Text(t("Отправить всю библиотеку", "Send entire library"))
                }
                Text(t("Сопряжение не гарантирует, что устройство сейчас доступно. Live-управление пока не поддерживается.", "Pairing does not guarantee the device is currently reachable. Live playback control is not supported yet."), style = MaterialTheme.typography.bodySmall)
            }
            sync.optJSONObject("progress")?.let { progress ->
                Text(progress.optString("title"))
                LinearProgressIndicator(progress = { (progress.optLong("bytes").toFloat() / progress.optLong("total").coerceAtLeast(1)).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
            }
            sync.optString("error").takeUnless { it.isBlank() || it == "null" }?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        } }
    }
}
