package com.lumen.core.vpn

import java.io.EOFException
import java.io.InputStream
import java.io.OutputStream
import java.math.BigInteger
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.Mac
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * Pure-Kotlin obfs2 / obfs3 pluggable transports.
 *
 * Lumen terminates the transport inside the app process, so no obfsproxy
 * binary has to be shipped: the APK does not grow and the upstream socket is
 * created by us, which keeps `VpnService.protect()` usable and avoids a
 * routing loop.
 */
internal class ObfsStream(
    val encrypt: (ByteArray, Int) -> ByteArray,
    val decrypt: (ByteArray, Int) -> ByteArray
)

private fun ctr(key: ByteArray, iv: ByteArray): Cipher =
    Cipher.getInstance("AES/CTR/NoPadding").apply {
        init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), IvParameterSpec(iv))
    }

/** Both transports derive a 16 byte AES key plus a 16 byte CTR IV per direction. */
private fun streamOf(sendSecret: ByteArray, recvSecret: ByteArray): ObfsStream {
    val send = ctr(sendSecret.copyOfRange(0, 16), sendSecret.copyOfRange(16, 32))
    val recv = ctr(recvSecret.copyOfRange(0, 16), recvSecret.copyOfRange(16, 32))
    return ObfsStream(
        encrypt = { data, len -> send.update(data, 0, len) },
        decrypt = { data, len -> recv.update(data, 0, len) }
    )
}

private fun readFully(input: InputStream, count: Int): ByteArray {
    val buffer = ByteArray(count)
    var read = 0
    while (read < count) {
        val n = input.read(buffer, read, count - read)
        if (n < 0) throw EOFException("obfs handshake: the bridge closed the connection")
        read += n
    }
    return buffer
}

/** obfs2 (and the pre-0.2 "legacy" MAC variant used by very old bridges). */
internal object Obfs2 {
    private const val MAGIC = 0x2BF5CA7E
    private const val MAX_PADDING = 8192
    private val random = SecureRandom()

    private fun mac(label: String, data: ByteArray, legacy: Boolean): ByteArray {
        val md = MessageDigest.getInstance("SHA-256")
        val salt = label.toByteArray(Charsets.US_ASCII)
        md.update(salt)
        md.update(data)
        if (!legacy) md.update(salt)
        return md.digest()
    }

    private fun int32(value: Int): ByteArray = byteArrayOf(
        (value ushr 24).toByte(), (value ushr 16).toByte(),
        (value ushr 8).toByte(), value.toByte()
    )

    private fun intAt(bytes: ByteArray, offset: Int): Int =
        ((bytes[offset].toInt() and 0xFF) shl 24) or
            ((bytes[offset + 1].toInt() and 0xFF) shl 16) or
            ((bytes[offset + 2].toInt() and 0xFF) shl 8) or
            (bytes[offset + 3].toInt() and 0xFF)

    fun handshake(input: InputStream, output: OutputStream, legacy: Boolean): ObfsStream {
        val initSeed = ByteArray(16).also { random.nextBytes(it) }
        val padKey = mac("Initiator obfuscation padding", initSeed, legacy)
        val padCipher = ctr(padKey.copyOfRange(0, 16), ByteArray(16))
        val padLen = random.nextInt(MAX_PADDING)
        val plain = int32(MAGIC) + int32(padLen) + ByteArray(padLen).also { random.nextBytes(it) }
        output.write(initSeed)
        output.write(padCipher.update(plain))
        output.flush()

        val respSeed = readFully(input, 16)
        val respKey = mac("Responder obfuscation padding", respSeed, legacy)
        val respCipher = ctr(respKey.copyOfRange(0, 16), ByteArray(16))
        val header = respCipher.update(readFully(input, 8))
        if (intAt(header, 0) != MAGIC) error("obfs2: the bridge sent an unexpected magic value")
        val respPadLen = intAt(header, 4)
        if (respPadLen < 0 || respPadLen > MAX_PADDING) error("obfs2: invalid padding length")
        if (respPadLen > 0) respCipher.update(readFully(input, respPadLen))

        val seeds = initSeed + respSeed
        return streamOf(
            mac("Initiator obfuscated data", seeds, legacy),
            mac("Responder obfuscated data", seeds, legacy)
        )
    }
}

/** obfs3: UniformDH key exchange over MODP group 5 plus AES-CTR. */
internal object Obfs3 {
    private val P = BigInteger(
        "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74" +
            "020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F1437" +
            "4FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED" +
            "EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF05" +
            "98DA48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB" +
            "9ED529077096966D670C354E4ABC9804F1746C08CA237327FFFFFFFFFFFFFFFF",
        16
    )
    private val G: BigInteger = BigInteger.valueOf(2)
    private const val KEY_BYTES = 192
    private const val MAX_PADDING = 8194
    private val random = SecureRandom()

    private fun hmac(key: ByteArray, text: String): ByteArray =
        Mac.getInstance("HmacSHA256")
            .apply { init(SecretKeySpec(key, "HmacSHA256")) }
            .doFinal(text.toByteArray(Charsets.US_ASCII))

    private fun fixed(value: BigInteger): ByteArray {
        val raw = value.toByteArray()
        val src = if (raw.size > KEY_BYTES) raw.copyOfRange(raw.size - KEY_BYTES, raw.size) else raw
        val out = ByteArray(KEY_BYTES)
        System.arraycopy(src, 0, out, KEY_BYTES - src.size, src.size)
        return out
    }

    fun handshake(input: InputStream, output: OutputStream): ObfsStream {
        // UniformDH: an even private exponent plus a randomly mirrored public
        // value make the handshake look like uniform random bytes.
        val priv = BigInteger(KEY_BYTES * 8, random).clearBit(0)
        var pub = G.modPow(priv, P)
        if (random.nextBoolean()) pub = P.subtract(pub)
        val padding = ByteArray(random.nextInt(MAX_PADDING / 2)).also { random.nextBytes(it) }
        output.write(fixed(pub))
        output.write(padding)
        output.flush()

        val serverPub = BigInteger(1, readFully(input, KEY_BYTES))
        val shared = fixed(serverPub.modPow(priv, P))
        val initSecret = hmac(shared, "Initiator obfuscated data")
        val respSecret = hmac(shared, "Responder obfuscated data")
        output.write(hmac(shared, "Initiator magic"))
        output.flush()
        awaitMagic(input, hmac(shared, "Responder magic"))
        return streamOf(initSecret, respSecret)
    }

    /** The bridge pads before its magic marker, so scan the stream for it. */
    private fun awaitMagic(input: InputStream, magic: ByteArray) {
        val window = ByteArray(magic.size)
        var filled = 0
        var scanned = 0
        while (true) {
            val next = input.read()
            if (next < 0) throw EOFException("obfs3: the bridge closed before the magic marker")
            if (filled < window.size) {
                window[filled++] = next.toByte()
            } else {
                System.arraycopy(window, 1, window, 0, window.size - 1)
                window[window.size - 1] = next.toByte()
            }
            if (filled == window.size && window.contentEquals(magic)) return
            if (++scanned > MAX_PADDING + magic.size) error("obfs3: magic marker not found")
        }
    }
}
