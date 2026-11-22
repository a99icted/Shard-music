package music.shard.mobile

import android.Manifest
import android.content.ComponentName
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.SystemClock
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
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
import kotlin.math.sin

/** Capture the Android mixer, not just isPlaying/currentPosition (which can advance silently).
 * Shell capture permissions are test-only; the release app never records audio.
 */
@RunWith(AndroidJUnit4::class)
@SdkSuppress(minSdkVersion = 29)
class AudioOutputTest {
    @Test fun firstStartQueueReplacementAndTransitionsProduceAudioWithoutSeeking() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val original = Core.snapshot().preferences
        val directory = File(Core.root, "staging/audio-output-${UUID.randomUUID()}").apply { mkdirs() }
        val tracks = mutableListOf<Track>()
        var controller: MediaController? = null
        var capture: AudioRecord? = null
        val audio = Core.context.getSystemService(AudioManager::class.java)
        val oldVolume = audio.getStreamVolume(AudioManager.STREAM_MUSIC)
        val activity = ActivityScenario.launch(MainActivity::class.java)
        try {
            // Exercise the actual downloaded formats as well as uncompressed audio.
            val wave = File(directory, "tone.wav")
            writeTone(wave)
            val formats = listOf(
                Triple("mp3", "libmp3lame", "44100"),
                Triple("m4a", "aac", "48000"),
                Triple("opus", "libopus", "48000"),
                Triple("aiff", "pcm_s16be", "44100")
            )
            for ((extension, codec, rate) in formats) {
                val file = File(directory, "tone.$extension")
                val process = AudioCompatibility.ffmpegCommand("-nostdin", "-v", "error", "-y",
                    "-i", wave.absolutePath, "-c:a", codec, "-ar", rate, file.absolutePath)
                    .start()
                val error = process.inputStream.bufferedReader().use { it.readText() }
                assertEquals("Cannot create $extension fixture: $error", 0, process.waitFor())
            }
            for (extension in listOf("wav", "mp3", "m4a", "opus", "aiff")) {
                tracks += Track(Core.call("import", "path" to File(directory, "tone.$extension").absolutePath,
                    "title" to "Audio output test $extension") as JSONObject)
            }
            var future: com.google.common.util.concurrent.ListenableFuture<MediaController>? = null
            instrumentation.runOnMainSync {
                future = MediaController.Builder(Core.context,
                    SessionToken(Core.context, ComponentName(Core.context, PlaybackService::class.java))).buildAsync()
            }
            controller = future!!.get(15, TimeUnit.SECONDS)
            instrumentation.runOnMainSync { controller!!.pause(); controller!!.clearMediaItems() }
            instrumentation.uiAutomation.adoptShellPermissionIdentity(
                Manifest.permission.RECORD_AUDIO, "android.permission.CAPTURE_AUDIO_OUTPUT")
            audio.setStreamVolume(AudioManager.STREAM_MUSIC, audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC), 0)
            val recorder = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.REMOTE_SUBMIX)
                .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build())
                .setBufferSizeInBytes(48000 * 4).build()
            capture = recorder
            assertEquals(AudioRecord.STATE_INITIALIZED, recorder.state)
            recorder.startRecording()
            assertEquals(AudioRecord.RECORDSTATE_RECORDING, recorder.recordingState)
            // Adopted permissions replace the app's identity for later binder calls.
            // Restore it before ExoPlayer requests focus (Android 15 hardening).
            instrumentation.uiAutomation.dropShellPermissionIdentity()
            fun assertSound(label: String) {
                val deadline = SystemClock.elapsedRealtime() + 8000
                val samples = ShortArray(4800)
                // Discard one second to exclude data buffered before a queue replacement.
                repeat(20) { assertTrue(recorder.read(samples, 0, samples.size, AudioRecord.READ_BLOCKING) > 0) }
                var loudSamples = 0
                var peak = 0
                while (SystemClock.elapsedRealtime() < deadline) {
                    val count = recorder.read(samples, 0, samples.size, AudioRecord.READ_BLOCKING)
                    assertTrue("Audio capture failed: $count", count > 0)
                    // Ignore occasional system clicks: require sustained tone, not one sample.
                    peak = maxOf(peak, (0 until count).maxOf { kotlin.math.abs(samples[it].toInt()) })
                    // REMOTE_SUBMIX has its own volume curve; detect the generated tone
                    // even when Android applies that route's default attenuation.
                    loudSamples = if ((0 until count).count { kotlin.math.abs(samples[it].toInt()) > 64 } > count / 4)
                        loudSamples + count else 0
                    if (loudSamples >= 24000) return
                }
                fail("$label: Android mixer produced no sustained audio (peak=$peak)")
            }
            for (track in tracks) {
                // Exactly the app's setMediaItems -> prepare -> play sequence. No seek workaround.
                instrumentation.runOnMainSync {
                    controller!!.shuffleModeEnabled = false
                    controller!!.repeatMode = androidx.media3.common.Player.REPEAT_MODE_OFF
                    controller!!.setMediaItems(listOf(track.mediaItem()), 0, 0)
                    controller!!.prepare(); controller!!.play()
                }
                await {
                    var ready = false
                    instrumentation.runOnMainSync {
                        assertNull(controller!!.playerError)
                        ready = controller!!.isPlaying && controller!!.currentMediaItem?.mediaId == track.id.toString()
                    }
                    ready
                }
                assertSound("Start/replacement ${track.path.substringAfterLast('.')}")
            }
            instrumentation.runOnMainSync {
                controller!!.setMediaItems(listOf(tracks[1].mediaItem(), tracks[2].mediaItem()), 0, 0)
                controller!!.prepare(); controller!!.play()
            }
            assertSound("MP3 before natural transition")
            await {
                var next = false
                instrumentation.runOnMainSync { next = controller!!.currentMediaItemIndex == 1 && controller!!.isPlaying }
                next
            }
            assertSound("MP3 -> AAC natural transition")
            instrumentation.runOnMainSync { controller!!.pause(); controller!!.play() }
            assertSound("Resume")
        } finally {
            instrumentation.runOnMainSync {
                controller?.pause(); controller?.clearMediaItems()
                controller?.shuffleModeEnabled = original.optBoolean("shuffle")
                controller?.repeatMode = when (original.optString("repeat")) {
                    "all" -> androidx.media3.common.Player.REPEAT_MODE_ALL
                    "one" -> androidx.media3.common.Player.REPEAT_MODE_ONE
                    else -> androidx.media3.common.Player.REPEAT_MODE_OFF
                }
                controller?.release()
            }
            capture?.release()
            audio.setStreamVolume(AudioManager.STREAM_MUSIC, oldVolume, 0)
            instrumentation.uiAutomation.dropShellPermissionIdentity()
            activity.close()
            for (track in tracks) Core.call("track_delete", "id" to track.id)
            directory.deleteRecursively()
        }
    }

    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 20000
        while (!condition()) {
            check(SystemClock.elapsedRealtime() < deadline) { "Playback transition did not finish" }
            Thread.sleep(50)
        }
    }

    private fun writeTone(file: File) {
        val sampleRate = 48000
        val pcm = ByteBuffer.allocate(sampleRate * 5 * 2).order(ByteOrder.LITTLE_ENDIAN)
        repeat(sampleRate * 5) { pcm.putShort((sin(2 * Math.PI * 440 * it / sampleRate) * 12000).toInt().toShort()) }
        file.writeBytes(ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            .put("RIFF".toByteArray()).putInt(36 + pcm.capacity()).put("WAVEfmt ".toByteArray())
            .putInt(16).putShort(1).putShort(1).putInt(sampleRate).putInt(sampleRate * 2).putShort(2).putShort(16)
            .put("data".toByteArray()).putInt(pcm.capacity()).array() + pcm.array())
    }
}
