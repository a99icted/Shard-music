package music.shard.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.view.View
import android.widget.RemoteViews
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.IconCompat
import androidx.media3.common.Player
import androidx.media3.session.CommandButton
import androidx.media3.session.MediaNotification
import androidx.media3.session.MediaSession
import com.google.common.collect.ImmutableList

/** Six visible controls need a custom notification: Android's MediaStyle allows five.
 * Media3 still owns the foreground service, audio session and action authentication.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal class PlaybackNotification(
    private val context: Context,
    private val isFavorite: (Long) -> Boolean,
    private val isEnglish: () -> Boolean
) : MediaNotification.Provider {
    private val handler = Handler(Looper.getMainLooper())
    private val power = context.getSystemService(PowerManager::class.java)
    private var session: MediaSession? = null
    private var actions: MediaNotification.ActionFactory? = null
    private var callback: MediaNotification.Provider.Callback? = null
    private var closed = false
    private val tick = object : Runnable {
        override fun run() {
            if (closed || !power.isInteractive) return
            val current = session ?: return
            if (!current.player.isPlaying) return
            callback?.onNotificationChanged(render(current, actions ?: return))
            schedule()
        }
    }
    private val screen = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            handler.removeCallbacks(tick)
            // No screen-off polling or wake lock. Refresh immediately when screen wakes.
            if (intent.action == Intent.ACTION_SCREEN_ON && session?.player?.isPlaying == true) handler.post(tick)
        }
    }

    init {
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel("playback", "Music / Музыка", NotificationManager.IMPORTANCE_LOW))
        ContextCompat.registerReceiver(context, screen,
            IntentFilter().apply { addAction(Intent.ACTION_SCREEN_ON); addAction(Intent.ACTION_SCREEN_OFF) },
            ContextCompat.RECEIVER_NOT_EXPORTED)
    }

    override fun createNotification(mediaSession: MediaSession, mediaButtonPreferences: ImmutableList<CommandButton>,
        actionFactory: MediaNotification.ActionFactory, onNotificationChangedCallback: MediaNotification.Provider.Callback): MediaNotification {
        session = mediaSession
        actions = actionFactory
        callback = onNotificationChangedCallback
        handler.removeCallbacks(tick)
        schedule()
        return render(mediaSession, actionFactory)
    }

    override fun handleCustomCommand(session: MediaSession, action: String, extras: Bundle) = false

    private fun schedule() {
        if (!closed && power.isInteractive && session?.player?.isPlaying == true) handler.postDelayed(tick, 5_000)
    }

    private fun render(session: MediaSession, factory: MediaNotification.ActionFactory): MediaNotification {
        val player = session.player
        fun t(ru: String, en: String) = if (isEnglish()) en else ru
        val id = player.currentMediaItem?.mediaId?.toLongOrNull()
        val liked = id?.let(isFavorite) == true
        val title = player.mediaMetadata.title ?: "Shard"
        val artist = player.mediaMetadata.artist ?: ""
        val position = player.currentPosition.coerceAtLeast(0)
        val duration = player.duration.coerceAtLeast(0)
        val playing = player.isPlaying
        val favoriteLabel = if (liked) t("Убрать из избранного", "Unlike track") else t("В избранное", "Like track")
        val shuffleLabel = if (player.shuffleModeEnabled) t("Перемешивание: вкл.", "Shuffle: on") else t("Перемешивание: выкл.", "Shuffle: off")
        val repeatLabel = when (player.repeatMode) {
            Player.REPEAT_MODE_ONE -> t("Повтор: один трек", "Repeat: one")
            Player.REPEAT_MODE_ALL -> t("Повтор: все треки", "Repeat: all")
            else -> t("Повтор: выкл.", "Repeat: off")
        }
        val open = session.sessionActivity
        fun views(layout: Int) = RemoteViews(context.packageName, layout).apply {
            setTextViewText(R.id.player_title, title)
            setTextViewText(R.id.player_artist, artist)
            setImageViewResource(R.id.player_play, if (playing) R.drawable.ic_player_pause else R.drawable.ic_player_play)
            setContentDescription(R.id.player_play, if (playing) t("Приостановить", "Pause") else t("Продолжить", "Resume"))
            setOnClickPendingIntent(R.id.player_play, factory.createMediaActionPendingIntent(session, Player.COMMAND_PLAY_PAUSE.toLong()))
            setImageViewResource(R.id.player_favorite, if (liked) R.drawable.ic_player_heart_filled else R.drawable.ic_player_heart)
            setContentDescription(R.id.player_favorite, favoriteLabel)
            setOnClickPendingIntent(R.id.player_favorite, factory.createCustomAction(session,
                IconCompat.createWithResource(context, R.drawable.ic_player_heart), favoriteLabel, PlaybackActions.FAVORITE, Bundle.EMPTY).actionIntent)
            setInt(R.id.player_favorite, "setColorFilter", if (liked) ACCENT else WHITE)
            if (open != null) {
                setOnClickPendingIntent(R.id.player_title, open)
                setOnClickPendingIntent(R.id.player_artist, open)
            }
        }
        val compact = views(R.layout.notification_player_compact)
        val expanded = views(R.layout.notification_player).apply {
            fun media(view: Int, command: Int, label: String, enabled: Boolean) {
                setContentDescription(view, label)
                setBoolean(view, "setEnabled", enabled)
                setFloat(view, "setAlpha", if (enabled) 1f else 0.35f)
                setOnClickPendingIntent(view, factory.createMediaActionPendingIntent(session, command.toLong()))
            }
            media(R.id.player_previous, Player.COMMAND_SEEK_TO_PREVIOUS, t("Предыдущий", "Previous"), player.isCommandAvailable(Player.COMMAND_SEEK_TO_PREVIOUS))
            media(R.id.player_next, Player.COMMAND_SEEK_TO_NEXT, t("Следующий", "Next"), player.isCommandAvailable(Player.COMMAND_SEEK_TO_NEXT))
            fun custom(view: Int, icon: Int, action: String, label: String, selected: Boolean) {
                setImageViewResource(view, icon)
                setContentDescription(view, label)
                setInt(view, "setColorFilter", if (selected) ACCENT else MUTED)
                setOnClickPendingIntent(view, factory.createCustomAction(session,
                    IconCompat.createWithResource(context, icon), label, action, Bundle.EMPTY).actionIntent)
            }
            custom(R.id.player_shuffle, R.drawable.ic_player_shuffle, PlaybackActions.SHUFFLE, shuffleLabel, player.shuffleModeEnabled)
            custom(R.id.player_repeat, if (player.repeatMode == Player.REPEAT_MODE_ONE) R.drawable.ic_player_repeat_one else R.drawable.ic_player_repeat,
                PlaybackActions.REPEAT, repeatLabel, player.repeatMode != Player.REPEAT_MODE_OFF)
            setProgressBar(R.id.player_progress, 1000, if (duration > 0) (position.toDouble() / duration * 1000).toInt().coerceIn(0, 1000) else 0, false)
            setContentDescription(R.id.player_progress, t("Открыть перемотку в плеере", "Open player to seek"))
            if (open != null) setOnClickPendingIntent(R.id.player_progress, open)
            setTextViewText(R.id.player_elapsed, time(position))
            setTextViewText(R.id.player_duration, time(duration))
            setViewVisibility(R.id.player_elapsed, if (playing) View.GONE else View.VISIBLE)
            setViewVisibility(R.id.player_clock, if (playing) View.VISIBLE else View.GONE)
            setChronometer(R.id.player_clock, SystemClock.elapsedRealtime() - position, null, playing)
        }
        val notification = NotificationCompat.Builder(context, "playback")
            .setSmallIcon(R.drawable.ic_notification).setContentTitle(title).setContentText(artist)
            .setContentIntent(open).setOnlyAlertOnce(true).setSilent(true).setShowWhen(false)
            .setColor(0xff25272d.toInt()).setColorized(true)
            .setCategory(NotificationCompat.CATEGORY_TRANSPORT).setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setOngoing(playing).setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .setStyle(NotificationCompat.DecoratedCustomViewStyle())
            .setCustomContentView(compact).setCustomBigContentView(expanded)
            .addExtras(Bundle().apply { putParcelable(Notification.EXTRA_MEDIA_SESSION, session.platformToken) })
            .setDeleteIntent(factory.createMediaActionPendingIntent(session, Player.COMMAND_STOP.toLong()))
            .build()
        return MediaNotification(1001, notification)
    }

    fun close() {
        closed = true
        handler.removeCallbacksAndMessages(null)
        context.unregisterReceiver(screen)
        session = null; actions = null; callback = null
    }

    private fun time(ms: Long) = "%d:%02d".format(ms / 60000, ms / 1000 % 60)
    private companion object {
        const val ACCENT = 0xffb5ed80.toInt()
        const val WHITE = 0xfff1f2f4.toInt()
        const val MUTED = 0xff989ba5.toInt()
    }
}
