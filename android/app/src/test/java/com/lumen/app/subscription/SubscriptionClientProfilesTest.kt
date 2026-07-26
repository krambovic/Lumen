package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SubscriptionClientProfilesTest {
    @Test
    fun lumenProfileIsTriedFirst() {
        val profiles = SubscriptionClient.clientProfiles()

        assertEquals("Lumen Android", profiles.first().first)
        assertEquals(SubscriptionClient.lumenUserAgent, profiles.first().second)
    }

    @Test
    fun lumenUserAgentCarriesTheAppVersion() {
        val userAgent = SubscriptionClient.lumenUserAgent

        assertTrue(userAgent, userAgent.startsWith("Lumen-Subscription/Android-"))
        assertTrue(userAgent, userAgent.removePrefix("Lumen-Subscription/Android-").isNotBlank())
    }

    @Test
    fun happIsOnlyAFallbackAfterLumen() {
        val names = SubscriptionClient.clientProfiles().map { it.first }

        assertTrue(names.toString(), names.indexOf("Lumen Android") < names.indexOf("Happ compatible"))
        assertEquals(1, names.indexOf("Happ compatible"))
    }

    @Test
    fun customUserAgentStillWins() {
        val profiles = SubscriptionClient.clientProfiles("CustomClient/2.0")

        assertEquals("Custom" to "CustomClient/2.0", profiles.first())
        assertEquals("Lumen Android", profiles[1].first)
        assertEquals("Happ compatible", profiles[2].first)
    }

    @Test
    fun blankCustomUserAgentIsIgnored() {
        assertEquals("Lumen Android", SubscriptionClient.clientProfiles("   ").first().first)
    }

    @Test
    fun profileNamesAreUnique() {
        val names = SubscriptionClient.clientProfiles("CustomClient/2.0").map { it.first }

        assertEquals(names.size, names.toSet().size)
    }

    @Test
    fun clientNotSupportedStubsAreRecognisedInBothLanguages() {
        val stubs = listOf(
            "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1#Your%20client%20is%20not%20supported",
            "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1#Please%20update%20your%20app",
            // "\u0412\u0430\u0448 \u043a\u043b\u0438\u0435\u043d\u0442 \u043d\u0435 \u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u0442\u0441\u044f", sent verbatim.
            "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1#" +
                "\u0412\u0430\u0448 \u043a\u043b\u0438\u0435\u043d\u0442 \u043d\u0435 " +
                "\u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u0442\u0441\u044f",
            // "\u041f\u0440\u0438\u043b\u043e\u0436\u0435\u043d\u0438\u0435 \u043d\u0435 \u043f\u043e\u0434\u0434\u0435\u0440\u0436\u0438\u0432\u0430\u0435\u0442\u0441\u044f", percent encoded in the fragment.
            "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1#" +
                "%D0%9F%D1%80%D0%B8%D0%BB%D0%BE%D0%B6%D0%B5%D0%BD%D0%B8%D0%B5%20" +
                "%D0%BD%D0%B5%20%D0%BF%D0%BE%D0%B4%D0%B4%D0%B5%D1%80%D0%B6%D0%B8%D0%B2%D0%B0%D0%B5%D1%82%D1%81%D1%8F"
        )

        stubs.forEach { assertTrue(it, SubscriptionClient.looksLikePlaceholder(it)) }
    }

    @Test
    fun realSubscriptionBodyIsNotAPlaceholder() {
        val body = "vless://00000000-0000-0000-0000-000000000001@one.example:443" +
            "?encryption=none&type=tcp&security=none#Netherlands"

        assertFalse(SubscriptionClient.looksLikePlaceholder(body))
    }

    @Test
    fun blankBodyIsAPlaceholder() {
        assertTrue(SubscriptionClient.looksLikePlaceholder("   "))
    }
}
