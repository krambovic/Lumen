package com.lumen.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class QrDecoderTest {
    @Test fun bundledDecoderReadsSubscriptionAndServerQrOffline() {
        val scanner = BarcodeScanning.getClient(BarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE).build())
        try {
            listOf("https://example.invalid/sub/test", "socks://user:secret@example.invalid:1080#Test",
                "wg:///+8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=@[2001:db8::1]:51820?publickey=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=&address=10.0.0.2/32",
                "[Interface]\nPrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\nEndpoint = [2001:db8::1]:51820\nAllowedIPs = 0.0.0.0/0").forEach { expected ->
                val bitmap = encodeQrBitmap(expected)
                try {
                    val decoded = Tasks.await(scanner.process(InputImage.fromBitmap(bitmap, 0)), 20, TimeUnit.SECONDS)
                    assertEquals(expected, decoded.single().rawValue)
                    if (expected.startsWith("wg:") || expected.startsWith("[Interface]")) {
                        val (nodes, errors) = com.lumen.core.config.parser.LinkParser.parseLinksText(decoded.single().rawValue!!)
                        assertEquals(errors.toString(), 1, nodes.size)
                        assertEquals("2001:db8::1", nodes.single().server)
                    }
                } finally { bitmap.recycle() }
            }
        } finally { scanner.close() }
    }
}
