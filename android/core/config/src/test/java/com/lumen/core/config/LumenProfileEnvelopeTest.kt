package com.lumen.core.config

import com.lumen.core.config.crypto.LumenProfileEnvelope
import com.lumen.core.config.crypto.LumenProfileEnvelope.Delivery
import com.lumen.core.config.crypto.LumenProfileEnvelope.Profile
import com.lumen.core.config.crypto.ProtectedProfileCodec
import java.security.KeyPairGenerator
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

/** Distribution encryption, authentication, compatibility and hostile-input checks. */
class LumenProfileEnvelopeTest {
    private val content = Profile("Тест ایران", "vless://test@vpn.example:443#Node")
    private fun keyPair() = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()

    @Test fun everyNamedRecipientCanOpenTheSameFileButAnUnlistedKeyCannot() {
        val first = keyPair()
        val second = keyPair()
        val other = keyPair()
        val file = LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, listOf(first.public, second.public))
        assertEquals(content, LumenProfileEnvelope.open(file, first.private))
        assertEquals(content, LumenProfileEnvelope.open(file, second.private))
        assertTrue(runCatching { LumenProfileEnvelope.open(file, other.private) }.isFailure)
    }

    @Test fun passwordDeliveryDoesNotRequireARecipientKey() {
        val password = "a long separate password".toCharArray()
        try {
            val file = LumenProfileEnvelope.seal(content, Delivery.PASSWORD, password = password)
            assertTrue(LumenProfileEnvelope.requiresPassword(file))
            assertEquals(content, LumenProfileEnvelope.open(file, password = password))
            assertFalse(file.contains(content.config))
            assertTrue(runCatching { LumenProfileEnvelope.open(file, password = "incorrect password".toCharArray()) }.isFailure)
        } finally { password.fill('\u0000') }
    }

    @Test fun changingTheEncryptedLockFlagOrPayloadIsDetected() {
        val recipient = keyPair()
        val root = JSONObject(LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, listOf(recipient.public)))
        val encrypted = root.getString("payload").toCharArray()
        encrypted[0] = if (encrypted[0] == 'A') 'B' else 'A'
        root.put("payload", String(encrypted))
        assertTrue(runCatching { LumenProfileEnvelope.open(root.toString(), recipient.private) }.isFailure)
    }

    @Test fun openDeliveryKeepsTheExplicitLockChoiceAndAdvertisesNoEncryption() {
        for (locked in listOf(true, false)) {
            val profile = content.copy(locked = locked)
            val file = LumenProfileEnvelope.seal(profile, Delivery.ANYONE)
            assertEquals("ANYONE", JSONObject(file).getString("delivery"))
            assertEquals(profile, LumenProfileEnvelope.open(file))
            assertFalse(LumenProfileEnvelope.requiresPassword(file))
        }
    }

    @Test fun existingVersionOneRecipientFilesRemainImportable() {
        val recipient = keyPair()
        val file = ProtectedProfileCodec.seal(content.name, content.config, recipient.public)
        assertEquals(content, LumenProfileEnvelope.open(file, recipient.private))
    }

    @Test fun unsupportedDerivationAndUnboundedRecipientListsAreRejected() {
        val recipient = keyPair()
        assertTrue(runCatching { LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, List(17) { recipient.public }) }.isFailure)
        val root = JSONObject(LumenProfileEnvelope.seal(content, Delivery.PASSWORD, password = "a strong password".toCharArray()))
        root.put("iterations", Int.MAX_VALUE)
        assertTrue(runCatching { LumenProfileEnvelope.open(root.toString(), password = "a strong password".toCharArray()) }.isFailure)
    }

    @Test fun randomKeysAndNoncesMakeRepeatedExportsDifferentAndMetadataCannotUnlockThem() {
        val recipient = keyPair()
        val first = LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, listOf(recipient.public))
        val second = LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, listOf(recipient.public))
        assertNotEquals(first, second)
        assertNotEquals(JSONObject(first).getString("iv"), JSONObject(second).getString("iv"))
        val root = JSONObject(first).put("locked", false)
        assertTrue(LumenProfileEnvelope.open(root.toString(), recipient.private).locked)
        root.put("iv", JSONObject(second).getString("iv"))
        assertTrue(runCatching { LumenProfileEnvelope.open(root.toString(), recipient.private) }.isFailure)
        assertTrue(runCatching { LumenProfileEnvelope.open(first.dropLast(20), recipient.private) }.isFailure)
    }

    @Test fun passwordCostAndKeySizesAreBounded() {
        val file = LumenProfileEnvelope.seal(content, Delivery.PASSWORD, password = "a separate passphrase".toCharArray())
        assertEquals(600_000, JSONObject(file).getInt("iterations"))
        assertTrue(runCatching { LumenProfileEnvelope.seal(content, Delivery.PASSWORD, password = "short".toCharArray()) }.isFailure)
        assertTrue(runCatching { LumenProfileEnvelope.seal(content, Delivery.PASSWORD, password = CharArray(1025) { 'a' }) }.isFailure)
        val weak = KeyPairGenerator.getInstance("RSA").apply { initialize(1024) }.generateKeyPair()
        assertTrue(runCatching { LumenProfileEnvelope.seal(content, Delivery.RECIPIENTS, listOf(weak.public)) }.isFailure)
    }

    @Test fun earlyVersionTwoPasswordFilesRemainReadable() {
        val password = "legacy separate passphrase".toCharArray()
        val salt = ByteArray(16) { it.toByte() }
        val spec = javax.crypto.spec.PBEKeySpec(password, salt, 210_000, 256)
        val key = javax.crypto.SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded
        val cipher = javax.crypto.Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(javax.crypto.Cipher.ENCRYPT_MODE, javax.crypto.spec.SecretKeySpec(key, "AES"))
        cipher.updateAAD("Lumen profile v2:PASSWORD".toByteArray(Charsets.UTF_8))
        val encoder = java.util.Base64.getUrlEncoder().withoutPadding()
        val root = JSONObject().put("format", "lumen-locked").put("version", 2).put("delivery", "PASSWORD")
            .put("iterations", 210_000).put("salt", encoder.encodeToString(salt)).put("iv", encoder.encodeToString(cipher.iv))
            .put("payload", encoder.encodeToString(cipher.doFinal(JSONObject().put("name", content.name)
                .put("config", content.config).put("locked", content.locked).toString().toByteArray(Charsets.UTF_8))))
        try { assertEquals(content, LumenProfileEnvelope.open(root.toString(), password = password)) }
        finally { key.fill(0); password.fill('\u0000'); spec.clearPassword() }
    }
}
