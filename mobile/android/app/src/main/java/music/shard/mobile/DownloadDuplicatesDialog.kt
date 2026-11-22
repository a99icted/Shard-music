package music.shard.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import org.json.JSONObject

@Composable internal fun DownloadDuplicatesDialog(plan: JSONObject, t: (String, String) -> String,
    apply: (Set<Int>) -> Unit, cancel: () -> Unit) {
    val entries = plan.getJSONArray("entries").objects()
    val duplicates = plan.getJSONArray("duplicates").objects().filter { entries[it.getInt("index")].optString("state") != "done" }
    var copies by remember(plan.getString("id")) { mutableStateOf(emptySet<Int>()) }
    AlertDialog(onDismissRequest = cancel, title = { Text(t("Повторяющиеся треки", "Duplicate tracks")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(plan.optString("title"), style = MaterialTheme.typography.titleSmall)
                Text(t("Без отметки используем имеющийся трек и добавим его в плейлист. Отметьте те, которые нужно скачать отдельной копией.",
                    "Unchecked tracks reuse the existing audio and join the playlist. Check tracks to download a separate copy."))
                LazyColumn(Modifier.heightIn(max = 300.dp)) {
                    items(duplicates, key = { it.getInt("index") }) { duplicate ->
                        val index = duplicate.getInt("index")
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(index in copies, { copies = if (it) copies + index else copies - index })
                            Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
                                Text(entries[index].downloadLabel())
                                Text(if (duplicate.optString("kind") == "same_source") t("Та же запись", "Same recording") else t("Совпадают исполнитель, название и длительность", "Matching artist, title and duration"), style = MaterialTheme.typography.bodySmall)
                                Text(duplicate.optString("existing_title"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                }
                OutlinedButton(onClick = { copies = duplicates.map { it.getInt("index") }.toSet() }, Modifier.fillMaxWidth()) {
                    Text(t("Отметить все копии", "Select all copies"))
                }
            }
        }, confirmButton = { Button(onClick = { apply(copies) }) { Text(t("Продолжить", "Continue")) } },
        dismissButton = { TextButton(onClick = cancel) { Text(t("Отменить загрузку", "Cancel download")) } })
}
