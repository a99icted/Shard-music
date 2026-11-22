package music.shard.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.URI
import java.util.UUID

/** Real JNI sockets: a QR fallback and the phone's actual connection interface. */
@RunWith(AndroidJUnit4::class)
class HotspotSyncTest {
    @Test fun qrFallbackAdvertisesTheConnectedInterface(): Unit = runBlocking {
        Core.call("sync_start")
        val token = UUID.randomUUID().toString()
        val loopback = InetAddress.getByName("127.0.0.1")
        ServerSocket(0, 1, loopback).use { server ->
            server.soTimeout = 20000
            val code = "shard-sync://127.0.0.1:${server.localPort}/$token"
            val qr = "shard-sync://127.0.0.2:${server.localPort}/$token?alt=127.0.0.1"
            val serving = async(Dispatchers.IO) {
                server.accept().use { socket ->
                    socket.soTimeout = 15000
                    val input = DataInputStream(socket.getInputStream())
                    val size = input.readInt()
                    check(size in 1..8192)
                    val bytes = ByteArray(size)
                    input.readFully(bytes)
                    val request = JSONObject(String(bytes, Charsets.UTF_8))
                    assertEquals("pair", request.getString("type"))
                    assertEquals(token, request.getString("token"))
                    // Android's default route normally advertises 10.0.2.15 on
                    // the emulator. The paired endpoint must use this socket.
                    val peer = URI(request.getJSONObject("peer").getString("code"))
                    assertEquals(socket.inetAddress.hostAddress, peer.host)
                    val reply = JSONObject().put("ok", true).put("error", JSONObject.NULL)
                        .put("peer", JSONObject().put("code", code).put("device", UUID.randomUUID().toString()).put("name", "Hotspot test PC"))
                        .put("missing", JSONArray()).put("offsets", JSONArray()).put("duplicates", JSONArray())
                        .put("done", false).put("report", JSONObject().put("copied", 0).put("reused", 0).put("resumed_bytes", 0))
                        .toString().toByteArray(Charsets.UTF_8)
                    DataOutputStream(socket.getOutputStream()).apply { writeInt(reply.size); write(reply); flush() }
                }
            }
            try {
                assertEquals(qr, Core.call("sync_validate_code", "code" to qr))
                Core.call("sync_pair", "code" to qr)
                serving.await()
                assertEquals("Hotspot test PC", (Core.call("sync_status") as JSONObject).getString("peer"))
                val device = (Core.call("sync_status") as JSONObject).getJSONObject("paired_device")
                assertEquals("Hotspot test PC", device.getString("name"))
                assertEquals("127.0.0.1:${server.localPort}", device.getString("address"))
                assertEquals(2, device.getInt("protocol_version"))
                assertFalse(device.toString().contains(token))
                Core.call("sync_stop")
                Core.call("sync_start")
                assertTrue((Core.call("sync_status") as JSONObject).isNull("paired_device"))
            } finally {
                Core.call("sync_stop")
                server.close()
            }
        }
    }
}
