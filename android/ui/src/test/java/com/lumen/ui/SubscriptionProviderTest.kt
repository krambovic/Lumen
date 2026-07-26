package com.lumen.ui

import com.lumen.ui.components.hasProviderBanner
import com.lumen.ui.components.hasProviderCard
import com.lumen.ui.components.parseProviderColor
import com.lumen.ui.components.providerColorPrefersDarkText
import com.lumen.ui.components.providerLinks
import com.lumen.ui.components.subscriptionAnnouncement
import com.lumen.ui.components.subscriptionUrlVisible
import com.lumen.ui.screens.SubscriptionUiModel
import com.lumen.ui.screens.stringsForLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The provider block of a subscription card is driven entirely by what the panel sent:
 * these guard that a plain subscription renders nothing extra, that a link only becomes
 * a button when it exists, and that a broken colour never paints an unreadable banner.
 */
class SubscriptionProviderTest {

    private val en = stringsForLanguage("en")

    private fun sub(
        announce: String? = null,
        description: String? = null,
        supportUrl: String? = null,
        supportEmail: String? = null,
        websiteUrl: String? = null,
        premiumUrl: String? = null,
        bannerText: String? = null,
        bannerButtonText: String? = null,
        bannerButtonUrl: String? = null,
        hideUrl: Boolean = false,
        url: String = "https://panel.example/sub/1"
    ) = SubscriptionUiModel(
        id = "sub1",
        name = "MaviksVPN",
        url = url,
        announce = announce,
        description = description,
        supportUrl = supportUrl,
        supportEmail = supportEmail,
        websiteUrl = websiteUrl,
        premiumUrl = premiumUrl,
        bannerText = bannerText,
        bannerButtonText = bannerButtonText,
        bannerButtonUrl = bannerButtonUrl,
        hideUrl = hideUrl
    )

    @Test
    fun aProviderThatSendsNothingProducesNoButtons() {
        val plain = sub()
        assertTrue(providerLinks(plain, en).isEmpty())
        assertNull(subscriptionAnnouncement(plain))
        assertFalse(hasProviderBanner(plain))
    }

    @Test
    fun onlyTheLinksThePanelSentBecomeButtons() {
        val links = providerLinks(
            sub(supportUrl = "https://t.me/maviks_bot", websiteUrl = "  "),
            en
        )
        assertEquals(listOf("support"), links.map { it.key })
        assertEquals(en.providerSupport, links.first().label)
        assertEquals("https://t.me/maviks_bot", links.first().target)
    }

    @Test
    fun linksKeepTheReferenceOrderAndEmailBecomesMailto() {
        val links = providerLinks(
            sub(
                supportUrl = "https://t.me/maviks_bot",
                supportEmail = "help@maviks.app",
                websiteUrl = "https://maviks.app",
                premiumUrl = "https://maviks.app/premium"
            ),
            en
        )
        assertEquals(listOf("support", "email", "website", "premium"), links.map { it.key })
        assertEquals("mailto:help@maviks.app", links[1].target)
    }

    @Test
    fun theAnnouncementSurvivesEmptyOneWordAndFullLengthText() {
        assertNull(subscriptionAnnouncement(sub(announce = "")))
        assertNull(subscriptionAnnouncement(sub(announce = "   ")))
        assertEquals("Hello", subscriptionAnnouncement(sub(announce = "Hello")))
        // 200 characters with the provider's own line breaks kept intact.
        val long = ("Line one\n" + "x".repeat(191))
        assertEquals(200, long.length)
        val rendered = subscriptionAnnouncement(sub(announce = long))
        assertEquals(long, rendered)
        assertTrue(rendered!!.contains("\n"))
    }

    @Test
    fun hideUrlKeepsTheSubscriptionUrlOutOfTheCard() {
        assertTrue(subscriptionUrlVisible(sub()))
        assertFalse(subscriptionUrlVisible(sub(hideUrl = true)))
        assertFalse(subscriptionUrlVisible(sub(url = "")))
    }

    @Test
    fun theBannerNeedsTextOrACompleteButton() {
        assertTrue(hasProviderBanner(sub(bannerText = "Summer sale")))
        assertFalse(hasProviderBanner(sub(bannerButtonText = "Buy")))
        assertFalse(hasProviderBanner(sub(bannerButtonUrl = "https://maviks.app/buy")))
        assertTrue(
            hasProviderBanner(sub(bannerButtonText = "Buy", bannerButtonUrl = "https://maviks.app/buy"))
        )
    }

    @Test
    fun theProviderCardIsSkippedUntilThePanelSendsSomethingForIt() {
        // Both screens decide on this before composing the card, and the dashboard also
        // hands the tile's rounded bottom corner back to the traffic bar when it is
        // false, so it has to agree with what the card itself would draw.
        assertFalse(hasProviderCard(sub()))
        assertFalse(hasProviderCard(sub(announce = "   ")))
        assertTrue(hasProviderCard(sub(announce = "Maintenance tonight")))
        assertTrue(hasProviderCard(sub(description = "Family plan")))
        assertTrue(hasProviderCard(sub(supportUrl = "https://t.me/maviks_bot")))
        assertTrue(hasProviderCard(sub(supportEmail = "help@maviks.app")))
        assertTrue(hasProviderCard(sub(websiteUrl = "https://maviks.app")))
        assertTrue(hasProviderCard(sub(premiumUrl = "https://maviks.app/premium")))
        assertTrue(hasProviderCard(sub(bannerText = "Summer sale")))
        // An incomplete banner button is not content on its own.
        assertFalse(hasProviderCard(sub(bannerButtonText = "Buy")))
    }

    @Test
    fun onlyRealHexColoursAreUsedAndTheRestFallBackToTheTheme() {
        assertEquals(0xFF1E88E5.toInt(), parseProviderColor("#1E88E5"))
        assertEquals(0xFF1E88E5.toInt(), parseProviderColor("1e88e5"))
        assertNull(parseProviderColor(null))
        assertNull(parseProviderColor(""))
        assertNull(parseProviderColor("#FFF"))
        assertNull(parseProviderColor("#GGGGGG"))
        assertNull(parseProviderColor("-12345"))
        assertNull(parseProviderColor("#1E88E5FF"))
    }

    @Test
    fun bannerLabelsFlipToDarkTextOnLightBackgrounds() {
        assertTrue(providerColorPrefersDarkText(0xFFFFFFFF.toInt()))
        assertTrue(providerColorPrefersDarkText(0xFFFFEB3B.toInt()))
        assertFalse(providerColorPrefersDarkText(0xFF000000.toInt()))
        assertFalse(providerColorPrefersDarkText(0xFF1E88E5.toInt()))
    }

    @Test
    fun everyLanguageLabelsTheProviderButtons() {
        listOf("en", "ru", "zh", "fa").forEach { code ->
            val s = stringsForLanguage(code)
            listOf(
                s.providerAnnouncement,
                s.providerSupport,
                s.providerEmail,
                s.providerWebsite,
                s.providerPremium
            ).forEach { label ->
                assertTrue("$code leaves a provider label empty", label.isNotBlank())
            }
        }
    }
}
