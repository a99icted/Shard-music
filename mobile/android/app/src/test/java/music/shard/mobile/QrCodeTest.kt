package music.shard.mobile

import com.google.zxing.*
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.assertEquals
import org.junit.Test

class QrCodeTest {
    @Test fun printedDesktopPairingCodeDecodesWithoutNetwork() {
        val code = "shard-sync://192.168.1.4:43567/8e65a36a-696f-4d70-8b63-329292cc9965"
        val image = QRCodeWriter().encode(code, BarcodeFormat.QR_CODE, 400, 400)
        val pixels = IntArray(400 * 400) { if (image[it % 400, it / 400]) 0xff000000.toInt() else 0xffffffff.toInt() }
        val result = MultiFormatReader().decode(BinaryBitmap(HybridBinarizer(RGBLuminanceSource(400, 400, pixels))),
            mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
        assertEquals(code, result.text)
    }
}
