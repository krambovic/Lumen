package com.lumen.ui

import com.lumen.ui.components.CountryFlagHelper
import com.lumen.ui.components.StripeStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CountryFlagHelperTest {

    @Test
    fun testGetFlagEmoji() {
        assertEquals("🇷🇺", CountryFlagHelper.getFlagEmoji("RU"))
        assertEquals("🇺🇸", CountryFlagHelper.getFlagEmoji("US"))
        assertEquals("🇩🇪", CountryFlagHelper.getFlagEmoji("de"))
        assertEquals("🇫🇷", CountryFlagHelper.getFlagEmoji("FR"))
        assertEquals("", CountryFlagHelper.getFlagEmoji("INVALID"))
        assertEquals("", CountryFlagHelper.getFlagEmoji(""))
        assertEquals("", CountryFlagHelper.getFlagEmoji(null))
    }

    @Test
    fun testDetectEmoji() {
        assertEquals("DE", CountryFlagHelper.detectEmoji("Node 🇩🇪 Germany"))
        assertEquals("RU", CountryFlagHelper.detectEmoji("Server 🇷🇺 Moscow"))
        assertEquals("US", CountryFlagHelper.detectEmoji("🇺🇸 USA Fast Node"))
        assertEquals("", CountryFlagHelper.detectEmoji("No emoji node"))
    }

    @Test
    fun testDetectName() {
        assertEquals("US", CountryFlagHelper.detectName("United States Premium"))
        assertEquals("DE", CountryFlagHelper.detectName("Германия Франкфурт"))
        assertEquals("RU", CountryFlagHelper.detectName("Moscow Server 1"))
        assertEquals("RU", CountryFlagHelper.detectName("Saint Petersburg Node"))
        assertEquals("NL", CountryFlagHelper.detectName("Amsterdam Highspeed"))
        assertEquals("GB", CountryFlagHelper.detectName("London Relay"))
        assertEquals("FR", CountryFlagHelper.detectName("Paris Express"))
        assertEquals("JP", CountryFlagHelper.detectName("Tokyo Ultra"))
    }

    @Test
    fun testDetectCode() {
        assertEquals("RU", CountryFlagHelper.detectCode("RU - Fast Server"))
        assertEquals("DE", CountryFlagHelper.detectCode("DE_01_Node"))
        assertEquals("US", CountryFlagHelper.detectCode("US#5"))
        assertEquals("NL", CountryFlagHelper.detectCode("NL 100"))
        assertEquals("", CountryFlagHelper.detectCode("XYZ 123"))
    }

    @Test
    fun testDetectServer() {
        assertEquals("DE", CountryFlagHelper.detectServer("de1.server.com"))
        assertEquals("US", CountryFlagHelper.detectServer("us.vpn-node.net"))
        assertEquals("RU", CountryFlagHelper.detectServer("vpn.company.ru"))
        assertEquals("GB", CountryFlagHelper.detectServer("relay.server.uk"))
        assertEquals("", CountryFlagHelper.detectServer(""))
    }

    @Test
    fun testDetectCountryFullChain() {
        assertEquals("DE", CountryFlagHelper.detectCountry("🇩🇪 Node", "other.com"))
        assertEquals("US", CountryFlagHelper.detectCountry("United States Server", ""))
        assertEquals("RU", CountryFlagHelper.detectCountry("RU - Fast", ""))
        assertEquals("FR", CountryFlagHelper.detectCountry("Unnamed Node", "fr1.vpn.org"))
        assertEquals("", CountryFlagHelper.detectCountry("Generic Node", "192.168.1.1"))
        assertEquals("US", CountryFlagHelper.detectCountry("WARP", "8.47.69.7"))
    }

    @Test
    fun testStripesMapContainsCommonCountries() {
        assertTrue(CountryFlagHelper.STRIPES.containsKey("RU"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("US"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("DE"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("FR"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("JP"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("UA"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("GB"))
        assertTrue(CountryFlagHelper.STRIPES.containsKey("EU"))
        assertEquals(StripeStyle.Czech, CountryFlagHelper.STRIPES["CZ"]?.style)
    }

    @Test
    fun testEuropeAndCountryPrefixCleanup() {
        assertEquals("EU", CountryFlagHelper.detectCountry("Europe", ""))
        assertEquals("RU", CountryFlagHelper.detectName("Сервер Москва"))
        assertEquals("", CountryFlagHelper.detectName("Moscowville"))
        assertEquals("1", CountryFlagHelper.serverDisplayNameWithoutCountryPrefix("NO-1", "NO"))
        assertEquals(
            "Oslo",
            CountryFlagHelper.serverDisplayNameWithoutCountryPrefix("NO | Oslo", "NO")
        )
    }
}
