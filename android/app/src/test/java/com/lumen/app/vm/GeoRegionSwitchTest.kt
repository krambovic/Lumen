package com.lumen.app.vm

import com.lumen.ui.screens.SettingsUiState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GeoRegionSwitchTest {
    @Test
    fun newRegionDoesNotRedownloadPreviousRegionalRules() {
        val sets = geoRuleSetsForRegion(
            code = "cn",
            directDomains = """
                direct:geosite:category-ru
                direct:geoip:ru
                block:geosite:youtube
            """.trimIndent(),
            directIpCidrs = "proxy:geoip:ir"
        )

        assertEquals("geosite" to "cn", sets[0])
        assertEquals("geoip" to "cn", sets[1])
        assertTrue(("geosite" to "category-ads-all") in sets)
        assertTrue(("geosite" to "youtube") in sets)
        assertFalse(("geosite" to "category-ru") in sets)
        assertFalse(("geoip" to "ru") in sets)
        assertFalse(("geoip" to "ir") in sets)
    }

    @Test
    fun automaticRoutingRulesAreReplacedAsOneRegion() {
        val switched = switchAutomaticGeoRegion(
            SettingsUiState(
                directDomains = """
                    direct:geosite:category-ru
                    proxy:geoip:ru
                    block:geosite:youtube
                """.trimIndent(),
                directIpCidrs = """
                    direct:geoip:ir
                    direct:10.0.0.0/8
                """.trimIndent()
            ),
            code = "cn"
        )

        assertEquals(
            listOf(
                "block:geosite:youtube",
                "direct:geosite:cn",
                "direct:geoip:cn"
            ),
            switched.directDomains.lines()
        )
        assertEquals("direct:10.0.0.0/8", switched.directIpCidrs)
    }

    @Test
    fun sourceMapsToExactlyOneSupportedRegion() {
        assertEquals("cn", geoRegionCode("https://github.com/Loyalsoldier/v2ray-rules-dat/"))
        assertEquals("ir", geoRegionCode("https://github.com/Chocolate4U/Iran-sing-box-rules/"))
        assertEquals("ru", geoRegionCode("https://github.com/runetfreedom/russia-v2ray-rules-dat/"))
    }
}
