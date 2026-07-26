package com.lumen.app.vm

import com.lumen.app.subscription.SubscriptionMetadata
import com.lumen.core.database.model.SubscriptionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The card must not blank out on a refresh that returns fewer headers — the same rule
 * the traffic figures already follow. Panels drop `announce` between refreshes, and a
 * response served to a fallback User-Agent often carries almost no metadata at all.
 */
class SubscriptionMetadataMergeTest {

    private val stored = SubscriptionEntity(
        id = "sub-1",
        name = "MaviksVPN",
        url = "https://maviks.app/sub/token",
        announce = "Old announcement",
        announceUrl = "https://t.me/maviks",
        supportUrl = "https://t.me/maviks_bot",
        websiteUrl = "https://maviks.app",
        bannerText = "Old banner",
        bannerBgColor = "#112233",
        hideUrl = true,
        sortOrder = "ping",
        updateIntervalHours = 12
    )

    @Test
    fun anEmptyResponseKeepsEveryStoredValue() {
        val merged = mergeSubscriptionMetadata(stored, SubscriptionMetadata(), null)
        assertEquals(stored, merged)
    }

    @Test
    fun onlyTheFieldsThePanelSentAreReplaced() {
        val merged = mergeSubscriptionMetadata(
            stored,
            SubscriptionMetadata(announce = "New announcement", supportEmail = "help@maviks.app"),
            6
        )
        assertEquals("New announcement", merged.announce)
        assertEquals("help@maviks.app", merged.supportEmail)
        assertEquals(6, merged.updateIntervalHours)
        // Untouched by this refresh.
        assertEquals("https://t.me/maviks", merged.announceUrl)
        assertEquals("https://t.me/maviks_bot", merged.supportUrl)
        assertEquals("Old banner", merged.bannerText)
        assertEquals("#112233", merged.bannerBgColor)
        assertEquals(true, merged.hideUrl)
        assertEquals("ping", merged.sortOrder)
    }

    @Test
    fun aFlagIsOnlyClearedWhenThePanelActuallySendsIt() {
        assertEquals(true, mergeSubscriptionMetadata(stored, SubscriptionMetadata(), null).hideUrl)
        assertEquals(
            false,
            mergeSubscriptionMetadata(stored, SubscriptionMetadata(hideUrl = false), null).hideUrl
        )
    }

    @Test
    fun providerTextIsBounded() {
        val merged = mergeSubscriptionMetadata(
            stored,
            SubscriptionMetadata(announce = "x".repeat(5_000), bannerButtonText = "y".repeat(500)),
            null
        )
        assertTrue(merged.announce.length <= 400)
        assertTrue(merged.bannerButtonText.length <= 80)
    }

    @Test
    fun identityFieldsAreNeverTouched() {
        val merged = mergeSubscriptionMetadata(
            stored,
            SubscriptionMetadata(announce = "New"),
            3
        )
        assertEquals("sub-1", merged.id)
        assertEquals("MaviksVPN", merged.name)
        assertEquals("https://maviks.app/sub/token", merged.url)
        assertEquals(stored.lastUpdated, merged.lastUpdated)
    }
}
