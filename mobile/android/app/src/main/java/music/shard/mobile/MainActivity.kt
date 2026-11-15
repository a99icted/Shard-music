package music.shard.mobile

import android.Manifest
import android.content.ComponentName
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.content.ContextCompat
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture

class MainActivity : ComponentActivity() {
    private val model: LibraryModel by viewModels()
    private var controllerFuture: ListenableFuture<MediaController>? = null
    private val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) { }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        controllerFuture = MediaController.Builder(this, SessionToken(this, ComponentName(this, PlaybackService::class.java))).buildAsync().also { future ->
            future.addListener({ runCatching { model.bind(future.get()) }.onFailure { model.message = it.message } }, ContextCompat.getMainExecutor(this))
        }
        setContent { ShardApp(model, onNotify = { if (Build.VERSION.SDK_INT >= 33) notifications.launch(Manifest.permission.POST_NOTIFICATIONS) }) }
        if (savedInstanceState == null) acceptShare(intent)
    }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); setIntent(intent); acceptShare(intent) }
    @Suppress("DEPRECATION")
    private fun acceptShare(intent: Intent) {
        if (intent.action == Intent.ACTION_SEND && intent.type == "text/plain") {
            intent.getStringExtra(Intent.EXTRA_TEXT)?.trim()?.let { SharedLink.url.value = it }
        } else if (intent.action in listOf(Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE) && intent.type?.startsWith("audio/") == true) {
            val uris = if (intent.action == Intent.ACTION_SEND_MULTIPLE) intent.getParcelableArrayListExtra<android.net.Uri>(Intent.EXTRA_STREAM).orEmpty()
                else listOfNotNull(intent.getParcelableExtra<android.net.Uri>(Intent.EXTRA_STREAM))
            model.importFiles(uris)
        }
    }
    override fun onStart() { super.onStart(); SyncService.visible = true; model.refresh() }
    override fun onStop() { SyncService.visible = false; super.onStop() }
    override fun onDestroy() { controllerFuture?.let(MediaController::releaseFuture); super.onDestroy() }
}
object SharedLink { val url = kotlinx.coroutines.flow.MutableStateFlow<String?>(null) }
