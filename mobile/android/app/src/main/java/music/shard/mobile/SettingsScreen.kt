@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package music.shard.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.work.WorkManager
import org.json.JSONObject

@Composable internal fun SettingsScreen(model: LibraryModel, snapshot: Snapshot, sync: JSONObject, t: (String, String) -> String, notify: () -> Unit, selected: Int, select: (Int) -> Unit, image: (Long) -> Unit) {
    val titles = listOf(t("Оформление", "Appearance"), t("Загрузки", "Downloads"), t("Синхронизация", "Sync"), t("Статистика", "Statistics"), t("О приложении", "About"))
    val active = selected.coerceIn(titles.indices)
    Column {
        ScrollableTabRow(selectedTabIndex = active, edgePadding = 16.dp, containerColor = Color.Transparent, divider = {}) {
            titles.forEachIndexed { index, title -> Tab(active == index, { select(index); if (index == 3) model.refresh() }, text = { Text(title, fontWeight = if (active == index) FontWeight.Bold else FontWeight.Normal) }) }
        }
        when (active) {
            0 -> AppearanceSettings(snapshot, model, t, image)
            1 -> DownloadSettings(model, t, notify)
            2 -> SyncSettings(snapshot, model, sync, t, notify)
            3 -> StatisticsScreen(snapshot, t)
            else -> LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                item { SettingsCard("Shard", t("Ваша музыка. На вашем устройстве.", "Your music. On your device."), Icons.Default.Headphones) {
                    Text("${t("Версия", "Version")} ${BuildConfig.VERSION_NAME}", style = MaterialTheme.typography.titleMedium)
                    Text(t("Локальная библиотека и синхронизация с компьютером. Музыка доступна без интернета после загрузки.", "A local music library with desktop sync. Downloaded music plays offline."), color = MaterialTheme.colorScheme.onSurfaceVariant)
                } }
            }
        }
    }
}

@Composable internal fun SettingsCard(title: String, subtitle: String? = null, icon: ImageVector, content: @Composable ColumnScope.() -> Unit) {
    Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Icon(icon, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(24.dp))
                Column {
                    Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    if (subtitle != null) Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            content()
        }
    }
}

@Composable internal fun SettingsToggle(title: String, description: String? = null, checked: Boolean, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            if (description != null) Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Switch(checked, change)
    }
}

@Composable private fun AppearanceSettings(snapshot: Snapshot, model: LibraryModel, t: (String, String) -> String, image: (Long) -> Unit) {
    val background = snapshot.backgrounds.find { it.getLong("scope") == 0L }
    var blur by remember(background?.optInt("blur")) { mutableFloatStateOf(background?.optInt("blur", 12)?.toFloat() ?: 12f) }
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { SettingsCard(t("Цветовая тема", "Color theme"), t("Выберите настроение приложения", "Make the library feel like you"), Icons.Default.Palette) {
            val themes = listOf(Triple("forest", t("Лес", "Forest"), Color(0xffb5ed80)), Triple("onyx", t("Оникс", "Onyx"), Color(0xffced0e0)),
                Triple("pick_me", t("Пикми", "Pick me"), Color(0xfff2a8ce)), Triple("cosmic", t("Космос", "Cosmic"), Color(0xffb3a2ff)))
            themes.chunked(2).forEach { row -> Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) { row.forEach { (id, name, color) ->
                OutlinedButton(onClick = { model.preferences("theme" to id) }, Modifier.weight(1f), colors = ButtonDefaults.outlinedButtonColors(
                    containerColor = if (snapshot.preferences.optString("theme") == id) color.copy(alpha = 0.16f) else Color.Transparent), contentPadding = PaddingValues(12.dp)) {
                    Icon(if (snapshot.preferences.optString("theme") == id) Icons.Default.CheckCircle else Icons.Default.Circle, null, Modifier.size(18.dp), tint = color)
                    Spacer(Modifier.width(8.dp)); Text(name, color = color)
                }
            } } }
        } }
        item { SettingsCard(t("Язык", "Language"), icon = Icons.Default.Language) {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                listOf("russian" to "Русский", "english" to "English").forEach { (id, name) ->
                    OutlinedButton(onClick = { model.preferences("language" to id) }, modifier = Modifier.weight(1f), colors = ButtonDefaults.outlinedButtonColors(containerColor = if (snapshot.preferences.optString("language") == id) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)) { Text(name) }
                }
            }
        } }
        item { SettingsCard(t("Фон библиотеки", "Library background"), t("Фотография или спокойный однотонный фон", "A photo or a calm solid background"), Icons.Default.Wallpaper) {
            FilledTonalButton(onClick = { image(0) }, Modifier.fillMaxWidth()) { Icon(Icons.Default.Image, null); Spacer(Modifier.width(8.dp)); Text(t("Выбрать изображение", "Choose image")) }
            if (background != null) {
                Text("${t("Размытие", "Blur")} · ${blur.toInt()}")
                Slider(blur, { blur = it }, valueRange = 0f..40f, onValueChangeFinished = { model.command("background_blur", "id" to 0, "blur" to blur.toInt()) })
                OutlinedButton(onClick = { model.command("background_remove", "id" to 0) }, Modifier.fillMaxWidth()) { Text(t("Убрать изображение", "Remove image")) }
            }
        } }
    }
}

