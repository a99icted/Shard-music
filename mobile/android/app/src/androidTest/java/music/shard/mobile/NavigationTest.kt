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

@RunWith(AndroidJUnit4::class)
class NavigationTest {
    @get:Rule val compose = createEmptyComposeRule()

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
                compose.onNodeWithText("Загрузки").performScrollTo().performClick()
                compose.onNodeWithText("Сеть и батарея").assertIsDisplayed()
                compose.onNodeWithText("Цветовая тема").assertDoesNotExist()
                compose.onNodeWithText("Статистика").performScrollTo().performClick()
                compose.onNodeWithText("Время прослушивания").assertIsDisplayed()
                compose.onNodeWithText("Сеть и батарея").assertDoesNotExist()
                compose.onNodeWithText("Библиотека").performClick()
                compose.onNodeWithText("Плейлисты").performClick()
                compose.onNodeWithText("Ваши плейлисты").assertIsDisplayed()
            }
        } finally {
            runBlocking { Core.call("preferences", "patch" to JSONObject().put("language", original)) }
        }
    }
}
