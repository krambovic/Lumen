package com.lumen.app.subscription

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The subscription headers the panel sends, and the `base64:` encoding half of them
 * arrive in. The card used to render the raw `base64:...` string because only
 * `profile-title` was ever decoded.
 */
class SubscriptionHeaderMetadataTest {

    private val body = "vless://uuid@node.example:443#Node%20A"

    private fun metadata(vararg headers: Pair<String, String>): SubscriptionMetadata =
        SubscriptionClient.normalize(body, mapOf(*headers)).metadata

    // ---------- the Channel/Bot and Support buttons ----------

    @Test
    fun telegramUrlSurvivesTheMetadataWhitelist() {
        // It reached buildMetadata's `pick` but never got into the source map, because
        // the whitelist that fills that map had no entry for it — so the Channel / Bot
        // button was always missing while desktop showed it.
        assertEquals("https://t.me/maviks", metadata("telegram-url" to "https://t.me/maviks").telegramUrl)
        assertEquals("https://t.me/maviks", metadata("telegram" to "https://t.me/maviks").telegramUrl)
    }

    @Test
    fun telegramUrlIsAlsoReadFromABodyComment() {
        val withComment = "#telegram-url https://t.me/maviks\n$body"
        val payload = SubscriptionClient.normalize(withComment, emptyMap())
        assertEquals("https://t.me/maviks", payload.metadata.telegramUrl)
        // The directive must not survive as a link line.
        assertFalse(payload.body.contains("telegram-url"))
    }

    @Test
    fun linksWithoutASchemeAreAcceptedTheWayDesktopAcceptsThem() {
        // Desktop stores these verbatim, so panels send them scheme-less; requiring
        // http(s) dropped the button on Android only.
        assertEquals("https://t.me/maviks", metadata("support-url" to "t.me/maviks").supportUrl)
        assertEquals("https://maviks.app/support", metadata("support-url" to "maviks.app/support").supportUrl)
        assertEquals("https://t.me/maviks", metadata("support-url" to "@maviks").supportUrl)
        assertEquals("tg://resolve?domain=maviks", metadata("support-url" to "tg://resolve?domain=maviks").supportUrl)
    }

    @Test
    fun valuesThatAreNotLinkShapedAreStillRejected() {
        assertNull(metadata("support-url" to "call us on monday").supportUrl)
        assertNull(metadata("support-url" to "support").supportUrl)
        assertNull(metadata("support-url" to "ftp://example.com/x").supportUrl)
        assertNull(metadata("support-url" to ".example.com").supportUrl)
    }

    // ---------- decodeHeader ----------

    @Test
    fun realAnnouncementFromTheBugReportDecodes() {
        val header = "base64:0KPQv9GA0LDQstC70Y/QuSBNYXZpa3NWUE4g0L3QsCDRgdCw0LnRgtC1Cmh0dHBz" +
            "Oi8vbWF2aWtzLmFwcArQn9C+0LvRjNC30L7QstCw0YLQtdC70Yw6IGYzZGE4MTNkLTc2NTItNDBkYi04OTQ5" +
            "LTY5ZDkwZTBiMDE5OA=="
        val expected = listOf(
            // "Manage MaviksVPN on the site" / "User: <uuid>"
            "\u0423\u043f\u0440\u0430\u0432\u043b\u044f\u0439 MaviksVPN \u043d\u0430 \u0441\u0430\u0439\u0442\u0435",
            "https://maviks.app",
            "\u041f\u043e\u043b\u044c\u0437\u043e\u0432\u0430\u0442\u0435\u043b\u044c: " +
                "f3da813d-7652-40db-8949-69d90e0b0198"
        )
        assertEquals(expected, SubscriptionClient.decodeHeader(header)?.lines())
        // ...and it reaches the card through the payload, which is where it was lost.
        assertEquals(expected.joinToString("\n"), metadata("Announce" to header).announce)
    }

    @Test
    fun paddedAndUnpaddedBase64BothDecode() {
        assertEquals("Maviks VPN ~ Pro", SubscriptionClient.decodeHeader("base64:TWF2aWtzIFZQTiB+IFBybw=="))
        assertEquals("Maviks VPN ~ Pro", SubscriptionClient.decodeHeader("base64:TWF2aWtzIFZQTiB+IFBybw"))
    }

