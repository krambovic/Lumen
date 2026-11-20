package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SubscriptionEditValidationTest {
    @Test
    fun encryptedSubscriptionSourcesCanBeAddedAndRenamed() {
        val links = listOf(
            "happ://crypt/MYuWn7D9+SuUMnwuAO4aDNJtyglzW+mcEwGG19GeoxdaQijb+/U1JnlLMT0mRoKVWQB/ZtzfANf8koLv7Iq62LuhsGRASD4Gf+6JDAx/Ubk8LYBZifSulN7JHV5HXsqW1HeBRT/DLQ997XTfiDXxYTU+Ed0shUcucvOdXq4Vx1I=",
            "incy://crypt1/AAECAwQFBgcICQoLNyIQL3rDwRZqnyoD8pGKSKPC6cwYYSGTRieILcbBXDI2_rVLW0ACi9IVK63oUXhNjJAgucw3__4DCG5s9js"
        )
        links.forEach { link ->
            assertEquals(SubscriptionEdit("Group", link), validateSubscriptionEdit("", "Group", link, false))
            assertEquals(SubscriptionEdit("Renamed", link), validateSubscriptionEdit(link, "Renamed", link, false))
        }
        assertNull(validateSubscriptionEdit("", "Group", "happ://crypt/broken", false))
        assertNull(validateSubscriptionEdit("", "Group", "incy://crypt1/broken", false))
    }
    @Test
    fun acceptsHttpsAndNormalizesTheName() {
        assertEquals(
            SubscriptionEdit("My group", "https://example.com/sub?token=1"),
            validateSubscriptionEdit(
                existingUrl = "https://old.example/sub",
                name = "  My group  ",
                url = " https://example.com/sub?token=1 ",
                allowHttp = false
            )
        )
    }

    @Test
    fun plainHttpRequiresTheSettingForANewUrl() {
        assertNull(
            validateSubscriptionEdit(
                existingUrl = "https://old.example/sub",
                name = "Group",
                url = "http://example.com/sub",
                allowHttp = false
            )
        )
        assertEquals(
            "http://example.com/sub",
            validateSubscriptionEdit(
                existingUrl = "https://old.example/sub",
                name = "Group",
                url = "http://example.com/sub",
                allowHttp = true
            )?.url
        )
    }

    @Test
    fun existingHttpCanBeKeptWhenOnlyTheNameChanges() {
        assertEquals(
            "Renamed",
            validateSubscriptionEdit(
                existingUrl = "http://example.com/sub",
                name = "Renamed",
                url = "http://example.com/sub",
                allowHttp = false
            )?.name
        )
    }

    @Test
    fun providerTitleIsOnlyAdoptedForAnUnnamedFirstImport() {
        assertEquals(
            "Provider title",
            refreshedSubscriptionName("example.com", " Provider title ", adoptProviderTitle = true)
        )
        assertEquals(
            "My custom name",
            refreshedSubscriptionName("My custom name", "Provider title", adoptProviderTitle = false)
        )
    }

    @Test
    fun rejectsMalformedOrUnsafeInput() {
        assertNull(validateSubscriptionEdit("", "", "https://example.com/sub", true))
        assertNull(validateSubscriptionEdit("", "Group", "ftp://example.com/sub", true))
        assertNull(validateSubscriptionEdit("", "Group", "https:///missing-host", true))
        assertNull(validateSubscriptionEdit("", "Group", "https://example.com\r\nInjected: x", true))
    }
}
