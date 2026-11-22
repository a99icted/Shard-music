package music.shard.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.journeyapps.barcodescanner.CameraPreview
import com.journeyapps.barcodescanner.DecoratedBarcodeView
import com.journeyapps.barcodescanner.DefaultDecoderFactory

/** Local, QR-only, single-shot decoding. The camera lives only while this dialog is resumed. */
@Composable internal fun QrScannerDialog(t: (String, String) -> String, onDismiss: () -> Unit, onResult: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val result by rememberUpdatedState(onResult)
    var error by remember { mutableStateOf(false) }
    val camera = remember {
        DecoratedBarcodeView(context).apply {
            barcodeView.decoderFactory = DefaultDecoderFactory(listOf(BarcodeFormat.QR_CODE))
            setStatusText("")
            decodeSingle { scan -> result(scan.text) }
            barcodeView.addStateListener(object : CameraPreview.StateListener {
                override fun previewSized() = Unit
                override fun previewStarted() = Unit
                override fun previewStopped() = Unit
                override fun cameraClosed() = Unit
                override fun cameraError(exception: Exception) { error = true }
            })
        }
    }
    DisposableEffect(lifecycle, camera) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_RESUME -> camera.resume()
                Lifecycle.Event.ON_PAUSE, Lifecycle.Event.ON_STOP -> camera.pause()
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) camera.resume()
        onDispose { lifecycle.removeObserver(observer); camera.pause() }
    }
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(Modifier.safeDrawingPadding()) {
                Row(Modifier.fillMaxWidth().padding(16.dp)) {
                    Text(t("Сканирование QR", "Scan QR"), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    IconButton(onClick = onDismiss) { Icon(Icons.Default.Close, t("Закрыть камеру", "Close camera")) }
                }
                Text(t("Откройте QR в синхронизации Shard на компьютере и наведите камеру.", "Open Shard sync on your computer and point the camera at its QR code."), Modifier.padding(horizontal = 20.dp, vertical = 12.dp))
                if (error) Text(t("Камера недоступна. Закройте сканер и вставьте код вручную.", "Camera unavailable. Close the scanner and paste the code manually."), Modifier.padding(20.dp), color = MaterialTheme.colorScheme.error)
                else AndroidView(factory = { camera }, modifier = Modifier.fillMaxWidth().weight(1f))
            }
        }
    }
}
