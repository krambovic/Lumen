package com.lumen.core.config.crypto

import java.io.StringReader
import java.io.StringWriter
import org.bouncycastle.asn1.pkcs.PrivateKeyInfo
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.openssl.PEMEncryptedKeyPair
import org.bouncycastle.openssl.PEMParser
import org.bouncycastle.openssl.jcajce.JceOpenSSLPKCS8DecryptorProviderBuilder
import org.bouncycastle.openssl.jcajce.JcePEMDecryptorProviderBuilder
import org.bouncycastle.pkcs.PKCS8EncryptedPrivateKeyInfo
import org.bouncycastle.util.io.pem.PemObject
import org.bouncycastle.util.io.pem.PemWriter

/** Decrypt only the runtime copy; the stored/imported profile keeps its encrypted key. */
internal object OpenVpnPrivateKey {
    private val provider by lazy { BouncyCastleProvider() }

    fun decryptIfNeeded(pem: String, password: String): String {
        if (!pem.contains("ENCRYPTED PRIVATE KEY") &&
            !(pem.contains("Proc-Type:", ignoreCase = true) && pem.contains("ENCRYPTED"))) return pem
        require(password.isNotEmpty()) { "OpenVPN private key is encrypted and requires a private key password" }
        val passphrase = password.toCharArray()
        try {
            val key: PrivateKeyInfo = PEMParser(StringReader(pem)).use { parser ->
                when (val value = parser.readObject()) {
                    is PEMEncryptedKeyPair -> value.decryptKeyPair(
                        JcePEMDecryptorProviderBuilder().setProvider(provider).build(passphrase)
                    ).privateKeyInfo
                    is PKCS8EncryptedPrivateKeyInfo -> value.decryptPrivateKeyInfo(
                        JceOpenSSLPKCS8DecryptorProviderBuilder().setProvider(provider).build(passphrase)
                    )
                    else -> throw IllegalArgumentException("Unsupported encrypted OpenVPN private key")
                }
            }
            val writer = StringWriter()
            PemWriter(writer).use { it.writeObject(PemObject("PRIVATE KEY", key.encoded)) }
            return writer.toString()
        } catch (_: Exception) {
            // Crypto exceptions can contain key material; expose only an actionable message.
            throw IllegalArgumentException("Could not decrypt OpenVPN private key; check the private key password")
        } finally {
            passphrase.fill('\u0000')
        }
    }
}