@Composable private fun DownloadSettings(model: LibraryModel, t: (String, String) -> String, notify: () -> Unit) {
    val context = LocalContext.current
    val preferences = remember { context.getSharedPreferences("mobile", Context.MODE_PRIVATE) }
    var wifi by remember { mutableStateOf(preferences.getBoolean("wifi_only", true)) }
    var charging by remember { mutableStateOf(preferences.getBoolean("charging_only", false)) }
    var details by remember { mutableStateOf(false) }
    val version = model.downloaderVersion
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { SettingsCard(t("Сеть и батарея", "Network & battery"), t("Условия для новых загрузок", "Applied to new downloads"), Icons.Default.BatteryChargingFull) {
            SettingsToggle(t("Только Wi-Fi", "Unmetered network only"), t("Не расходовать мобильный трафик", "Save mobile data"), wifi) { wifi = it; preferences.edit().putBoolean("wifi_only", it).apply() }
            SettingsToggle(t("Только на зарядке", "Only while charging"), checked = charging) { charging = it; preferences.edit().putBoolean("charging_only", it).apply() }
            Text(t("При низком заряде загрузки ждут автоматически.", "Downloads automatically wait when the battery is low."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        } }
        item { SettingsCard(t("Загрузчик YouTube", "YouTube downloader"), icon = Icons.Default.SystemUpdateAlt) {
            Text(if (version.isBlank()) t("Версия проверяется при первой загрузке", "Version checked on the first download") else "yt-dlp $version")
            Text(t("Обновления проверяются перед скачиванием, не чаще раза в сутки. Это помогает при изменениях YouTube.", "Updates are checked before downloads, at most once a day, to keep up with YouTube changes."), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Button(onClick = { notify(); DownloadWorker.update(context) }, enabled = !model.downloadActive, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Default.Update, null); Spacer(Modifier.width(8.dp)); Text(t("Обновить загрузчик", "Update downloader"))
            }
        } }
        item { SettingsCard(t("Состояние загрузок", "Download status"), icon = Icons.Default.Download) {
            Text(model.downloadMessage.ifEmpty { t("Активных загрузок нет", "No active downloads") })
            if (model.downloadActive) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                OutlinedButton(onClick = { WorkManager.getInstance(context).cancelUniqueWork(DownloadWorker.CHAIN) }, Modifier.fillMaxWidth()) { Icon(Icons.Default.Close, null); Spacer(Modifier.width(8.dp)); Text(t("Остановить загрузки", "Cancel downloads")) }
            }
            if (model.downloadDetails.isNotBlank()) OutlinedButton(onClick = { details = true }, Modifier.fillMaxWidth()) { Text(t("Технические подробности", "Technical details")) }
        } }
    }
    if (details) AlertDialog(onDismissRequest = { details = false }, title = { Text(t("Отчёт загрузчика", "Downloader report")) }, text = {
        LazyColumn { item { Text(model.downloadDetails, style = MaterialTheme.typography.bodySmall) } }
    }, confirmButton = { Button(onClick = { details = false }) { Text(t("Закрыть", "Close")) } }, dismissButton = {
        OutlinedButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Shard", model.downloadDetails)) }) { Text(t("Скопировать", "Copy")) }
    })
}

