package music.shard.mobile

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class QrScannerTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun openingAndClosingCameraKeepsLanSessionAlive(): Unit = runBlocking {
        val original = Core.snapshot().preferences.optString("language")
        Core.call("preferences", "patch" to JSONObject().put("language", "russian"))
        InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(Core.context.packageName, android.Manifest.permission.CAMERA)
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.waitUntil(15000) { compose.onAllNodesWithText("Настройки").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Настройки").performClick()
                compose.onNodeWithText("Синхронизация").performScrollTo().performClick()
                SyncService.start(Core.context)
                compose.waitUntil(15000) { compose.onAllNodesWithText("Сканировать QR с компьютера").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Сканировать QR с компьютера").performScrollTo().performClick()
                compose.onNodeWithText("Сканирование QR").assertIsDisplayed()
                delay(1600) // A separate Activity would stop the idle LAN service within one second.
                assertTrue((Core.call("sync_status") as JSONObject).getBoolean("enabled"))
                assertTrue(SyncService.visible)
                compose.onNodeWithContentDescription("Закрыть камеру").performClick()
                compose.onNodeWithText("Сканирование QR").assertDoesNotExist()
                assertTrue((Core.call("sync_status") as JSONObject).getBoolean("enabled"))
            }
        } finally {
            Core.call("sync_stop")
            Core.context.stopService(android.content.Intent(Core.context, SyncService::class.java))
            Core.call("preferences", "patch" to JSONObject().put("language", original))
        }
    }
}
