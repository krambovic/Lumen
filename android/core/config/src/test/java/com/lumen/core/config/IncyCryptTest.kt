package com.lumen.core.config

import com.lumen.core.config.crypto.IncyCrypt
import com.lumen.core.config.crypto.IncyDecryptError
import com.lumen.core.config.parser.LinkParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class IncyCryptTest {
    private val link =
        "incy://crypt1/AAECAwQFBgcICQoLNyIQL3rDwRZqnyoD8pGKSKPC6cwYYSGTRieILcbBXDI2_rVLW0ACi9IVK63oUXhNjJAgucw3__4DCG5s9js"
    private val officialExample =
        "incy://crypt1/FZEVXuV39UEX1yHB3nkrgdPdrJ3syVxcQm_Y-lY0oKWAT5yRn00xe6ohg06aVWjWRrGJ7BAeEzuoFzv8XBosLnqnqqCMbnAJmR7EN2hII4Yyql1FtWlLlLs"

    @Test
    fun decryptsCrypt1Vector() {
        assertTrue(IncyCrypt.isIncyLink(link))
        assertTrue(IncyCrypt.isIncyCryptLink(link))
        assertEquals("https://example.com/sub?id=incy-test", IncyCrypt.decryptIncyLink(link))
    }

    @Test
    fun decryptsOfficialDocumentationExample() {
        assertEquals(
            "https://incsub.myincteam.org/vTyt0xVE-aAjHv8T",
            IncyCrypt.decryptIncyLink(officialExample)
        )
    }

    @Test(expected = IncyDecryptError::class)
    fun rejectsTamperedCiphertext() {
        IncyCrypt.decryptIncyLink(link.dropLast(1) + "A")
    }

    @Test
    fun parserRoutesWrappedUrlToSubscriptionFlow() {
        val (nodes, errors) = LinkParser.parseLinksText(link)
        assertTrue(nodes.isEmpty())
        assertEquals(listOf(LinkParser.INCY_SUBSCRIPTION_ERROR), errors)
    }
}