    @Test
    fun urlSafeAlphabetDecodes() {
        // "-" and "_" stand in for "+" and "/" in the URL-safe alphabet.
        assertEquals("Maviks VPN ~ Pro", SubscriptionClient.decodeHeader("base64:TWF2aWtzIFZQTiB-IFBybw=="))
        assertEquals("Renew?~", SubscriptionClient.decodeHeader("base64:UmVuZXc_fg=="))
    }

    @Test
    fun prefixIsCaseInsensitive() {
        assertEquals("MaviksVPN", SubscriptionClient.decodeHeader("BASE64:TWF2aWtzVlBO"))
        assertEquals("MaviksVPN", SubscriptionClient.decodeHeader("Base64:TWF2aWtzVlBO"))
        assertEquals("MaviksVPN", SubscriptionClient.decodeHeader("  base64:TWF2aWtzVlBO  "))
    }

    @Test
    fun plainTextIsLeftAlone() {
        // Valid base64 on its own, but with no prefix it is a name, not a payload.
        assertEquals("TWF2aWtzVlBO", SubscriptionClient.decodeHeader("TWF2aWtzVlBO"))
        assertEquals("MaviksVPN", SubscriptionClient.decodeHeader("MaviksVPN"))
        assertNull(SubscriptionClient.decodeHeader("   "))
        assertNull(SubscriptionClient.decodeHeader(null))
    }

    @Test
    fun malformedBase64FallsBackToTheRawString() {
        assertEquals("base64:!!!not base64!!!", SubscriptionClient.decodeHeader("base64:!!!not base64!!!"))
        // A single leftover character can never be a base64 quantum.
        assertEquals("base64:A", SubscriptionClient.decodeHeader("base64:A"))
        assertEquals("base64:", SubscriptionClient.decodeHeader("base64:"))
    }

    // ---------- header names ----------

    @Test
    fun headerNamesAreCaseInsensitive() {
        val payload = SubscriptionClient.normalize(
            body,
            mapOf("Profile-Title" to "base64:TWF2aWtzVlBO", "ANNOUNCE" to "Hello")
        )
        assertEquals("MaviksVPN", payload.profileTitle)
        assertEquals("Hello", payload.metadata.announce)
    }

    @Test
    fun subscriptionNameIsATitleFallback() {
        assertEquals(
            "MaviksVPN",
            SubscriptionClient.normalize(body, mapOf("subscription-name" to "base64:TWF2aWtzVlBO")).profileTitle
        )
        // profile-title still wins when both arrive.
        assertEquals(
            "Primary",
            SubscriptionClient.normalize(
                body,
                mapOf("profile-title" to "Primary", "subscription-name" to "Secondary")
            ).profileTitle
        )
    }

    @Test
    fun contentDispositionIsTheLastResortName() {
        assertEquals(
            "MaviksVPN",
            SubscriptionClient.normalize(
                body,
                mapOf("content-disposition" to "attachment; filename=\"MaviksVPN.txt\"")
            ).profileTitle
        )
        assertEquals(
            "MaviksVPN",
            SubscriptionClient.normalize(
                body,
                mapOf("content-disposition" to "attachment; filename=MaviksVPN.yaml")
            ).profileTitle
        )
        // Generic filenames are not names.
        assertNull(
            SubscriptionClient.normalize(
                body,
                mapOf("content-disposition" to "attachment; filename=subscription.txt")
            ).profileTitle
        )
    }

    @Test
    fun documentedAlternativesAreAccepted() {
        assertEquals("https://maviks.app", metadata("homepage" to "https://maviks.app").websiteUrl)
        assertEquals("Hello", metadata("announcement" to "Hello").announce)
        assertEquals("https://t.me/maviks", metadata("announcement-url" to "https://t.me/maviks").announceUrl)
        assertEquals("https://t.me/support", metadata("support" to "https://t.me/support").supportUrl)
        // profile-web-page-url wins over homepage.
        assertEquals(
            "https://a.example",
            metadata("profile-web-page-url" to "https://a.example", "homepage" to "https://b.example").websiteUrl
        )
    }

    // ---------- typed fields ----------

    @Test
    fun everyBase64CapableFieldIsDecoded() {
        val meta = metadata(
            "profile-description" to "base64:TWF2aWtzVlBO",
            "announce" to "base64:TWF2aWtzVlBO",
            "banner-text" to "base64:0J3QsNGIINGB0LDQudGC",
            "banner-button-text" to "base64:TWF2aWtzVlBO"
        )
        assertEquals("MaviksVPN", meta.description)
        assertEquals("MaviksVPN", meta.announce)
        // "Our site"
        assertEquals("\u041d\u0430\u0448 \u0441\u0430\u0439\u0442", meta.bannerText)
        assertEquals("MaviksVPN", meta.bannerButtonText)
    }

