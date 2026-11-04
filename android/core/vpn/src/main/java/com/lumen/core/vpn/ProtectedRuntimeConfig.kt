package com.lumen.core.vpn

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Device-bound cache encryption for protected profiles started by a tile/widget. */
internal object ProtectedRuntimeConfig {
    private const val ALIAS = "lumen-protected-runtime-v1"
    private const val PREFIX = "lumen-runtime-v1:"
    private val aad = "Lumen protected runtime v1".toByteArray(Charsets.UTF_8)

    fun isEncrypted(value: String): Boolean = value.startsWith(PREFIX)

    fun seal(config: String): String {
        val plaintext = config.toByteArray(Charsets.UTF_8)
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.ENCRYPT_MODE, key(create = true))
                updateAAD(aad)
            }
            PREFIX + Base64.getEncoder().encodeToString(cipher.iv + cipher.doFinal(plaintext))
        } finally { plaintext.fill(0) }
    }

    fun open(value: String): String {
        require(isEncrypted(value))
        val bytes = Base64.getDecoder().decode(value.removePrefix(PREFIX))
        require(bytes.size >= 12 + 16)
        val plaintext = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, key(create = false), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
            updateAAD(aad)
        }.doFinal(bytes, 12, bytes.size - 12)
        return try { plaintext.toString(Charsets.UTF_8) } finally { plaintext.fill(0) }
    }

    @Synchronized private fun key(create: Boolean): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        check(create) { "Protected runtime key unavailable" }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build())
        }.generateKey()
    }
}
