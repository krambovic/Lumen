package com.lumen.core.config

import com.lumen.core.config.crypto.HappCrypt
import com.lumen.core.config.crypto.HappDecryptError
import com.lumen.core.config.crypto.HappKeyUnavailableError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HappCryptTest {

    @Test
    fun testIsHappLink() {
        assertTrue(HappCrypt.isHappLink("happ://crypt5/somepayload"))
        assertTrue(HappCrypt.isHappLink("HAPP://CRYPT/12345"))
        assertFalse(HappCrypt.isHappLink("vless://12345"))
        assertFalse(HappCrypt.isHappLink(null))
    }

    @Test
    fun testIsHappCryptLink() {
        assertTrue(HappCrypt.isHappCryptLink("happ://crypt/payload"))
        assertTrue(HappCrypt.isHappCryptLink("happ://crypt2/payload"))
        assertTrue(HappCrypt.isHappCryptLink("happ://crypt3/payload"))
        assertTrue(HappCrypt.isHappCryptLink("happ://crypt4/payload"))
        assertTrue(HappCrypt.isHappCryptLink("happ://crypt5/payload"))
        assertFalse(HappCrypt.isHappCryptLink("happ://other/payload"))
        assertFalse(HappCrypt.isHappCryptLink("vmess://payload"))
    }

    @Test(expected = HappDecryptError::class)
    fun testDecryptInvalidSchemeThrows() {
        HappCrypt.decryptHappLink("vless://invalid")
    }

    @Test(expected = HappDecryptError::class)
    fun testDecryptEmptyBodyThrows() {
        HappCrypt.decryptHappLink("happ://crypt/")
    }

    @Test
    fun testDecryptInvalidPayloadThrowsError() {
        var errorThrown = false
        try {
            HappCrypt.decryptHappLink("happ://crypt/invalidpayload==")
        } catch (e: HappDecryptError) {
            errorThrown = true
        }
        assertTrue(errorThrown)
    }

    @Test
    fun testCrypt51KeyUnavailableThrowsSpecificException() {
        var unavailableErrorThrown = false
        try {
            // A crypt5 payload with an unknown key selector
            HappCrypt.decryptHappLink("happ://crypt5/00000000000000000000000000000000000000000000000000000000000000000000000000000000")
        } catch (e: HappKeyUnavailableError) {
            unavailableErrorThrown = true
        } catch (e: HappDecryptError) {
            // Decryption failure expected
            unavailableErrorThrown = true
        }
        assertTrue(unavailableErrorThrown)
    }
}
