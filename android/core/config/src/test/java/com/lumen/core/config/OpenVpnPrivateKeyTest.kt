package com.lumen.core.config

import com.lumen.core.config.crypto.OpenVpnPrivateKey
import java.io.StringWriter
import java.security.KeyPairGenerator
import java.util.Base64
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.openssl.jcajce.JcaPEMWriter
import org.bouncycastle.openssl.jcajce.JcePEMEncryptorBuilder
import org.junit.Assert.*
import org.junit.Test

class OpenVpnPrivateKeyTest {
    @Test fun decryptsTraditionalRsaKeyAndRejectsWrongOrMissingPassword() {
        val pair = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
        val writer = StringWriter()
        JcaPEMWriter(writer).use {
            it.writeObject(pair, JcePEMEncryptorBuilder("AES-256-CBC")
                .setProvider(BouncyCastleProvider()).build("test passphrase".toCharArray()))
        }
        val encrypted = writer.toString()
        val decrypted = OpenVpnPrivateKey.decryptIfNeeded(encrypted, "test passphrase")
        val der = Base64.getMimeDecoder().decode(decrypted.lines().filterNot { it.startsWith("---") }.joinToString(""))
        assertArrayEquals(pair.private.encoded, der)
        assertTrue(encrypted.contains("ENCRYPTED"))
        assertThrows(IllegalArgumentException::class.java) { OpenVpnPrivateKey.decryptIfNeeded(encrypted, "incorrect") }
        assertThrows(IllegalArgumentException::class.java) { OpenVpnPrivateKey.decryptIfNeeded(encrypted, "") }
        assertEquals(decrypted, OpenVpnPrivateKey.decryptIfNeeded(decrypted, ""))
    }
}
