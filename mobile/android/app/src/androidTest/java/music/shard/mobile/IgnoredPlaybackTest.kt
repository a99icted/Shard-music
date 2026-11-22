package music.shard.mobile

import android.content.ComponentName
import android.os.SystemClock
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class IgnoredPlaybackTest {
    @Test fun ignoreSkipsCurrentButDirectPreviewDoesNotRepeat() = runBlocking {
        val original = Core.snapshot().preferences
        val imported = mutableListOf<Track>()
        repeat(2) {
            val wave = File(Core.root, "staging/ignore-${UUID.randomUUID()}.wav")
            val pcm = ByteArray(16000 * 8)
            wave.writeBytes(ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
                .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
                .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
                .put("data".toByteArray()).putInt(pcm.size).array() + pcm)
            try { imported += Track(Core.call("import", "path" to wave.absolutePath) as JSONObject) }
            finally { wave.delete() }
        }
        val first = imported[0]; val next = imported[1]
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        var controller: MediaController? = null
        val activity = ActivityScenario.launch(MainActivity::class.java)
        fun onMain(block: () -> Unit) = instrumentation.runOnMainSync(block)
        fun await(check: () -> Boolean) {
            val deadline = SystemClock.elapsedRealtime() + 15000
            while (!check() && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(100)
            assertTrue("Playback condition timed out", check())
        }
        fun current(): String? { var id: String? = null; onMain { id = controller!!.currentMediaItem?.mediaId }; return id }
        try {
            var future: com.google.common.util.concurrent.ListenableFuture<MediaController>? = null
            onMain { future = MediaController.Builder(Core.context, SessionToken(Core.context, ComponentName(Core.context, PlaybackService::class.java))).buildAsync() }
            controller = future!!.get(15, TimeUnit.SECONDS)
            onMain {
                controller!!.shuffleModeEnabled = false
                controller!!.repeatMode = Player.REPEAT_MODE_OFF
                controller!!.setMediaItems(imported.map { it.mediaItem() })
                controller!!.prepare(); controller!!.play()
            }
            await { current() == first.id.toString() }
            Core.call("favorite", "id" to first.id, "enabled" to true)
            Core.call("ignored", "id" to first.id, "enabled" to true)
            await { current() == next.id.toString() }
            assertTrue(first.id in Core.snapshot().ignored)
            assertFalse(first.id in Core.snapshot().favorites)
            assertTrue(Core.snapshot().tracks.any { it.id == first.id })
            val addProcessed = java.util.concurrent.CountDownLatch(1)
            onMain { controller!!.addMediaItem(first.mediaItem()) }
            // Media3 1.5 masks addMediaItem locally even when the callback filters
            // the entire request. Read the authoritative service timeline through
            // a new controller instead of asserting that optimistic local mask.
            onMain {
                controller!!.sendCustomCommand(androidx.media3.session.SessionCommand(PlaybackActions.SHUFFLE, android.os.Bundle.EMPTY), android.os.Bundle.EMPTY)
                    .addListener({ addProcessed.countDown() }, java.util.concurrent.Executor { it.run() })
            }
            assertTrue(addProcessed.await(15, TimeUnit.SECONDS))
            onMain {
                controller!!.release()
                future = MediaController.Builder(Core.context, SessionToken(Core.context, ComponentName(Core.context, PlaybackService::class.java))).buildAsync()
            }
            controller = future!!.get(15, TimeUnit.SECONDS)
            onMain { controller!!.shuffleModeEnabled = false; assertEquals(1, controller!!.mediaItemCount) }
            // A stale UI context can still contain an ignored row before the selection.
            onMain {
                controller!!.setMediaItems(listOf(first.mediaItem(), next.mediaItem()), 1, 0)
                controller!!.prepare(); controller!!.play()
            }
            await {
                var count = 0
                onMain { count = controller!!.mediaItemCount }
                current() == next.id.toString() && count == 1
            }
            onMain {
                controller!!.repeatMode = Player.REPEAT_MODE_ONE
                controller!!.setMediaItems(listOf(first.mediaItem(direct = true), next.mediaItem()))
                controller!!.prepare(); controller!!.play()
            }
            await { current() == first.id.toString() }
            // Re-reading preferences/ignored IDs must not interrupt a direct preview.
            Core.call("preferences", "patch" to JSONObject().put("language", original.optString("language")))
            SystemClock.sleep(500)
            assertEquals(first.id.toString(), current())
            onMain { controller!!.seekTo(7800) }
            await { current() == next.id.toString() }
            onMain { assertEquals(1, controller!!.mediaItemCount) }
        } finally {
            onMain { controller?.stop(); controller?.clearMediaItems(); controller?.release() }
            activity.close()
            imported.forEach { Core.call("track_delete", "id" to it.id) }
            Core.call("preferences", "patch" to JSONObject().put("shuffle", original.optBoolean("shuffle")).put("repeat", original.optString("repeat", "off")))
        }
    }
}
