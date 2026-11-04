package com.lumen.core.config.crypto

import java.security.KeyFactory
import java.security.PrivateKey
import java.security.PublicKey
import java.security.SecureRandom
import java.security.interfaces.RSAPublicKey
import java.security.spec.MGF1ParameterSpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.OAEPParameterSpec
import javax.crypto.spec.PSource
import javax.crypto.spec.SecretKeySpec
import org.json.JSONObject

/** Versioned, authenticated envelope; no app-wide decryption secret is embedded in the APK. */
object ProtectedProfileCodec {
    const val PREFIX = "lumen-locked://"
    private const val MAX_BYTES = 8 * 1024 * 1024
    private val oaep = OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT)
    private val encoder = Base64.getUrlEncoder().withoutPadding()
    private val decoder = Base64.getUrlDecoder()
    private val aad = "Lumen protected profile v1".toByteArray()

    fun publicKey(text: String): PublicKey {
        require(text.length <= 2048) { "Recipient key is too large" }
        val key = KeyFactory.getInstance("RSA").generatePublic(X509EncodedKeySpec(
            decoder.decode(text.trim().removePrefix("lumen-key-v1:"))))
        require((key as RSAPublicKey).modulus.bitLength() in 2048..8192) { "Unsupported recipient key size" }
        return key
    }

    fun seal(name: String, config: String, recipient: PublicKey): String {
        require(recipient is RSAPublicKey && recipient.modulus.bitLength() in 2048..8192)
        require(config.toByteArray().size in 1..MAX_BYTES)
        val key = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val iv = ByteArray(12).also { SecureRandom().nextBytes(it) }
        try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
            cipher.updateAAD(aad)
            val payload = JSONObject().put("name", name.take(120)).put("config", config).toString().toByteArray()
            val wrapped = Cipher.getInstance("RSA/ECB/OAEPPadding").apply {
                init(Cipher.ENCRYPT_MODE, recipient, oaep)
            }.doFinal(key)
            return JSONObject().put("format", "lumen-locked").put("version", 1)
                .put("key", encoder.encodeToString(wrapped)).put("iv", encoder.encodeToString(iv))
                .put("payload", encoder.encodeToString(cipher.doFinal(payload))).toString()
        } finally { key.fill(0) }
    }

    fun open(envelope: String, recipient: PrivateKey): Pair<String, String> {
        require(envelope.length <= MAX_BYTES * 2) { "Protected file is too large" }
        try {
            val json = JSONObject(envelope)
            require(json.getString("format") == "lumen-locked" && json.getInt("version") == 1)
            val key = Cipher.getInstance("RSA/ECB/OAEPPadding").apply {
                init(Cipher.DECRYPT_MODE, recipient, oaep)
            }.doFinal(decoder.decode(json.getString("key")))
            try {
                val iv = decoder.decode(json.getString("iv"))
                require(iv.size == 12 && key.size == 32)
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
                cipher.updateAAD(aad)
                val payload = JSONObject(String(cipher.doFinal(decoder.decode(json.getString("payload"))), Charsets.UTF_8))
                return payload.getString("name") to payload.getString("config")
            } finally { key.fill(0) }
        } catch (_: Exception) {
            throw IllegalArgumentException("This protected profile belongs to another Lumen installation or is damaged")
        }
    }

    fun isEnvelope(text: String): Boolean = text.length <= MAX_BYTES && text.trim().startsWith("{") &&
        runCatching { JSONObject(text).optString("format") == "lumen-locked" }.getOrDefault(false)
    fun reference(envelope: String, index: Int): String = PREFIX +
        encoder.encodeToString(envelope.toByteArray()) + "/$index"
    fun referenceParts(reference: String): Pair<String, Int> {
        require(reference.startsWith(PREFIX) && reference.length <= MAX_BYTES * 3)
        val body = reference.removePrefix(PREFIX)
        return String(decoder.decode(body.substringBeforeLast('/')), Charsets.UTF_8) to body.substringAfterLast('/').toInt()
    }
}
