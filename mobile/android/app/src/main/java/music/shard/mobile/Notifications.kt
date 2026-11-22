package music.shard.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent

object Notifications {
    fun createChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel("transfers", "Downloads and LAN transfers", NotificationManager.IMPORTANCE_LOW))
    }
    fun transfer(context: Context, text: String, cancel: PendingIntent? = null): Notification =
        Notification.Builder(context, "transfers").setSmallIcon(R.drawable.ic_notification).setContentTitle("Shard")
            .setContentText(text).setOnlyAlertOnce(true).setOngoing(true)
            .setContentIntent(PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
            .apply { if (cancel != null) addAction(Notification.Action.Builder(null, "Cancel / Отмена", cancel).build()) }
            .build()
}
