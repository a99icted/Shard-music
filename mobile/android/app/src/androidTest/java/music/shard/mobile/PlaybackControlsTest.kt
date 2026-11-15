package music.shard.mobile

import android.app.Notification
import android.app.NotificationManager
import android.content.ComponentName
import android.media.session.MediaController as SystemMediaController
import android.media.session.MediaSession as SystemMediaSession
import android.os.Bundle
import android.os.SystemClock
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
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
            // Check the actual platform session used by the notification, not an app-only shortcut.
            system!!.transportControls.sendCustomAction(PlaybackActions.FAVORITE, Bundle.EMPTY)
            await { runBlocking { track.id !in Core.snapshot().favorites } }
            compose.waitUntil(15000) { compose.onAllNodesWithContentDescription("В избранное").fetchSemanticsNodes().isNotEmpty() }
            system!!.transportControls.sendCustomAction(PlaybackActions.SHUFFLE, Bundle.EMPTY)
            await { runBlocking { Core.snapshot().preferences.optBoolean("shuffle") } }
            system!!.transportControls.sendCustomAction(PlaybackActions.REPEAT, Bundle.EMPTY)
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "all" } }

            compose.onNodeWithText(track.title).performClick()
            compose.onNodeWithText("Вперемешку").assertIsSelected()
            compose.onNodeWithText("Повтор: все").assertIsSelected()
            compose.onNodeWithContentDescription("Громкость").assertDoesNotExist()
            compose.onNodeWithText("Повтор: все").performClick()
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "one" } }
            compose.onNodeWithText("Повтор: один").performClick()
            await { runBlocking { Core.snapshot().preferences.optString("repeat") == "off" } }
            compose.onNodeWithText("Без повтора").assertIsNotSelected()
            // Platform labels follow app changes too.
            await { system!!.playbackState?.customActions?.lastOrNull()?.name.toString() == "Повтор: выкл." }
        } finally {
            instrumentation.runOnMainSync {
                controller?.pause(); controller?.clearMediaItems()
                controller?.shuffleModeEnabled = original.optBoolean("shuffle")
                controller?.repeatMode = when (original.optString("repeat")) { "all" -> Player.REPEAT_MODE_ALL; "one" -> Player.REPEAT_MODE_ONE; else -> Player.REPEAT_MODE_OFF }
                controller?.release()
            }
            activity.close()
            Core.call("track_delete", "id" to track.id)
            Core.call("preferences", "patch" to JSONObject().put("language", original.optString("language"))
                .put("shuffle", original.optBoolean("shuffle")).put("repeat", original.optString("repeat")).put("volume", original.optDouble("volume", 0.7)))
            wave.delete()
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