    @Test
    fun linksEmailFlagsAndColoursAreTyped() {
        val meta = metadata(
            "support-url" to "https://t.me/maviks_bot",
            "support-email" to "mailto:help@maviks.app",
            "profile-web-page-url" to "https://maviks.app",
            "premium-url" to "https://maviks.app/premium",
            "banner-button-url" to "https://maviks.app/renew",
            "banner-bg-color" to "#1a2b3c",
            "banner-button-color" to "4a5b6c",
            "hide-url" to "1",
            "sort-order" to "PING"
        )
        assertEquals("https://t.me/maviks_bot", meta.supportUrl)
        assertEquals("help@maviks.app", meta.supportEmail)
        assertEquals("https://maviks.app", meta.websiteUrl)
        assertEquals("https://maviks.app/premium", meta.premiumUrl)
        assertEquals("https://maviks.app/renew", meta.bannerButtonUrl)
        assertEquals("#1A2B3C", meta.bannerBgColor)
        assertEquals("#4A5B6C", meta.bannerButtonColor)
        assertEquals(true, meta.hideUrl)
        assertEquals("ping", meta.sortOrder)
    }

    @Test
    fun junkValuesAreDroppedRatherThanShown() {
        val meta = metadata(
            "support-url" to "javascript:alert(1)",
            "support-email" to "not-an-email",
            "banner-bg-color" to "rebeccapurple",
            "hide-url" to "maybe",
            "sort-order" to "random"
        )
        assertNull(meta.supportUrl)
        assertNull(meta.supportEmail)
        assertNull(meta.bannerBgColor)
        assertNull(meta.hideUrl)
        assertNull(meta.sortOrder)
    }

    @Test
    fun hideUrlAcceptsBothSpellingsOfFalse() {
        assertEquals(false, metadata("hide-url" to "0").hideUrl)
        assertEquals(false, metadata("hide-url" to "false").hideUrl)
        assertEquals(true, metadata("hide-url" to "true").hideUrl)
    }

    @Test
    fun absentHeadersStayNullSoTheCallerCanKeepItsValues() {
        val meta = metadata("profile-title" to "MaviksVPN")
        assertNull(meta.announce)
        assertNull(meta.supportUrl)
        assertNull(meta.bannerText)
        assertNull(meta.hideUrl)
    }

    // ---------- body comments ----------

    @Test
    fun bodyCommentsAreReadAndStrippedFromTheLinks() {
        val payload = SubscriptionClient.normalize(
            "# announce: base64:TWF2aWtzVlBO\n# support-url: https://t.me/maviks\n$body",
            emptyMap()
        )
        assertEquals("MaviksVPN", payload.metadata.announce)
        assertEquals("https://t.me/maviks", payload.metadata.supportUrl)
        assertEquals(body, payload.body)
    }

    @Test
    fun httpHeadersBeatBodyComments() {
        val payload = SubscriptionClient.normalize(
            "# announce: From the body\n# profile-title: Body title\n$body",
            mapOf("announce" to "From the header", "profile-title" to "Header title")
        )
        assertEquals("From the header", payload.metadata.announce)
        assertEquals("Header title", payload.profileTitle)
    }

    @Test
    fun updateIntervalComesFromEitherSource() {
        assertEquals(
            12,
            SubscriptionClient.normalize(body, mapOf("profile-update-interval" to "12")).updateIntervalHours
        )
        assertEquals(
            6,
            SubscriptionClient.normalize("# profile-update-interval: 6\n$body", emptyMap()).updateIntervalHours
        )
    }

    @Test
    fun premiumMapStillCarriesADecodedAnnounce() {
        // The dashboard's premium feature count and the legacy map keep working, but the
        // value in it is no longer a raw "base64:..." string.
        val payload = SubscriptionClient.normalize(body, mapOf("announce" to "base64:TWF2aWtzVlBO"))
        assertEquals("MaviksVPN", payload.premiumFeatures["announce"])
        assertTrue(payload.premiumFeatures.isNotEmpty())
        assertFalse(payload.premiumFeatures.getValue("announce").startsWith("base64:"))
    }
}
