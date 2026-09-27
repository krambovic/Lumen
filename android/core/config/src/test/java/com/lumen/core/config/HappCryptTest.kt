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
    fun testDecryptsCrypt51SubscriptionVector() {
        val link =
            "happ://crypt5/fzvd4oXqWHPd9ZJzbmZcpU3I20FsDc8WfLpIJg8yO6G9p/GbNqkmpD1avm2fTYWsJmVeKxs/zdzR8yugTK73iS" +
            "H6DXZ+Z/U6KivYcEeNBtYcSrziaK5+PDLsBMsCL1qwyDpXGn3esHXxj9tXNE/t0mmHiJycS6n6B3TnrpXNsBcpEUgji9oORF46JK" +
            "0i5xwpAXrDNqY/4hLaGJhK0X4hoFkyuqx8M1VKXabyVq9q0geu84PwTPH2FOeOh1rKmFNWTMMcOSPG2YjFg6phIgEpoks8fwystr" +
            "TVWV3138pqmeMRwzYthQcatqxRRMsrcwGnhq4mymB813vPboFGHflMcyYT/hpWAz9WfPPWjldEfgMhLHiS0+mznmZsHY9n9ZFU8g" +
            "MHDtbIJTirbukv6V2taTh6wan4a6FWKovf85mIO6iUYbpQE3Uz3czKldiBx/MEFfTA5/k9N3WC1MQG2LddZ6Vod6thWpwaN7/Zhg" +
            "qoHflA1hoV0SDaQ0q+EWI+egMoFrsRs55E91r1yObG5uYw9OZ399Qtv3ecveX98YOF4k8cn0DLrYhm7iCrbpWwLeg4bCFIY9KTq+" +
            "u1TAqNIKxMlm29Tb2tSMFu7zoypz+GacEl00y4lTHpm/FTQtbqHxSSz7GCVYepZXfJkxQkjMf9V53YyrYtsbGhw8mhUnHOtEg0L/" +
            "kHldlqpRqGctgvA1aA7OzpviIoyYv6BvqxblSBQrYIRZEj1WPE8P+rNodlI+6jMC16QFW/b2NWUtzuz7U8+slkCHdTV20hv+GZ6n" +
            "Iap41RKp41OPi5Un+PTkfGailpGazGInwecp8DXYuvudSxZqIeopf8YODcle1iWnSUJkurlnNP55jlmwCffr9c70mf7B+Q6OtMfb" +
            "/f7rL8p3DjQLmzW/Cv+q0l2nCpqAxYM1+Nfos=ff"
        assertEquals(
            "https://heaver.tgmru.ru/6f81573aa5ab4ce46075b7ac2d91a186de26f77b688bb9f5/json?template=default-xray-json",
            HappCrypt.decryptHappLink(link)
        )
    }

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
    fun testCrypt51EmbeddedLengthOneModFourStillBuildsCandidates() {
        // Embedded length n = 9 (n - 1 is a multiple of 4) used to index one past
        // the end of the url region, aborting candidate generation entirely.
        val payload = StringBuilder("A".repeat(20 + 9 + 684))
        payload.setCharAt(18, '0')
        payload.setCharAt(19, '9')

        var keyUnavailable = false
        var otherMessage = ""
        try {
            HappCrypt.decryptHappLink("happ://crypt5/$payload")
        } catch (e: HappKeyUnavailableError) {
            keyUnavailable = true
        } catch (e: HappDecryptError) {
            otherMessage = e.message ?: ""
        }
        assertTrue("expected key-unavailable error, got: $otherMessage", keyUnavailable)
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
