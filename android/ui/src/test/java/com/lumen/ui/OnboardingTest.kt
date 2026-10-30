package com.lumen.ui

import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class OnboardingTest {
    @Test fun iranSetupPreservesCustomPlaintextDnsTransports() {
        val custom = SettingsUiState(dnsDirectServers = "192.168.1.1", dnsDirectType = "udp",
            dnsProxyServers = "9.9.9.10", dnsProxyType = "tcp")
        val configured = onboardingNetworkDefaults(custom, SetupRegion.IRAN)
        assertEquals(custom.dnsDirectType, configured.dnsDirectType)
        assertEquals(custom.dnsProxyType, configured.dnsProxyType)
        assertEquals(custom.dnsDirectServers, configured.dnsDirectServers)
    }

    @Test fun iranSetupUsesEncryptedDnsAndResilientProxyChecks() {
        val iran = onboardingRouting(SettingsUiState(), SetupRegion.IRAN, SetupRouting.EXCEPT_LOCAL)
        assertEquals("https", iran.dnsDirectType)
        assertEquals("https", iran.dnsProxyType)
        assertTrue(iran.dnsParallelQuery)
        assertEquals("http", iran.pingType)
        assertEquals(10_000, iran.pingTimeoutMs)
        assertEquals(2, iran.pingAttempts)
        assertEquals(4, iran.pingConcurrency)
        assertFalse(iran.pingAutoDeleteUnreachable)
        assertEquals("https://www.google.com/generate_204", iran.pingUrl)
        assertEquals(iran, onboardingRouting(iran, SetupRegion.IRAN, SetupRouting.EXCEPT_LOCAL))
    }

    @Test fun regionalSetupPreservesCustomDnsAndProbeDestinations() {
        val custom = SettingsUiState(dnsMode = "json", dnsCustomJson = "{custom}",
            dnsDirectServers = "10.0.0.53", dnsDirectType = "tcp",
            pingUrl = "https://example.org/check", pingTimeoutMs = 9_000)
        val iran = onboardingRouting(custom, SetupRegion.IRAN, SetupRouting.GLOBAL)
        assertEquals(custom.dnsMode, iran.dnsMode)
        assertEquals(custom.dnsCustomJson, iran.dnsCustomJson)
        assertEquals(custom.dnsDirectServers, iran.dnsDirectServers)
        assertEquals(custom.dnsDirectType, iran.dnsDirectType)
        assertEquals(custom.pingUrl, iran.pingUrl)
        assertEquals(custom.pingTimeoutMs.coerceAtLeast(10_000), iran.pingTimeoutMs)
        val russia = onboardingRouting(SettingsUiState(), SetupRegion.RUSSIA, SetupRouting.GLOBAL)
        assertEquals("udp", russia.dnsDirectType)
        assertEquals(2_000, russia.pingTimeoutMs)
    }

    @Test fun iranSetupDoesNotLeaveQueriesOnTheCarrierDnsPath() {
        val iran = onboardingRouting(SettingsUiState(dnsMode = "android", dnsHijackEnabled = false),
            SetupRegion.IRAN, SetupRouting.GLOBAL)
        assertEquals("automatic", iran.dnsMode)
        assertTrue(iran.dnsHijackEnabled)
    }

    @Test fun existingIranInstallMigratesDefaultsWithoutOverwritingCustomSettings() {
        val migrated = migrateIranNetworkDefaults(SettingsUiState())
        assertEquals("https", migrated.dnsDirectType)
        assertEquals(10_000, migrated.pingTimeoutMs)
        assertTrue(migrated.dnsParallelQuery)
        val custom = SettingsUiState(dnsMode = "json", dnsCustomJson = "{custom}",
            dnsDirectServers = "192.168.1.1", dnsDirectType = "tcp", dnsParallelQuery = false,
            pingType = "tcping", pingConcurrency = 3, pingTimeoutMs = 3_000,
            pingUrl = "https://example.org/check")
        val retained = migrateIranNetworkDefaults(custom)
        assertEquals(custom.dnsMode, retained.dnsMode)
        assertEquals(custom.dnsCustomJson, retained.dnsCustomJson)
        assertEquals(custom.dnsDirectType, retained.dnsDirectType)
        assertFalse(retained.dnsParallelQuery)
        assertEquals(custom.pingType, retained.pingType)
        assertEquals(custom.pingConcurrency, retained.pingConcurrency)
        assertEquals(custom.pingTimeoutMs, retained.pingTimeoutMs)
        assertEquals(custom.pingUrl, retained.pingUrl)
    }

    @Test fun freshSetupAndUpdatesHaveDifferentFlows() {
        assertEquals(listOf(WelcomeStep.WELCOME, WelcomeStep.LOOK, WelcomeStep.ROUTES,
            WelcomeStep.CONFIGS, WelcomeStep.SYSTEM, WelcomeStep.DONE), onboardingSteps(true, ""))
        assertEquals(listOf(WelcomeStep.NEWS), onboardingSteps(false, "1.4.0"))
        assertEquals(WelcomeStep.NEWS, onboardingSteps(true, "1.4.0").first())
    }

    @Test fun completionIsRequiredOnlyForAChangedVersionOrSetupRevision() {
        assertTrue(onboardingRequired(0, "", 0, "1.5.0", 503))
        // This update must also welcome users who completed an earlier 1.5.0 build.
        assertTrue(onboardingRequired(1, "1.5.0", 503, "1.5.0", 503))
        assertFalse(onboardingRequired(ONBOARDING_REVISION, "1.5.0", 503, "1.5.0", 503))
        assertTrue(onboardingRequired(ONBOARDING_REVISION, "1.5.0", 503, "1.5.1", 513))
        assertTrue(onboardingRequired(ONBOARDING_REVISION, "1.5.0", 502, "1.5.0", 503))
    }

    @Test fun switchingPresetsPreservesCustomRulesAndReplacesTheManagedRegion() {
        val state = SettingsUiState(directDomains = "proxy:full:work.example\ndirect:geosite:category-ru\ndirect:geoip:ru")
        val china = onboardingRouting(state, SetupRegion.CHINA, SetupRouting.EXCEPT_LOCAL)
        assertEquals("proxy", china.routingFinalOutbound)
        assertTrue(china.directDomains.contains("proxy:full:work.example"))
        assertTrue(china.directDomains.contains("direct:geosite:cn"))
        assertFalse(china.directDomains.contains("category-ru"))
        val global = onboardingRouting(china, SetupRegion.CHINA, SetupRouting.GLOBAL)
        assertEquals("proxy:full:work.example", global.directDomains)
    }

    @Test fun blockedOnlyUsesRealDirectFallbackRatherThanCatchAllRules() {
        val blocked = onboardingRouting(SettingsUiState(directDomains = ""), SetupRegion.RUSSIA, SetupRouting.BLOCKED)
        assertEquals("direct", blocked.routingFinalOutbound)
        assertTrue(blocked.directDomains.contains("proxy:geosite:ru-blocked"))
        assertFalse(blocked.directDomains.contains("0.0.0.0/0"))
        assertTrue(onboardingRuleSets(blocked).contains("geosite" to "ru-blocked"))
        val global = onboardingRouting(blocked, SetupRegion.RUSSIA, SetupRouting.GLOBAL)
        assertTrue(onboardingRuleSets(global).isEmpty())
        val iran = onboardingRouting(blocked, SetupRegion.IRAN, SetupRouting.EXCEPT_LOCAL)
        assertEquals("proxy", iran.routingFinalOutbound)
        assertFalse(iran.directDomains.contains("ru-blocked"))
        assertEquals(SetupRouting.EXCEPT_LOCAL, initialSetupRouting(iran))
    }
}
