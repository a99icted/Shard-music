package music.shard.mobile

import android.app.PendingIntent
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import java.io.File
import java.util.UUID

fun Track.mediaItem(): MediaItem = MediaItem.Builder().setMediaId(id.toString())
    .setUri(android.net.Uri.fromFile(File(Core.root, path)))
    .setMediaMetadata(MediaMetadata.Builder().setTitle(title).setArtist(artist).build()).build()

/** Android owns decoding, focus, noisy-route handling, notification and the audio wake lock. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
class PlaybackService : MediaSessionService() {
    private lateinit var player: ExoPlayer
    private var session: MediaSession? = null
    private val handler = Handler(Looper.getMainLooper())
    private val writer = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var trackId: Long? = null
    private var sessionId = UUID.randomUUID().toString()
    private var startedAt: Long? = null
    private var pendingMs = 0L
    private val checkpoint = object : Runnable {
        override fun run() {
            measure()
            flush()
            if (player.isPlaying) handler.postDelayed(this, 60_000)
        }
    }

    override fun onCreate() {
        super.onCreate()
        player = ExoPlayer.Builder(this).setAudioAttributes(
            AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true
        ).setHandleAudioBecomingNoisy(true).setWakeMode(C.WAKE_MODE_LOCAL).build()
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
            .setAudioOffloadPreferences(androidx.media3.common.TrackSelectionParameters.AudioOffloadPreferences.Builder()
                .setAudioOffloadMode(androidx.media3.common.TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_ENABLED)
                .build()).build()
        player.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                measure()
                startedAt = if (isPlaying) SystemClock.elapsedRealtime() else null
                handler.removeCallbacks(checkpoint)
                if (isPlaying) handler.postDelayed(checkpoint, 60_000) else flush()
            }
            override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
                measure(); flush()
                trackId = mediaItem?.mediaId?.toLongOrNull()
                sessionId = UUID.randomUUID().toString()
                startedAt = if (player.isPlaying) SystemClock.elapsedRealtime() else null
            }
        })
        val activity = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        session = MediaSession.Builder(this, player).setSessionActivity(activity)
            .setCallback(object : MediaSession.Callback {
                override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
                    return if (controller.packageName == packageName || controller.isTrusted)
                        super.onConnect(session, controller) else MediaSession.ConnectionResult.reject()
                }
                override fun onAddMediaItems(mediaSession: MediaSession, controller: MediaSession.ControllerInfo, mediaItems: MutableList<MediaItem>): ListenableFuture<MutableList<MediaItem>> {
                    val result = SettableFuture.create<MutableList<MediaItem>>()
                    writer.launch {
                        runCatching {
                            val byId = Core.snapshot().tracks.associateBy { it.id.toString() }
                            mediaItems.mapNotNull { byId[it.mediaId] }.map { AudioCompatibility.item(it) }.toMutableList()
                        }.onSuccess { result.set(it) }.onFailure { result.setException(it) }
                    }
                    return result
                }
            }).build()
        writer.launch {
            runCatching { Core.snapshot().preferences }.onSuccess { p ->
                handler.post {
                    player.volume = p.optDouble("volume", 0.7).toFloat()
                    player.shuffleModeEnabled = p.optBoolean("shuffle")
                    player.repeatMode = when (p.optString("repeat")) { "one" -> Player.REPEAT_MODE_ONE; "all" -> Player.REPEAT_MODE_ALL; else -> Player.REPEAT_MODE_OFF }
                }
            }
        }
    }

    private fun measure() {
        val now = SystemClock.elapsedRealtime()
        startedAt?.let { pendingMs += (now - it).coerceAtLeast(0) }
        startedAt = if (startedAt != null) now else null
    }
    private fun flush() {
        val id = trackId ?: return
        val ms = pendingMs
        val token = sessionId
        pendingMs = 0
        if (ms > 0) writer.launch {
            runCatching { Core.call("record", "id" to id, "ms" to ms, "at" to System.currentTimeMillis() / 1000, "session" to token) }
                .onFailure { android.util.Log.e("Shard", "Cannot save listening statistics", it) }
        }
    }
    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session
    override fun onTaskRemoved(rootIntent: Intent?) { if (!player.playWhenReady || player.mediaItemCount == 0) stopSelf() }
    override fun onDestroy() {
        measure(); flush()
        handler.removeCallbacksAndMessages(null)
        session?.release(); player.release()
        super.onDestroy()
    }
}
