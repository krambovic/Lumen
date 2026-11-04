package com.lumen.core.config.crypto

import java.security.PrivateKey
import java.security.PublicKey
import java.security.SecureRandom
import java.security.interfaces.RSAPublicKey
import java.security.spec.MGF1ParameterSpec
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.OAEPParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.PSource
import javax.crypto.spec.SecretKeySpec
import org.json.JSONArray
import org.json.JSONObject

/** v2 separates recipient encryption from the app's view/edit restriction. */
object LumenProfileEnvelope {
    enum class Delivery { RECIPIENTS, PASSWORD, ANYONE }
    data class Profile(val name: String, val config: String, val locked: Boolean = true,
        val policy: LumenProfilePolicy = LumenProfilePolicy())
    private const val MAX_FILE_BYTES = 8 * 1024 * 1024
    private const val ITERATIONS = 600_000
    private const val LEGACY_ITERATIONS = 210_000
    private val random = SecureRandom()
    private val encoder = Base64.getUrlEncoder().withoutPadding()
    private val decoder = Base64.getUrlDecoder()
    private val oaep = OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT)

    fun requiresPassword(envelope: String): Boolean = runCatching {
        require(envelope.length <= MAX_FILE_BYTES)
        val json = JSONObject(envelope)
        json.optString("format") == "lumen-locked" && json.optInt("version") in 2..3 &&
            json.optString("delivery") == Delivery.PASSWORD.name
    }.getOrDefault(false)

    fun seal(profile: Profile, delivery: Delivery, recipients: List<PublicKey> = emptyList(), password: CharArray = charArrayOf()): String {
        validate(profile)
        if (delivery == Delivery.PASSWORD) require(password.size in 12..1024) { "Use 12 to 1024 characters" }
        if (delivery == Delivery.RECIPIENTS) {
            require(recipients.size in 1..16)
            require(recipients.all { it is RSAPublicKey && it.modulus.bitLength() in 2048..8192 })
        }
        val payload = JSONObject().put("name", profile.name.take(120)).put("config", profile.config)
            .put("locked", profile.locked)
            .put("policy", profile.policy.toJson())
            .toString().toByteArray(Charsets.UTF_8)
        val root = JSONObject().put("format", "lumen-locked").put("version", 3).put("delivery", delivery.name)
        if (delivery == Delivery.ANYONE) {
            // No application-wide secret: public delivery is explicitly plaintext.
            try {
                root.put("profile", JSONObject(payload.toString(Charsets.UTF_8)))
                return root.toString().also { require(it.toByteArray().size <= MAX_FILE_BYTES) }
            } finally { payload.fill(0) }
        }
        val salt = ByteArray(16).also(random::nextBytes)
        val key = if (delivery == Delivery.PASSWORD) {
            root.put("salt", encoder.encodeToString(salt)).put("iterations", ITERATIONS)
            passwordKey(password, salt, ITERATIONS)
        } else {
            require(recipients.size in 1..16)
            ByteArray(32).also(random::nextBytes)
        }
        try {
            if (delivery == Delivery.RECIPIENTS) root.put("keys", JSONArray(recipients.map { recipient ->
                encoder.encodeToString(Cipher.getInstance("RSA/ECB/OAEPPadding").apply {
                    init(Cipher.ENCRYPT_MODE, recipient, oaep)
                }.doFinal(key))
            }))
            val iv = ByteArray(12).also(random::nextBytes)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
                updateAAD(aad(delivery, 3))
            }
            return root.put("iv", encoder.encodeToString(iv))
                .put("payload", encoder.encodeToString(cipher.doFinal(payload))).toString()
                .also { require(it.toByteArray().size <= MAX_FILE_BYTES) { "Profile is too large" } }
        } finally { key.fill(0); payload.fill(0) }
    }

    fun open(envelope: String, recipient: PrivateKey? = null, password: CharArray = charArrayOf()): Profile {
        require(envelope.length <= MAX_FILE_BYTES && envelope.toByteArray().size <= MAX_FILE_BYTES) { "Profile is too large" }
        val root = JSONObject(envelope)
        require(root.getString("format") == "lumen-locked")
        if (root.getInt("version") == 1) {
            val (name, config) = ProtectedProfileCodec.open(envelope, requireNotNull(recipient))
            return Profile(name, config).also(::validate)
        }
        val version = root.getInt("version")
        require(version in 2..3)
        val delivery = Delivery.valueOf(root.getString("delivery"))
        if (delivery == Delivery.ANYONE) return profile(root.getJSONObject("profile"), version)
        val key = when (delivery) {
            Delivery.PASSWORD -> {
                require(password.size in 1..1024) { "Password required" }
                val iterations = root.getInt("iterations")
                require(iterations == ITERATIONS || iterations == LEGACY_ITERATIONS) { "Unsupported key derivation" }
                val salt = decoder.decode(root.getString("salt"))
                require(salt.size == 16)
                passwordKey(password, salt, iterations)
            }
            Delivery.RECIPIENTS -> {
                val keys = root.getJSONArray("keys")
                require(keys.length() in 1..16)
                val privateKey = requireNotNull(recipient) { "Recipient key required" }
                (0 until keys.length()).firstNotNullOfOrNull { index ->
                    runCatching {
                        val wrapped = decoder.decode(keys.getString(index))
                        require(wrapped.size in 256..1024)
                        Cipher.getInstance("RSA/ECB/OAEPPadding").apply {
                            init(Cipher.DECRYPT_MODE, privateKey, oaep)
                        }.doFinal(wrapped)
                    }.getOrNull()
                } ?: error("This file is for another recipient")
            }
            Delivery.ANYONE -> error("Unreachable")
        }
        try {
            require(key.size == 32)
            val iv = decoder.decode(root.getString("iv"))
            require(iv.size == 12)
            val plaintext = Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
                updateAAD(aad(delivery, version))
            }.doFinal(decoder.decode(root.getString("payload")))
            return try { profile(JSONObject(plaintext.toString(Charsets.UTF_8)), version) } finally { plaintext.fill(0) }
        } finally { key.fill(0) }
    }

    private fun profile(json: JSONObject, version: Int): Profile = Profile(json.getString("name"), json.getString("config"),
        json.getBoolean("locked"), if (version >= 3) LumenProfilePolicy.fromJson(json.getJSONObject("policy"))
            else LumenProfilePolicy()).also(::validate)

    private fun validate(profile: Profile) {
        require(profile.config.toByteArray().size in 1..MAX_FILE_BYTES)
        require(profile.name.length <= 120)
        profile.policy.validate()
        require(profile.locked || !profile.policy.restricted) { "Restrictions require a closed profile" }
    }

    private fun aad(delivery: Delivery, version: Int) = "Lumen profile v$version:${delivery.name}".toByteArray(Charsets.UTF_8)
    private fun passwordKey(password: CharArray, salt: ByteArray, iterations: Int): ByteArray {
        val spec = PBEKeySpec(password, salt, iterations, 256)
        return try { SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded }
            finally { spec.clearPassword() }
    }
}
