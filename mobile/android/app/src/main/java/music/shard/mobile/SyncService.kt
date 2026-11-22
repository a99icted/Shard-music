package music.shard.mobile

import android.app.Service
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import android.os.PowerManager
import androidx.core.content.ContextCompat
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONObject

/** LAN listener exists only during an explicit session. No boot receiver or periodic discovery. */
class SyncService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var started = false
    @Volatile private var sending = false
    @Volatile private var shuttingDown = false
    private var lock: PowerManager.WakeLock? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val cancel = PendingIntent.getService(this, 0, Intent(this, SyncService::class.java).setAction("stop"), PendingIntent.FLAG_IMMUTABLE)
        androidx.core.app.ServiceCompat.startForeground(this, 202, Notifications.transfer(this, "LAN sync / Синхронизация", cancel),
            if (android.os.Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0)
        if (intent?.action == "stop") { shutdown(); return START_NOT_STICKY }
        if (!started) {
            started = true
            scope.launch {
                try {
                    Core.call("sync_start")
                    val idleStarted = android.os.SystemClock.elapsedRealtime()
                    var lastRevision = 0L
                    while (isActive && !shuttingDown) {
                        val status = Core.call("sync_status") as JSONObject
                        state.value = status
                        val revision = status.optLong("revision")
                        if (revision > lastRevision) {
                            lastRevision = revision
                            Core.notifyLibraryChanged()
                        }
                        val transferring = status.optBoolean("transferring") || sending
                        if (transferring) acquireTransferLock() else releaseTransferLock()
                        if (!visible && !transferring) { shutdown(); break }
                        if (!transferring && android.os.SystemClock.elapsedRealtime() - idleStarted > 10 * 60 * 1000L) { shutdown(); break }
                        delay(1000) // UI/progress only while a user-requested session is alive.
                    }
                } catch (e: Exception) {
                    if (e !is CancellationException) state.value = JSONObject().put("error", e.message)
                    shutdown()
                }
            }
        }
        intent?.getStringExtra("selection")?.let { selection ->
            if (!sending) {
                sending = true
                scope.launch {
                    try {
                        // Start command may still be opening the listener.
                        Core.call("sync_start")
                        Core.call("sync_send", "selection" to selection, "id" to intent.getLongExtra("id", 0))
                    } catch (e: Exception) {
                        if (e !is CancellationException) error.value = e.message
                    } finally { sending = false }
                }
            }
        }
        return START_NOT_STICKY
    }

    private fun acquireTransferLock() {
        val wake = lock ?: (getSystemService(POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Shard:transfer").also { it.setReferenceCounted(false); lock = it }
        // A bounded lock tolerates service/process failures. Refresh only while bytes may flow.
        if (!wake.isHeld) wake.acquire(10 * 60 * 1000L)
    }
    private fun releaseTransferLock() { lock?.let { if (it.isHeld) it.release() } }
    private fun shutdown() {
        if (shuttingDown) return
        shuttingDown = true
        scope.launch {
            runCatching { Core.call("sync_stop") }
            state.value = JSONObject().put("enabled", false)
            releaseTransferLock()
            stopSelf()
        }
    }
    override fun onTimeout(startId: Int, fgsType: Int) { shutdown() }
    override fun onDestroy() {
        releaseTransferLock()
        // Also close Rust sockets if Android destroys the service unexpectedly.
        CoroutineScope(Dispatchers.IO).launch { runCatching { Core.call("sync_stop") } }
        scope.cancel()
        super.onDestroy()
    }
    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        @Volatile var visible = false
        val state = MutableStateFlow(JSONObject().put("enabled", false))
        val error = MutableStateFlow<String?>(null)
        fun start(context: Context, selection: String? = null, id: Long = 0) {
            ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java)
                .putExtra("selection", selection).putExtra("id", id))
        }
        fun stop(context: Context) { context.startService(Intent(context, SyncService::class.java).setAction("stop")) }
    }
}
