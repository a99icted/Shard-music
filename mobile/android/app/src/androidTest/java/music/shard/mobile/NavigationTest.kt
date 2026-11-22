package music.shard.mobile

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class NavigationTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun albumHeaderScrollsAwayAndTitleReturnsOnReverseScroll(): Unit = runBlocking {
        val original = Core.snapshot().preferences.optString("language")
        Core.call("preferences", "patch" to JSONObject().put("language", "russian"))
        val name = "Scroll album ${UUID.randomUUID()}"
        val playlist = (Core.call("playlist_create", "name" to name) as Number).toLong()
        val directory = File(Core.root, "staging/scroll-${UUID.randomUUID()}").apply { mkdirs() }
        val tracks = mutableListOf<Long>()
        try {
            repeat(24) { index ->
                val pcm = ByteArray(16000).apply { this[0] = index.toByte(); this[1] = 1 }
                val wave = File(directory, "$index.wav")
                wave.writeBytes(ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
                    .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
                    .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
                    .put("data".toByteArray()).putInt(pcm.size).array() + pcm)
                val id = (Core.call("import", "path" to wave.absolutePath, "title" to "Scroll track $index") as JSONObject).getLong("id")
                tracks += id
                Core.call("playlist_add", "id" to playlist, "track" to id)
            }
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.waitUntil(15000) { compose.onAllNodesWithText(name).fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText(name).performScrollTo().performClick()
                compose.onNodeWithTag("playlist-header").assertIsDisplayed()
                repeat(3) { compose.onNodeWithTag("track-list").performTouchInput { swipeUp() } }
                compose.onNodeWithTag("playlist-header").assertIsNotDisplayed()
                compose.onNodeWithText("Название или исполнитель").assertIsNotDisplayed()
                compose.onNodeWithTag("collection-title").assertIsNotDisplayed()
                compose.onNodeWithTag("track-list").performTouchInput { swipeDown() }
                compose.onNodeWithTag("collection-title").assertIsDisplayed()
                compose.onNodeWithTag("track-list").performScrollToIndex(0)
                compose.onNodeWithTag("playlist-header").assertIsDisplayed()
                compose.onNodeWithText("Слушать").assertIsDisplayed()
            }
        } finally {
            Core.call("playlist_delete", "id" to playlist)
            for (id in tracks) Core.call("track_delete", "id" to id)
            Core.call("preferences", "patch" to JSONObject().put("language", original))
            directory.deleteRecursively()
        }
    }

    @Test fun libraryFirstAndSettingsInSeparateTabs() {
        val original = runBlocking { Core.snapshot().preferences.optString("language") }
        runBlocking { Core.call("preferences", "patch" to JSONObject().put("language", "russian")) }
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.waitUntil(15000) { compose.onAllNodesWithText("Ваши плейлисты").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Плейлисты").assertIsDisplayed()
                compose.onNodeWithText("Статистика").assertDoesNotExist()
                compose.onAllNodesWithText("Все треки").onFirst().performClick()
                compose.onNodeWithText("Название или исполнитель").assertIsDisplayed()
                compose.onNodeWithText("Избранное").performClick()
                compose.onNodeWithText("Слушать").assertIsDisplayed()
                compose.onNodeWithText("Настройки").performClick()
                compose.onNodeWithText("Цветовая тема").assertIsDisplayed()
                compose.onNodeWithText("Плеер").assertDoesNotExist()
                compose.onNodeWithText("Загрузки").performScrollTo().performClick()
                compose.onNodeWithText("Сеть и батарея").assertIsDisplayed()
                compose.onNodeWithText("Цветовая тема").assertDoesNotExist()
                compose.onNodeWithText("Статистика").performScrollTo().performClick()
                compose.onNodeWithText("Время прослушивания").assertIsDisplayed()
                compose.onNodeWithText("Сеть и батарея").assertDoesNotExist()
                compose.onNodeWithText("Игнорирование").performScrollTo().performClick()
                compose.onNodeWithText("Удалить все игнорируемые треки").assertIsDisplayed()
                screenshot("ignored-settings.png")
                compose.onNodeWithText("Подключённые устройства").assertDoesNotExist()
                SyncService.start(Core.context)
                compose.waitUntil(15000) { compose.onAllNodesWithText("Подключённые устройства").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Подключённые устройства").performScrollTo().performClick()
                compose.onNodeWithText("Это устройство").assertIsDisplayed()
                screenshot("connected-devices.png")
                SyncService.stop(Core.context)
                compose.waitUntil(15000) { compose.onAllNodesWithText("Подключённые устройства").fetchSemanticsNodes().isEmpty() }
                compose.onNodeWithText("Библиотека").performClick()
                compose.onNodeWithText("Плейлисты").performClick()
                compose.onNodeWithText("Ваши плейлисты").assertIsDisplayed()
            }
        } finally {
            SyncService.stop(Core.context)
            runBlocking { Core.call("preferences", "patch" to JSONObject().put("language", original)) }
        }
    }

    private fun screenshot(name: String) {
        val bitmap = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        try { File(Core.context.cacheDir, name).outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) } }
        finally { bitmap.recycle() }
    }
}
