package music.shard.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.core.app.ActivityScenario
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import android.content.ComponentName
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.TimeUnit
import java.util.concurrent.CountDownLatch
import androidx.media3.common.Player

@RunWith(AndroidJUnit4::class)
class NativeBridgeTest {
    @Test fun mediaSessionDecodesPrivateRustImportedAudio() = runBlocking {
        Core.snapshot()
        val wave = File(Core.root, "staging/${UUID.randomUUID()}.wav")
        val pcm = ByteArray(16000 * 5)
        val header = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
            .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
            .put("data".toByteArray()).putInt(pcm.size).array()
        wave.writeBytes(header + pcm)
        val json = Core.call("import", "path" to wave.absolutePath) as JSONObject
        val track = Track(json)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        var future: com.google.common.util.concurrent.ListenableFuture<MediaController>? = null
        var controller: MediaController? = null
        val playing = CountDownLatch(1)
        // API 35 requires a foreground app to acquire initial audio focus, as with a user tap.
        val activity = ActivityScenario.launch(MainActivity::class.java)
        try {
            instrumentation.runOnMainSync {
                future = MediaController.Builder(Core.context, SessionToken(Core.context, ComponentName(Core.context, PlaybackService::class.java))).buildAsync()
            }
            controller = future!!.get(15, TimeUnit.SECONDS)
            instrumentation.runOnMainSync {
                controller!!.addListener(object : Player.Listener {
                    override fun onIsPlayingChanged(isPlaying: Boolean) { if (isPlaying) playing.countDown() }
                    override fun onPlayerError(error: androidx.media3.common.PlaybackException) { playing.countDown() }
                })
                controller!!.setMediaItem(track.mediaItem())
                controller!!.prepare()
                controller!!.play()
            }
            assertTrue("Playback did not start", playing.await(15, TimeUnit.SECONDS))
            instrumentation.runOnMainSync {
                assertNull(controller!!.playerError)
                assertTrue(controller!!.isPlaying)
                assertEquals(track.id.toString(), controller!!.currentMediaItem?.mediaId)
                controller!!.seekTo(1000)
                controller!!.pause()
                controller!!.clearMediaItems()
            }
        } finally {
            instrumentation.runOnMainSync { controller?.release() }
            activity.close()
            Core.call("track_delete", "id" to track.id)
            wave.delete()
        }
    }

    /** Exercises real JNI, bundled SQLite and the same importer as desktop on Android. */
    @Test fun importPlaylistFavoriteAndDeleteThroughJni() = runBlocking {
        Core.snapshot() // initializes private directories and the actual Android database
        val wave = File(Core.root, "staging/${UUID.randomUUID()}.wav")
        val pcm = ByteArray(16000)
        val header = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            .put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray())
            .putInt(16).putShort(1).putShort(1).putInt(8000).putInt(16000).putShort(2).putShort(16)
            .put("data".toByteArray()).putInt(pcm.size).array()
        wave.writeBytes(header + pcm)
        val track = Core.call("import", "path" to wave.absolutePath) as JSONObject
        val id = track.getLong("id")
        val playlist = (Core.call("playlist_create", "name" to "JNI test") as Number).toLong()
        try {
            Core.call("favorite", "id" to id, "enabled" to true)
            Core.call("playlist_add", "id" to playlist, "track" to id)
            Core.call("track_edit", "id" to id, "title" to "Android WAV", "artist" to "JNI")
            val state = Core.snapshot()
            assertTrue(id in state.favorites)
            assertEquals("Android WAV", state.tracksIn(playlist).single().title)
            assertTrue(File(Core.root, track.getString("file_path")).isFile)
            Core.call("playlist_delete", "id" to playlist)
            assertTrue(Core.snapshot().tracks.any { it.id == id })
        } finally {
            runCatching { Core.call("playlist_delete", "id" to playlist) }
            Core.call("track_delete", "id" to id)
            wave.delete()
        }
        assertFalse(File(Core.root, track.getString("file_path")).exists())
        assertFalse(id in Core.snapshot().favorites)
    }
}
