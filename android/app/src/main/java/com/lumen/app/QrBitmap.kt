package com.lumen.app

import android.graphics.Bitmap
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter

internal fun encodeQrBitmap(text: String, edge: Int = 720): Bitmap {
    val bits = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, edge, edge)
    val pixels = IntArray(edge * edge) { i -> if (bits[i % edge, i / edge]) 0xff000000.toInt() else 0xffffffff.toInt() }
    return Bitmap.createBitmap(pixels, edge, edge, Bitmap.Config.ARGB_8888)
}
