package com.lumen.core.config.crypto

import org.json.JSONObject
import java.security.MessageDigest
import java.util.Base64
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

open class IncyDecryptError(message: String, cause: Throwable? = null) : Exception(message, cause)

/** Native, dependency-free (JCA) decoder for `incy://crypt1/` subscriptions. */
object IncyCrypt {
    const val INCY_SCHEME = "incy://"
    const val INCY_CRYPT1_PREFIX = "incy://crypt1/"

    private const val KEYMAT_A_SLICE_B64 = "7odqBjr3BNe0CfGRDZcxzBQZCB7AiZOgEnBVaHPh7y0="
    private const val KEYMAT_B_SLICE_B64 = "z1EN9DsquX6phHju5fGxz4DjpIT8k2kxbM2H5Ut5l7c="
    private const val KEY_FINGERPRINT = "b6bf708471cc90043232967660aade86a50b4e57929db2e53c5fa34db624c08c"
    private const val NONCE_SIZE = 12
    private const val TAG_SIZE = 16
    private const val MAX_WIRE_SIZE = 1024 * 1024

    private val key: ByteArray by lazy { deriveKey() }

    fun isIncyLink(text: String?): Boolean =
        text?.trim()?.startsWith(INCY_SCHEME, ignoreCase = true) == true

    fun isIncyCryptLink(text: String?): Boolean =
        text?.trim()?.startsWith(INCY_CRYPT1_PREFIX, ignoreCase = true) == true

    fun decryptIncyLink(link: String): String {
        val body = link.trim()
        if (!isIncyCryptLink(body)) {
            throw IncyDecryptError("Unsupported link (expected incy://crypt1/)")
        }
        val payload = body.substring(INCY_CRYPT1_PREFIX.length)
            .filterNot(Char::isWhitespace)
            .trimEnd('/')
        if (payload.isEmpty()) throw IncyDecryptError("Empty incy://crypt1 payload")
        if (payload.length > MAX_WIRE_SIZE * 4 / 3 + 8) {
            throw IncyDecryptError("incy://crypt1 payload is too large")
        }
        val wire = try {
            val padded = payload + "=".repeat((4 - payload.length % 4) % 4)
            Base64.getUrlDecoder().decode(padded)
        } catch (e: IllegalArgumentException) {
            throw IncyDecryptError("Invalid incy://crypt1 base64 payload", e)
        }
        if (wire.size < NONCE_SIZE + TAG_SIZE + 1) {
            throw IncyDecryptError("incy://crypt1 payload is too short")
        }
        val plaintext = try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(
                Cipher.DECRYPT_MODE,
                SecretKeySpec(key, "AES"),
                GCMParameterSpec(TAG_SIZE * 8, wire.copyOfRange(0, NONCE_SIZE))
            )
            cipher.doFinal(wire, NONCE_SIZE, wire.size - NONCE_SIZE)
        } catch (e: AEADBadTagException) {
            throw IncyDecryptError("incy://crypt1 authentication failed", e)
        } catch (e: Exception) {
            throw IncyDecryptError("Failed to decrypt incy://crypt1 payload", e)
        }
        val json = try {
            JSONObject(String(plaintext, Charsets.UTF_8))
        } catch (e: Exception) {
            throw IncyDecryptError("incy://crypt1 plaintext is not valid JSON", e)
        }
        val url = json.optString("url", "").trim()
        if (url.isEmpty()) {
            throw IncyDecryptError("incy://crypt1 JSON has no non-empty 'url' field")
        }
        return url
    }

    private fun deriveKey(): ByteArray {
        val decoder = Base64.getDecoder()
        val seed = "incydeepcrypt1v2026.06".toByteArray(Charsets.US_ASCII) +
            decoder.decode(KEYMAT_A_SLICE_B64) + decoder.decode(KEYMAT_B_SLICE_B64)
        val digest = MessageDigest.getInstance("SHA-256")
        val derived = digest.digest(seed)
        val fingerprint = digest.digest(derived).joinToString("") { "%02x".format(it.toInt() and 0xff) }
        if (fingerprint != KEY_FINGERPRINT) throw IncyDecryptError("Incy key fingerprint mismatch")
        return derived
    }
}
