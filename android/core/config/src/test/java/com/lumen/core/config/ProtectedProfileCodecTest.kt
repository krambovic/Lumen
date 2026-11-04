package com.lumen.core.config

import com.lumen.core.config.crypto.ProtectedProfileCodec
import java.security.KeyPairGenerator
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ProtectedProfileCodecTest {
    private fun key() = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
    @Test fun encryptedAuthenticatedRecipientBoundRoundTrip() {
        val key = key()
        val source = "vless://secret@example.invalid:443?encryption=none"
        val file = ProtectedProfileCodec.seal("Private", source, key.public)
        assertFalse(file.contains("secret"))
        assertFalse(file.contains("example.invalid"))
        assertEquals("Private" to source, ProtectedProfileCodec.open(file, key.private))
        val reference = ProtectedProfileCodec.reference(file, 2)
        assertEquals(file to 2, ProtectedProfileCodec.referenceParts(reference))
        assertTrue(runCatching { ProtectedProfileCodec.open(file, key().private) }.isFailure)
        val tampered = JSONObject(file).put("payload", "AAAA").toString()
        assertTrue(runCatching { ProtectedProfileCodec.open(tampered, key.private) }.isFailure)
        assertTrue(runCatching { ProtectedProfileCodec.open(JSONObject(file).put("version", 2).toString(), key.private) }.isFailure)
    }
}
