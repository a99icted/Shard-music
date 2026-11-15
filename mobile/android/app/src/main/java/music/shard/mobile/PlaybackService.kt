package music.shard.mobile

import android.app.PendingIntent
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.Bundle
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.CommandButton
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import androidx.media3.session.SessionError
import androidx.media3.session.MediaNotification
import androidx.core.app.NotificationCompat
import com.google.common.collect.ImmutableList
import androidx.media3.session.DefaultMediaNotificationProvider
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.channels.Channel
import org.json.JSONObject
import java.io.File
import java.util.UUID

internal fun nextRepeatMode(mode: Int): Int = when (mode) {
    Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL
    Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE
    else -> Player.REPEAT_MODE_OFF
}

internal object PlaybackActions {
    const val FAVORITE = "music.shard.favorite"
    const val SHUFFLE = "music.shard.shuffle"
    const val REPEAT = "music.shard.repeat"
    val commands = listOf(FAVORITE, SHUFFLE, REPEAT).map { SessionCommand(it, Bundle.EMPTY) }
}

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
    private val controls = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val preferenceWrites = Channel<Pair<String, Any>>(Channel.UNLIMITED)
    private var favorites = emptySet<Long>()
    private var english = false
    private var preferencesLoaded = false
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
        writer.launch {
            for ((key, value) in preferenceWrites) {
                runCatching { Core.call("preferences", "patch" to JSONObject().put(key, value)) }
                    .onFailure { android.util.Log.e("Shard", "Cannot save playback mode", it) }
            }
        }
        setMediaNotificationProvider(object : DefaultMediaNotificationProvider(this) {
            override fun addNotificationActions(session: MediaSession, buttons: ImmutableList<CommandButton>, builder: NotificationCompat.Builder, actionFactory: MediaNotification.ActionFactory): IntArray {
                super.addNotificationActions(session, buttons, builder, actionFactory)
                // Android 8-12 uses the notification's compact layout, independently of
                // Android 13+'s system media slots. Keep heart, play/pause and next visible.
                return listOf(
                    buttons.indexOfFirst { it.sessionCommand?.customAction == PlaybackActions.FAVORITE },
                    buttons.indexOfFirst { it.playerCommand == Player.COMMAND_PLAY_PAUSE },
                    buttons.indexOfFirst { it.playerCommand == Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM }
                ).filter { it >= 0 }.toIntArray()
            }
        })
        player = ExoPlayer.Builder(this).setAudioAttributes(
            AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true
        ).setHandleAudioBecomingNoisy(true).setWakeMode(C.WAKE_MODE_LOCAL).build()
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
            .setAudioOffloadPreferences(androidx.media3.common.TrackSelectionParameters.AudioOffloadPreferences.Builder()
                .setAudioOffloadMode(androidx.media3.common.TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_ENABLED)
                .build()).build()
        player.addListener(object : Player.Listener {
            override fun onShuffleModeEnabledChanged(shuffleModeEnabled: Boolean) {
                if (preferencesLoaded) savePreference("shuffle", shuffleModeEnabled)
                refreshMediaButtons()
            }
            override fun onRepeatModeChanged(repeatMode: Int) {
                if (preferencesLoaded) savePreference("repeat", when (repeatMode) {
                    Player.REPEAT_MODE_ALL -> "all"; Player.REPEAT_MODE_ONE -> "one"; else -> "off"
                })
                refreshMediaButtons()
            }
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
                refreshMediaButtons()
            }
        })
        val activity = PendingIntent.getActivity(this, 1001, Intent(this, MainActivity::class.java)
            .setAction(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(PlayerNavigation.OPEN_PLAYER, true), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        session = MediaSession.Builder(this, player).setSessionActivity(activity)
            .setCallback(object : MediaSession.Callback {
                override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
                    if (controller.packageName != packageName && !controller.isTrusted) return MediaSession.ConnectionResult.reject()
                    val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon().apply {
                        PlaybackActions.commands.forEach { add(it) }
                    }.build()
                    val result = MediaSession.ConnectionResult.AcceptedResultBuilder(session).setAvailableSessionCommands(commands)
                    // Android 13+ has five media slots. Free the previous-track slot so all
                    // three explicit controls fit alongside play/pause and next. App/headset
                    // controllers keep the full transport commands.
                    if (session.isMediaNotificationController(controller) || controller.packageName == "com.android.systemui") {
                        result.setAvailablePlayerCommands(MediaSession.ConnectionResult.DEFAULT_PLAYER_COMMANDS.buildUpon()
                            .remove(Player.COMMAND_SEEK_TO_PREVIOUS).remove(Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM).build())
                    }
                    return result.build()
                }
                override fun onCustomCommand(session: MediaSession, controller: MediaSession.ControllerInfo, command: SessionCommand, args: Bundle): ListenableFuture<SessionResult> {
                    when (command.customAction) {
                        PlaybackActions.SHUFFLE -> player.shuffleModeEnabled = !player.shuffleModeEnabled
                        PlaybackActions.REPEAT -> player.repeatMode = nextRepeatMode(player.repeatMode)
                        PlaybackActions.FAVORITE -> {
                            val id = player.currentMediaItem?.mediaId?.toLongOrNull()
                                ?: return Futures.immediateFuture(SessionResult(SessionError.ERROR_INVALID_STATE))
                            val result = SettableFuture.create<SessionResult>()
                            controls.launch {
                                runCatching { Core.toggleFavorite(id) }
                                    .onSuccess { result.set(SessionResult(SessionResult.RESULT_SUCCESS)) }
                                    .onFailure {
                                        android.util.Log.e("Shard", "Cannot change favorite", it)
                                        result.set(SessionResult(SessionError.ERROR_UNKNOWN))
                                    }
                            }
                            return result
                        }
                        else -> return Futures.immediateFuture(SessionResult(SessionError.ERROR_NOT_SUPPORTED))
                    }
                    return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
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
        refreshMediaButtons()
        controls.launch {
            Core.libraryChanges.collectLatest {
                runCatching { Core.snapshot() }.onSuccess { snapshot ->
                    favorites = snapshot.favorites
                    english = snapshot.preferences.optString("language") == "english"
                    if (!preferencesLoaded) {
                        player.shuffleModeEnabled = snapshot.preferences.optBoolean("shuffle")
                        player.repeatMode = when (snapshot.preferences.optString("repeat")) { "one" -> Player.REPEAT_MODE_ONE; "all" -> Player.REPEAT_MODE_ALL; else -> Player.REPEAT_MODE_OFF }
                        preferencesLoaded = true
                    }
                    refreshMediaButtons()
                }.onFailure { android.util.Log.e("Shard", "Cannot load playback controls", it) }
            }
        }
    }

    private fun savePreference(key: String, value: Any) {
        preferenceWrites.trySend(key to value)
    }

    private fun refreshMediaButtons() {
        val id = player.currentMediaItem?.mediaId?.toLongOrNull()
        val favorite = id in favorites
        fun t(ru: String, en: String) = if (english) en else ru
        fun button(icon: Int, action: String, label: String, compact: Int = -1) = CommandButton.Builder(icon)
            .setSessionCommand(SessionCommand(action, Bundle.EMPTY)).setDisplayName(label)
            .setEnabled(id != null)
            .setExtras(Bundle().apply { putInt(DefaultMediaNotificationProvider.COMMAND_KEY_COMPACT_VIEW_INDEX, compact) }).build()
        session?.setCustomLayout(listOf(
            button(if (favorite) CommandButton.ICON_HEART_FILLED else CommandButton.ICON_HEART_UNFILLED,
                PlaybackActions.FAVORITE, if (favorite) t("Убрать из избранного", "Unlike track") else t("В избранное", "Like track"), 0),
            button(if (player.shuffleModeEnabled) CommandButton.ICON_SHUFFLE_ON else CommandButton.ICON_SHUFFLE_OFF,
                PlaybackActions.SHUFFLE, if (player.shuffleModeEnabled) t("Перемешивание: вкл.", "Shuffle: on") else t("Перемешивание: выкл.", "Shuffle: off")),
            button(when (player.repeatMode) { Player.REPEAT_MODE_ALL -> CommandButton.ICON_REPEAT_ALL; Player.REPEAT_MODE_ONE -> CommandButton.ICON_REPEAT_ONE; else -> CommandButton.ICON_REPEAT_OFF },
                PlaybackActions.REPEAT, when (player.repeatMode) { Player.REPEAT_MODE_ALL -> t("Повтор: все треки", "Repeat: all"); Player.REPEAT_MODE_ONE -> t("Повтор: один трек", "Repeat: one"); else -> t("Повтор: выкл.", "Repeat: off") })
        ))
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
        controls.cancel()
        preferenceWrites.close()
        session?.release(); player.release()
        super.onDestroy()
    }
}