@Composable private fun SyncSettings(snapshot: Snapshot, model: LibraryModel, sync: JSONObject, t: (String, String) -> String, notify: () -> Unit) {
    val context = LocalContext.current
    var code by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf(snapshot.preferences.optString("sync_device_name")) }
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { SettingsCard(t("Устройства рядом", "Nearby devices"), t("Телефон и компьютер в одной Wi-Fi сети", "Phone and computer on the same Wi-Fi"), Icons.Default.Devices) {
            SettingsToggle(t("Локальная синхронизация", "LAN sync"), checked = sync.optBoolean("enabled")) { if (it) { notify(); SyncService.start(context) } else SyncService.stop(context) }
            Text(t("В фоне соединение закроется после передачи для экономии батареи.", "In the background, the connection closes after the transfer to save battery."), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        } }
        if (sync.optBoolean("enabled")) {
            item { SettingsCard(t("Подключить устройство", "Connect a device"), icon = Icons.Default.Link) {
                OutlinedButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Shard", sync.optString("code"))) }, Modifier.fillMaxWidth()) { Icon(Icons.Default.ContentCopy, null); Spacer(Modifier.width(8.dp)); Text(t("Скопировать мой код", "Copy my code")) }
                OutlinedTextField(code, { code = it }, label = { Text(t("Код с другого устройства", "Code from the other device")) }, modifier = Modifier.fillMaxWidth(), maxLines = 4)
                Button(onClick = { model.command("sync_pair", "code" to code) }, enabled = code.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(t("Подключиться", "Connect")) }
                sync.optString("peer").takeUnless { it.isEmpty() || it == "null" }?.let { Text(it, color = MaterialTheme.colorScheme.primary) }
                FilledTonalButton(onClick = { notify(); SyncService.start(context, "library") }, enabled = !sync.optBoolean("transferring") && !sync.isNull("peer"), modifier = Modifier.fillMaxWidth()) { Text(t("Отправить всю библиотеку", "Send entire library")) }
                sync.optJSONObject("progress")?.let { progress ->
                    Text(progress.optString("title")); LinearProgressIndicator(progress = { (progress.optLong("bytes").toFloat() / progress.optLong("total").coerceAtLeast(1)).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                }
                sync.optString("error").takeUnless { it.isEmpty() || it == "null" }?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            } }
        }
        item { SettingsCard(t("Имя этого устройства", "This device's name"), icon = Icons.Default.Smartphone) {
            OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth(), singleLine = true)
            FilledTonalButton(onClick = { model.preferences("sync_device_name" to name) }, enabled = name.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(t("Сохранить имя", "Save name")) }
        } }
        item { SettingsCard(t("Повторяющиеся треки", "Duplicate tracks"), icon = Icons.Default.LibraryMusic) {
            SettingsToggle(t("Спрашивать о дубликатах", "Ask about duplicates"), checked = snapshot.preferences.optBoolean("sync_warn_duplicates")) { model.preferences("sync_warn_duplicates" to it) }
            SettingsToggle(t("Использовать существующие", "Reuse existing tracks"), t("Не создавать дополнительные копии", "Avoid extra copies"), snapshot.preferences.optBoolean("sync_ignore_duplicates")) { model.preferences("sync_ignore_duplicates" to it) }
        } }
    }
}
