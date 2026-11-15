package music.shard.mobile

import android.app.Notification
import android.app.NotificationManager
import android.content.ComponentName
import android.media.session.MediaController as SystemMediaController
import android.media.session.MediaSession as SystemMediaSession
import android.os.SystemClock
import android.view.KeyEvent
import android.view.View
import android.widget.FrameLayout
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class PlaybackControlsTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun notificationAndMiniPlayerShareFavoritesAndPlaybackModes() = runBlocking {
        val original = Core.snapshot().preferences
        Core.call("preferences", "patch" to JSONObject().put("language", "russian").put("shuffle", false).put("repeat", "off").put("volume", 0.23))
        val wave = File(Core.root, "staging/controls-${UUID.randomUUID()}.wav")
        val pcm = ByteArray(16000 * 120)
        wave.writeBytes(ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
            .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
            .put("data".toByteArray()).putInt(pcm.size).array() + pcm)
        val track = Track(Core.call("import", "path" to wave.absolutePath, "title" to "Playback controls test") as JSONObject)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        if (android.os.Build.VERSION.SDK_INT >= 33)
            instrumentation.uiAutomation.grantRuntimePermission(Core.context.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
        var controller: MediaController? = null
        val activity = ActivityScenario.launch(MainActivity::class.java)
        try {
            var future: com.google.common.util.concurrent.ListenableFuture<MediaController>? = null
            instrumentation.runOnMainSync {
                future = MediaController.Builder(Core.context, SessionToken(Core.context, ComponentName(Core.context, PlaybackService::class.java))).buildAsync()
            }
            controller = future!!.get(15, TimeUnit.SECONDS)
            instrumentation.runOnMainSync {
                assertEquals("Old app gain must not attenuate system volume", 1f, controller!!.volume, 0.001f)
                controller!!.shuffleModeEnabled = false
                controller!!.repeatMode = Player.REPEAT_MODE_OFF
                controller!!.setMediaItems(listOf(track.mediaItem(), track.mediaItem()))
                controller!!.prepare(); controller!!.play()
            }
            await { var playing = false; instrumentation.runOnMainSync { playing = controller!!.isPlaying }; playing }
            compose.waitUntil(15000) { compose.onAllNodesWithContentDescription("В избранное").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithContentDescription("В избранное").performClick()
            await { runBlocking { track.id in Core.snapshot().favorites } }
            compose.onNodeWithContentDescription("Убрать из избранного").assertIsOn()
            // Reproduce overlapping library refreshes and writes from app/service
            // clients. All mutations belong to the test track and are removed below.
            withTimeout(30_000) {
                coroutineScope {
                    repeat(4) {
                        launch(Dispatchers.IO) {
                            repeat(10) {
                                assertTrue(Core.snapshot().tracks.any { it.id == track.id })
                                Core.call("favorite", "id" to track.id, "enabled" to true)
                            }
                        }
                    }
                }
            }
            compose.onNodeWithContentDescription("Приостановить").performClick()
            await { var paused = false; instrumentation.runOnMainSync { paused = !controller!!.playWhenReady }; paused }
            compose.onNodeWithContentDescription("Продолжить").performClick()
            await { var playing = false; instrumentation.runOnMainSync { playing = controller!!.isPlaying }; playing }

            val notifications = Core.context.getSystemService(NotificationManager::class.java)
            var system: SystemMediaController? = null
            await {
                val token = notifications.activeNotifications.firstNotNullOfOrNull {
                    @Suppress("DEPRECATION")
                    it.notification.extras.getParcelable<SystemMediaSession.Token>(Notification.EXTRA_MEDIA_SESSION)
                }
                if (token != null) system = SystemMediaController(Core.context, token)
                system?.playbackState?.customActions?.size == 3
            }
            assertEquals(PlaybackActions.commands.map { it.customAction }, system!!.playbackState!!.customActions.map { it.action })
            fun notification() = notifications.activeNotifications.first { it.id == 1001 }.notification
            fun expanded(): View {
                var view: View? = null
                instrumentation.runOnMainSync {
                    view = notification().bigContentView.apply(Core.context, FrameLayout(Core.context))
                }
                return view!!
            }
            fun tap(viewId: Int) {
                val panel = expanded()
                instrumentation.runOnMainSync { assertTrue(panel.findViewById<View>(viewId).performClick()) }
            }
            val panel = expanded()
            listOf(R.id.player_shuffle, R.id.player_previous, R.id.player_play, R.id.player_next, R.id.player_repeat, R.id.player_favorite).forEach {
                assertEquals(View.VISIBLE, panel.findViewById<View>(it).visibility)
            }
            // Exercise the actual notification PendingIntents, including transport controls.
            tap(R.id.player_play)
            await { var paused = false; instrumentation.runOnMainSync { paused = !controller!!.playWhenReady }; paused }
            await { expanded().findViewById<View>(R.id.player_play).contentDescription == "Продолжить" }
            tap(R.id.player_play)
            await { var playing = false; instrumentation.runOnMainSync { playing = controller!!.isPlaying }; playing }
            tap(R.id.player_next)
            await { var index = 0; instrumentation.runOnMainSync { index = controller!!.currentMediaItemIndex }; index == 1 }
            tap(R.id.player_previous)
            await { var index = 1; instrumentation.runOnMainSync { index = controller!!.currentMediaItemIndex }; index == 0 }
            tap(R.id.player_favorite)
            await { runBlocking { track.id !in Core.snapshot().favorites } }
            compose.waitUntil(15000) { compose.onAllNodesWithContentDescription("В избранное").fetchSemanticsNodes().isNotEmpty() }
            tap(R.id.player_shuffle)
            await { runBlocking { Core.snapshot().preferences.optBoolean("shuffle") } }
            tap(R.id.player_repeat)
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "all" } }

            compose.onNodeWithText(track.title, substring = true).performClick()
            compose.onNodeWithText("Добавить в избранное").assertIsDisplayed()
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
            compose.waitUntil(15000) { compose.onAllNodesWithText("Добавить в избранное").fetchSemanticsNodes().isEmpty() }
            notification().contentIntent.send()
            compose.waitUntil(15000) { compose.onAllNodesWithText("Добавить в избранное").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Повтор: все · Вперемешку").assertIsDisplayed()
            compose.onNodeWithText("Добавить в избранное").assertIsDisplayed().performClick()
            await { runBlocking { track.id in Core.snapshot().favorites } }
            compose.onNodeWithText("В избранном").assertIsDisplayed()
            compose.onNodeWithContentDescription("Громкость").assertDoesNotExist()
            compose.onNodeWithContentDescription("Повтор: все").performClick()
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "one" } }
            compose.onNodeWithContentDescription("Повтор: один").performClick()
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "off" } }
            compose.onNodeWithText("Без повтора · Вперемешку").assertIsDisplayed()
            compose.onNodeWithContentDescription("Позиция воспроизведения")
                .performSemanticsAction(SemanticsActions.SetProgress) { assertTrue(it(30_000f)) }
            await { var position = 0L; instrumentation.runOnMainSync { position = controller!!.currentPosition }; position in 29_000..40_000 }
            compose.onNodeWithContentDescription("Настройки воспроизведения").performClick()
            compose.onNodeWithText("Перемешивание: вкл.").performClick()
            await { runBlocking { !Core.snapshot().preferences.optBoolean("shuffle") } }
            // Platform labels follow app changes too.
            await { system!!.playbackState?.customActions?.lastOrNull()?.name.toString() == "Повтор: выкл." }
            await { expanded().findViewById<View>(R.id.player_repeat).contentDescription == "Повтор: выкл." }
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
            compose.waitUntil(15000) { compose.onAllNodesWithContentDescription("Закрыть плеер").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithContentDescription("Закрыть плеер").performClick()
            await { var empty = false; instrumentation.runOnMainSync { empty = controller!!.mediaItemCount == 0 && !controller!!.isPlaying }; empty }
            compose.onNodeWithContentDescription("Закрыть плеер").assertDoesNotExist()
            assertTrue(Core.snapshot().tracks.any { it.id == track.id })
        } finally {
            instrumentation.runOnMainSync {
                controller?.pause(); controller?.clearMediaItems()
                controller?.shuffleModeEnabled = original.optBoolean("shuffle")
                controller?.repeatMode = when (original.optString("repeat")) { "all" -> Player.REPEAT_MODE_ALL; "one" -> Player.REPEAT_MODE_ONE; else -> Player.REPEAT_MODE_OFF }
                controller?.release()
            }
            try {
                activity.close()
            } finally {
                Core.call("track_delete", "id" to track.id)
                Core.call("preferences", "patch" to JSONObject().put("language", original.optString("language"))
                    .put("shuffle", original.optBoolean("shuffle")).put("repeat", original.optString("repeat")).put("volume", original.optDouble("volume", 0.7)))
                wave.delete()
            }
        }
    }

    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 15000
        while (!condition()) {
            check(SystemClock.elapsedRealtime() < deadline) { "Playback controls did not update" }
            Thread.sleep(50)
        }
    }
}
