package com.lumen.app.onboarding

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import com.lumen.ui.screens.*
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class IranFirstRunTest {
    private val assets = File("src/main/assets")
    private fun replace(from: File, to: File) { check(from.renameTo(to)) }

    @Test fun allIranSetupModesCompleteWithBundledListsAndLocalRuntimeRules() = runBlocking {
        val reports = File("build/reports/iran-first-run").apply { mkdirs() }
        for (mode in SetupRouting.entries) {
            val directory = Files.createTempDirectory(reports.toPath(), "${mode.name}-").toFile()
            val settings = onboardingRouting(SettingsUiState(), SetupRegion.IRAN, mode)
            restoreBundledGeoResources("ir", directory, { File(assets, it).inputStream() }, ::replace)
            assertTrue(onboardingRuleSets(settings).all { (kind, code) ->
                File(directory, "$kind-$code.srs").length() > 64
            })
            // With all required sets restored, setup performs no HTTP request.
            prepareOnboardingGeoResources(settings, directory, "test") { _, _ ->
                error("First run must not need a downloaded replacement")
            }
            val node = LinkParser.parseSingle(
                "vless://00000000-0000-0000-0000-000000000001@vpn.example:443?security=tls#IR"
            )
            val config = SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(
                tunMode = false, geoResourceSource = settings.geoResourceSource,
                directDomains = settings.directDomains.lines().filter(String::isNotBlank),
                routingFinalOutbound = settings.routingFinalOutbound,
                dnsMode = settings.dnsMode, dnsDirectType = settings.dnsDirectType,
                dnsProxyType = settings.dnsProxyType,
                dnsDirectServers = settings.dnsDirectServers.lines(),
                dnsProxyServers = settings.dnsProxyServers.lines(),
                dnsParallelQuery = settings.dnsParallelQuery,
                systemDnsServers = listOf("172.19.0.2"),
                geoRuleSetDir = directory.absolutePath, requireLocalRuleSets = true
            ))
            val root = JSONObject(config)
            assertEquals("proxy", root.getJSONObject("route").getString("final"))
            val rules = root.getJSONObject("route").optJSONArray("rule_set")
            if (rules != null) for (i in 0 until rules.length()) {
                assertEquals("local", rules.getJSONObject(i).getString("type"))
                assertTrue(File(rules.getJSONObject(i).getString("path")).isFile)
            }
            File(directory, "config.json").writeText(config)
        }
    }

    @Test fun bundledRestorePreservesUpdatesAndRepairsMissingIranLists() {
        val directory = Files.createTempDirectory("lumen-iran-restore").toFile()
        try {
            val updated = File(directory, "geosite-ir.srs").apply { writeText("user updated list") }
            restoreBundledGeoResources("ir", directory, { File(assets, it).inputStream() }, ::replace)
            assertEquals("user updated list", updated.readText())
            assertTrue(File(directory, "geoip-ir.srs").length() > 64)
            assertFalse(File(directory, "geoip-ru.srs").exists())
            assertEquals(3, directory.listFiles()!!.size)
        } finally {
            directory.listFiles()?.forEach(File::delete)
            directory.delete()
        }
    }
}
