package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SubscriptionEditValidationTest {
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
