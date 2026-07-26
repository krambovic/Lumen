package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `subscription-userinfo` parsing. The bodies here are deliberately plain links: the
 * JSON branch of [SubscriptionClient.normalize] needs org.json, which is a stub in
 * unit tests, and it is skipped for anything that does not start with "{".
 */
class SubscriptionUserInfoTest {

    private val body = "vless://11111111-1111-1111-1111-111111111111@example.com:443#Node"

    private fun userInfoOf(header: String?): Map<String, Long> =
        SubscriptionClient.normalize(
            body,
            if (header == null) emptyMap() else mapOf("subscription-userinfo" to header)
        ).userInfo

    @Test
    fun parsesTheStandardHeader() {
        val info = SubscriptionClient.parseUserInfo(
            "upload=1024; download=2048; total=10240; expire=1767225600"
        )
        assertEquals(1024L, info["upload"])
        assertEquals(2048L, info["download"])
        assertEquals(10240L, info["total"])
        assertEquals(1767225600L, info["expire"])
    }

    @Test
    fun expiryInSecondsIsKept() {
        // 2026-01-01T00:00:00Z. Anything a panel can legitimately mean is below the
        // year 9999 ceiling and must survive untouched.
        assertEquals(1767225600L, SubscriptionClient.parseUserInfo("expire=1767225600")["expire"])
        assertEquals(1767225600L, SubscriptionClient.normalizeExpireSeconds(1767225600L))
    }

    @Test
    fun expiryInMillisecondsIsRescaledToSeconds() {
        // Treated as seconds this is the year 57 000; it used to render as an absurd date.
        assertEquals(1767225600L, SubscriptionClient.parseUserInfo("expire=1767225600000")["expire"])
        assertEquals(1767225600L, SubscriptionClient.normalizeExpireSeconds(1767225600000L))
        // Microseconds too, for the panels that report them.
        assertEquals(1767225600L, SubscriptionClient.normalizeExpireSeconds(1767225600000000L))
    }

    @Test
    fun nonPositiveExpiryMeansNoExpiry() {
        assertEquals(0L, SubscriptionClient.parseUserInfo("expire=0")["expire"])
        assertEquals(0L, SubscriptionClient.parseUserInfo("expire=-1")["expire"])
        assertEquals(0L, SubscriptionClient.normalizeExpireSeconds(Long.MIN_VALUE))
    }

    @Test
    fun missingFieldsStayAbsent() {
        // Absent must be distinguishable from zero: the caller keeps the value it
        // already had for a key the panel did not send, instead of overwriting it.
        val info = SubscriptionClient.parseUserInfo("download=2048; total=10240")
        assertFalse(info.containsKey("upload"))
        assertFalse(info.containsKey("expire"))
        assertEquals(2048L, info["download"])
        assertEquals(10240L, info["total"])
    }

    @Test
    fun explicitZeroTotalIsKept() {
        // total = 0 is how panels spell an unlimited plan, so it has to reach the
        // caller rather than being dropped as "falsy".
        val info = SubscriptionClient.parseUserInfo("upload=1; download=2; total=0; expire=0")
        assertTrue(info.containsKey("total"))
        assertEquals(0L, info["total"])
        assertEquals(0L, info["expire"])
    }

    @Test
    fun negativeCountersAreDropped() {
        val info = SubscriptionClient.parseUserInfo("upload=-5; download=2048; total=-1")
        assertFalse(info.containsKey("upload"))
        assertFalse(info.containsKey("total"))
        assertEquals(2048L, info["download"])
    }

    @Test
    fun garbageAndBlankPartsAreIgnored() {
        val info = SubscriptionClient.parseUserInfo("; download=2048; total=abc; =7; expire")
        assertEquals(mapOf("download" to 2048L), info)
        assertEquals(emptyMap<String, Long>(), SubscriptionClient.parseUserInfo(""))
    }

    @Test
    fun commaSeparatedHeaderIsAccepted() {
        val info = SubscriptionClient.parseUserInfo("upload=1, download=2, total=3")
        assertEquals(3, info.size)
        assertEquals(2L, info["download"])
    }

    @Test
    fun normalizeReadsTheHeader() {
        val info = userInfoOf("upload=1024; download=2048; total=10240; expire=1767225600000")
        assertEquals(1024L, info["upload"])
        assertEquals(2048L, info["download"])
        assertEquals(10240L, info["total"])
        // Rescaled on the way through normalize(), not just in parseUserInfo().
        assertEquals(1767225600L, info["expire"])
    }

    @Test
    fun responseWithoutUserinfoHeaderYieldsNothing() {
        // A refresh that came back through a fallback User-Agent profile carries no
        // usage header at all. It must report "no data", never zeroes, so the caller
        // can keep the figures it already has.
        assertTrue(userInfoOf(null).isEmpty())
        assertTrue(userInfoOf("").isEmpty())
        assertTrue(userInfoOf("   ").isEmpty())
    }

    @Test
    fun normalizeStillReturnsTheLinksBody() {
        val payload = SubscriptionClient.normalize(body, emptyMap())
        assertEquals(body, payload.body)
        assertNull(payload.profileTitle)
    }
}
