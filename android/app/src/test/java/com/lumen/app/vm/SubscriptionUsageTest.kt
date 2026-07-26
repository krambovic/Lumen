package com.lumen.app.vm

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SubscriptionUsageTest {

    private val gib = 1024L * 1024L * 1024L

    @Test
    fun uploadCountsTowardsTheQuota() {
        // Download-only accounting under-reported usage on every panel that meters
        // both directions, which is what made "remaining" look wrong.
        val info = mapOf("upload" to 2L * gib, "download" to 3L * gib, "total" to 10L * gib)
        assertEquals(5L * gib, SubscriptionUsage.used(info))
        assertEquals(5L * gib, SubscriptionUsage.remaining(info))
        assertEquals(0.5f, SubscriptionUsage.ratio(info)!!, 0.0001f)
    }

    @Test
    fun missingUploadIsNotAnError() {
        val info = mapOf("download" to 3L * gib, "total" to 10L * gib)
        assertEquals(3L * gib, SubscriptionUsage.used(info))
        assertEquals(7L * gib, SubscriptionUsage.remaining(info))
    }

    @Test
    fun zeroOrMissingTotalMeansUnlimited() {
        // Not "nothing left": a zero quota is how panels report an uncapped plan.
        assertNull(SubscriptionUsage.totalOrUnlimited(mapOf("total" to 0L)))
        assertNull(SubscriptionUsage.totalOrUnlimited(mapOf("download" to gib)))
        assertNull(SubscriptionUsage.remaining(mapOf("download" to gib, "total" to 0L)))
        assertNull(SubscriptionUsage.ratio(mapOf("download" to gib, "total" to 0L)))
        assertEquals("1.0 GB / ∞", SubscriptionUsage.summary(mapOf("download" to gib, "total" to 0L)))
    }

    @Test
    fun overspentQuotaClampsInsteadOfGoingNegative() {
        val info = mapOf("upload" to 8L * gib, "download" to 8L * gib, "total" to 10L * gib)
        assertEquals(0L, SubscriptionUsage.remaining(info))
        assertEquals(1f, SubscriptionUsage.ratio(info)!!, 0.0001f)
    }

    @Test
    fun summaryShowsUsedOverTotal() {
        assertEquals(
            "3.0 GB / 10.0 GB",
            SubscriptionUsage.summary(mapOf("upload" to gib, "download" to 2L * gib, "total" to 10L * gib))
        )
    }

    @Test
    fun zeroOrMissingExpiryMeansNoExpiry() {
        assertNull(SubscriptionUsage.expiryEpochSeconds(emptyMap()))
        assertNull(SubscriptionUsage.expiryEpochSeconds(mapOf("expire" to 0L)))
        assertEquals(1767225600L, SubscriptionUsage.expiryEpochSeconds(mapOf("expire" to 1767225600L)))
    }

    @Test
    fun daysLeftTreatsTheValueAsSeconds() {
        val nowMs = 1_767_225_600_000L
        val expireSec = 1_767_225_600L + 10L * 86_400L
        assertEquals(10, SubscriptionUsage.daysLeft(expireSec, nowMs))
        // A part-day rounds up, so "18 hours left" is not shown as 0 days.
        assertEquals(1, SubscriptionUsage.daysLeft(1_767_225_600L + 64_800L, nowMs))
        // Already elapsed, and never negative.
        assertEquals(0, SubscriptionUsage.daysLeft(1_767_225_600L - 86_400L, nowMs))
    }

    @Test
    fun absurdExpiryDoesNotOverflow() {
        // A stored value past year 9999 is clamped instead of wrapping the millisecond
        // multiplication into a negative number (which read as "expired").
        assertEquals(
            SubscriptionUsage.daysLeft(253_402_300_799L, 0L),
            SubscriptionUsage.daysLeft(Long.MAX_VALUE, 0L)
        )
    }

    @Test
    fun mergeKeepsFieldsTheRefreshDidNotSend() {
        val stored = mapOf("upload" to 1L, "download" to 2L, "total" to 10L, "expire" to 1767225600L)
        // The panel answered with traffic only: the expiry it already knew must survive.
        val merged = SubscriptionUsage.merge(stored, mapOf("upload" to 3L, "download" to 4L))
        assertEquals(3L, merged["upload"])
        assertEquals(4L, merged["download"])
        assertEquals(10L, merged["total"])
        assertEquals(1767225600L, merged["expire"])
    }

    @Test
    fun mergeOfAnEmptyRefreshChangesNothing() {
        // A fallback User-Agent whose response carries no userinfo header at all.
        val stored = mapOf("upload" to 1L, "download" to 2L, "total" to 10L)
        assertEquals(stored, SubscriptionUsage.merge(stored, emptyMap()))
    }

    @Test
    fun mergeAppliesAnExplicitZero() {
        // A plan that became unlimited does send total = 0, and that must win.
        val merged = SubscriptionUsage.merge(mapOf("total" to 10L), mapOf("total" to 0L))
        assertEquals(0L, merged["total"])
        assertNull(SubscriptionUsage.totalOrUnlimited(merged))
    }
}
