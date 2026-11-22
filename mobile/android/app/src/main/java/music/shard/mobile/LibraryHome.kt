package music.shard.mobile

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import java.io.File

@Composable
internal fun LibraryTabs(selected: Int, t: (String, String) -> String, select: (Int) -> Unit) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf(t("Плейлисты", "Playlists"), t("Избранное", "Favorites"), t("Все треки", "All tracks")).forEachIndexed { i, title ->
            FilterChip(selected == i, { select(i) }, label = { Text(title, maxLines = 1, style = MaterialTheme.typography.labelLarge) },
                shape = RoundedCornerShape(24.dp), colors = FilterChipDefaults.filterChipColors(selectedContainerColor = MaterialTheme.colorScheme.primary, selectedLabelColor = MaterialTheme.colorScheme.onPrimary))
        }
    }
}

@Composable
internal fun LibraryHome(snapshot: Snapshot, t: (String, String) -> String, selected: Int, selectTab: (Int) -> Unit,
    open: (Long) -> Unit, allTracks: () -> Unit, create: () -> Unit, add: () -> Unit) {
    Column {
        LibraryTabs(selected, t, selectTab)
        LazyVerticalGrid(GridCells.Adaptive(148.dp), contentPadding = PaddingValues(16.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    CollectionShortcut(t("Избранное", "Liked music"), "${snapshot.favorites.size} ${t("треков", "tracks")}", Icons.Default.Favorite,
                        Modifier.weight(1f), Color(0xff7864cd), Color(0xffc2b6ec)) { open(-1) }
                    CollectionShortcut(t("Все треки", "All tracks"), "${snapshot.tracks.size} ${t("в библиотеке", "in your library")}", Icons.Default.LibraryMusic,
                        Modifier.weight(1f), Color(0xff285f50), Color(0xff76ab83), allTracks)
                }
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(t("Ваши плейлисты", "Made by you"), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    FilledTonalButton(onClick = create, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp)) {
                        Icon(Icons.Default.Add, null, Modifier.size(18.dp)); Spacer(Modifier.width(4.dp)); Text(t("Создать", "Create"))
                    }
                }
            }
            if (snapshot.playlists.isEmpty()) item(span = { GridItemSpan(maxLineSpan) }) {
                Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)) {
                    Column(Modifier.fillMaxWidth().padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Icon(Icons.Default.Headphones, null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
                        Text(t("Музыка под ваше настроение", "A place for every mood"), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                        Text(t("Соберите первый плейлист или добавьте музыку с телефона и по ссылке.", "Create your first playlist, import music from your phone or paste a link."), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Button(onClick = add) { Icon(Icons.Default.Add, null); Spacer(Modifier.width(8.dp)); Text(t("Добавить музыку", "Add music")) }
                    }
                }
            }
            items(snapshot.playlists, key = { it.id }) { playlist ->
                Card(onClick = { open(playlist.id) }, colors = CardDefaults.cardColors(containerColor = Color.Transparent), shape = RoundedCornerShape(16.dp)) {
                    Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(16.dp))
                        .background(playlist.color?.let { Color(it or 0xff000000) } ?: MaterialTheme.colorScheme.secondaryContainer), contentAlignment = Alignment.Center) {
                        if (playlist.cover != null) AsyncImage(File(Core.root, playlist.cover), null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                        else { Icon(Icons.Default.Album, null, Modifier.size(64.dp), tint = MaterialTheme.colorScheme.primary.copy(alpha = 0.75f)) }
                    }
                    Text(playlist.name, Modifier.padding(top = 10.dp, start = 2.dp), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text("${playlist.count} ${t("треков", "tracks")}", Modifier.padding(top = 2.dp, start = 2.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable private fun CollectionShortcut(title: String, subtitle: String, icon: ImageVector, modifier: Modifier, start: Color, end: Color, click: () -> Unit) {
    Card(onClick = click, modifier = modifier, shape = RoundedCornerShape(20.dp)) {
        Column(Modifier.fillMaxWidth().background(Brush.linearGradient(listOf(start, end.copy(alpha = 0.5f)))).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(icon, null, Modifier.size(28.dp), tint = Color.White)
            Text(title, color = Color.White, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.titleMedium)
            Text(subtitle, color = Color.White.copy(alpha = 0.8f), style = MaterialTheme.typography.bodySmall, maxLines = 1)
        }
    }
}
