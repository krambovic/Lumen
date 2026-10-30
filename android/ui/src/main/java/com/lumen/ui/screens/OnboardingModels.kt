package com.lumen.ui.screens

import androidx.compose.runtime.Immutable

const val ONBOARDING_REVISION = 2

fun onboardingRequired(revision: Int, lastVersion: String, lastCode: Int, version: String, code: Int): Boolean =
    revision < ONBOARDING_REVISION || lastVersion != version || lastCode != code

enum class WelcomeStep { NEWS, WELCOME, LOOK, ROUTES, CONFIGS, SYSTEM, DONE }

@Immutable
data class OnboardingUiState(
    val visible: Boolean = false,
    val setupRequired: Boolean = true,
    val previousVersion: String = "",
    val warpBusy: Boolean = false,
    val warpCount: Int = 0,
    val warpIncomplete: Boolean = false,
    val warpError: Boolean = false,
    val preparing: Boolean = false,
    val preparationError: Boolean = false
)

/** Only explicitly selected rules need resources; Global needs no regional download. */
fun onboardingRuleSets(settings: SettingsUiState): List<Pair<String, String>> =
    (settings.directDomains + "\n" + settings.directIpCidrs).lineSequence().map(String::trim)
        .map { it.replace(Regex("^(direct|proxy|block|reject):", RegexOption.IGNORE_CASE), "").lowercase() }
        .mapNotNull { Regex("^(geosite|geoip):([a-z0-9._-]+)$").matchEntire(it)?.destructured?.let { (kind, code) -> kind to code } }
        .filterNot { it == "geoip" to "private" }.distinct().toList()

fun onboardingSteps(setupRequired: Boolean, previousVersion: String): List<WelcomeStep> = buildList {
    add(if (previousVersion.isNotBlank()) WelcomeStep.NEWS else WelcomeStep.WELCOME)
    if (setupRequired) addAll(listOf(WelcomeStep.LOOK, WelcomeStep.ROUTES, WelcomeStep.CONFIGS, WelcomeStep.SYSTEM, WelcomeStep.DONE))
}

enum class SetupRegion(val code: String, val source: String) {
    RUSSIA("ru", "https://github.com/runetfreedom/russia-v2ray-rules-dat/"),
    CHINA("cn", "https://github.com/Loyalsoldier/v2ray-rules-dat/"),
    IRAN("ir", "https://github.com/Chocolate4U/Iran-sing-box-rules/")
}

enum class SetupRouting { GLOBAL, BLOCKED, EXCEPT_LOCAL }

fun initialSetupRouting(settings: SettingsUiState): SetupRouting = when {
    settings.routingFinalOutbound == "direct" -> SetupRouting.BLOCKED
    settings.directDomains.lineSequence().any { line ->
        line.trim().replace(Regex("^direct:", RegexOption.IGNORE_CASE), "").lowercase() in setOf(
            "geosite:category-ru", "geoip:ru", "geosite:cn", "geoip:cn", "geosite:ir", "geoip:ir")
    } -> SetupRouting.EXCEPT_LOCAL
    else -> SetupRouting.GLOBAL
}

private val welcomeManagedRules = setOf(
    "geosite:ru", "geosite:category-ru", "geoip:ru", "geosite:cn", "geoip:cn", "geosite:ir", "geoip:ir",
    "geosite:ru-blocked", "geoip:ru-blocked", "geoip:ru-blocked-community", "geosite:lumen-exclude", "geoip:lumen-exclude",
    "geosite:gfw", "geosite:greatfire", "geosite:google", "geoip:facebook", "geoip:fastly", "geoip:google",
    "geoip:netflix", "geoip:telegram", "geoip:twitter"
)

/** Uses the existing ordered routing grammar, retaining unrelated custom rules. */
fun onboardingRouting(settings: SettingsUiState, region: SetupRegion, routing: SetupRouting): SettingsUiState {
    val retained = (settings.directDomains + "\n" + settings.directIpCidrs).lineSequence()
        .map(String::trim).filter(String::isNotBlank).filter { line ->
            val value = line.replace(Regex("^(direct|proxy|block|reject):", RegexOption.IGNORE_CASE), "").lowercase()
            value !in welcomeManagedRules
        }.toList()
    val managed = when {
        routing == SetupRouting.EXCEPT_LOCAL -> listOf(
            "direct:geosite:${if (region == SetupRegion.RUSSIA) "category-ru" else region.code}", "direct:geoip:${region.code}")
        routing == SetupRouting.BLOCKED && region == SetupRegion.RUSSIA -> listOf(
            "direct:geosite:lumen-exclude", "direct:geoip:lumen-exclude", "proxy:geosite:ru-blocked",
            "proxy:geoip:ru-blocked", "proxy:geoip:ru-blocked-community")
        routing == SetupRouting.BLOCKED && region == SetupRegion.CHINA -> listOf(
            "proxy:geosite:gfw", "proxy:geosite:greatfire", "proxy:geosite:google", "proxy:geoip:facebook",
            "proxy:geoip:fastly", "proxy:geoip:google", "proxy:geoip:netflix", "proxy:geoip:telegram", "proxy:geoip:twitter")
        else -> emptyList()
    }
    return onboardingNetworkDefaults(settings, region).copy(
        geoResourceSource = region.source,
        routingFinalOutbound = if (routing == SetupRouting.BLOCKED && region != SetupRegion.IRAN) "direct" else "proxy",
        directDomains = (retained + managed).distinct().joinToString("\n"), directIpCidrs = ""
    )
}

const val IRAN_DIRECT_DNS = "dns.google\ncloudflare-dns.com\n1.1.1.1\n8.8.8.8"
const val IRAN_PROXY_DNS = "cloudflare-dns.com\ndns.google\ndns.quad9.net"

/** Regional defaults are explicit setup choices; custom DNS/URLs stay intact. */
fun onboardingNetworkDefaults(settings: SettingsUiState, region: SetupRegion): SettingsUiState {
    val managedDns = settings.dnsMode != "json"
    if (region != SetupRegion.IRAN) {
        if (settings.dnsDirectServers.trim() == IRAN_DIRECT_DNS.trim()) {
            return settings.copy(
                dnsDirectServers = "1.1.1.1\n8.8.8.8",
                dnsDirectType = "udp",
                dnsProxyServers = "cloudflare-dns.com\ndns.google",
                dnsProxyType = "https",
                dnsParallelQuery = false
            )
        }
        return settings
    }
    val directLines = settings.dnsDirectServers.lines().map(String::trim).filter(String::isNotEmpty)
    val isDefaultDirect = directLines.isEmpty() || directLines == listOf("1.1.1.1", "8.8.8.8") || directLines == listOf("1.1.1.1") || settings.dnsDirectServers.trim() == IRAN_DIRECT_DNS.trim()
    val proxyLines = settings.dnsProxyServers.lines().map(String::trim).filter(String::isNotEmpty)
    val isDefaultProxy = proxyLines.isEmpty() || proxyLines == listOf("cloudflare-dns.com", "dns.google") || proxyLines == listOf("cloudflare-dns.com") || settings.dnsProxyServers.trim() == IRAN_PROXY_DNS.trim()
    return settings.copy(
        dnsMode = if (managedDns) "automatic" else settings.dnsMode,
        dnsHijackEnabled = if (managedDns) true else settings.dnsHijackEnabled,
        dnsDirectServers = if (managedDns && isDefaultDirect) IRAN_DIRECT_DNS else settings.dnsDirectServers,
        dnsDirectType = if (managedDns && isDefaultDirect && settings.dnsDirectType == "udp") "https" else settings.dnsDirectType,
        dnsProxyServers = if (managedDns && isDefaultProxy) IRAN_PROXY_DNS else settings.dnsProxyServers,
        dnsProxyType = if (managedDns && isDefaultProxy && settings.dnsProxyType == "udp") "https" else settings.dnsProxyType,
        dnsParallelQuery = if (managedDns) true else settings.dnsParallelQuery,
        pingType = "http",
        pingTimeoutMs = settings.pingTimeoutMs.coerceAtLeast(10_000),
        pingAttempts = settings.pingAttempts.coerceAtLeast(2),
        pingConcurrency = settings.pingConcurrency.coerceAtMost(4),
        pingAutoDeleteUnreachable = false,
        pingUrl = regionalProbeUrl(settings.pingUrl),
        urlTestUrl = regionalProbeUrl(settings.urlTestUrl)
    )
}

/** One-time upgrade of historical defaults, preserving explicit non-default choices. */
fun migrateIranNetworkDefaults(settings: SettingsUiState): SettingsUiState {
    if (settings.dnsMode == "json") return settings
    val recommended = onboardingNetworkDefaults(settings, SetupRegion.IRAN)
    val directLines = settings.dnsDirectServers.lines().map(String::trim).filter(String::isNotEmpty)
    val isDefaultDirect = directLines.isEmpty() || directLines == listOf("1.1.1.1", "8.8.8.8") || directLines == listOf("1.1.1.1") || settings.dnsDirectServers.trim() == IRAN_DIRECT_DNS.trim()
    val proxyLines = settings.dnsProxyServers.lines().map(String::trim).filter(String::isNotEmpty)
    val isDefaultProxy = proxyLines.isEmpty() || proxyLines == listOf("cloudflare-dns.com", "dns.google") || proxyLines == listOf("cloudflare-dns.com") || settings.dnsProxyServers.trim() == IRAN_PROXY_DNS.trim()
    val defaults = SettingsUiState()
    return settings.copy(
        dnsDirectServers = if (isDefaultDirect) recommended.dnsDirectServers else settings.dnsDirectServers,
        dnsDirectType = recommended.dnsDirectType,
        dnsProxyServers = if (isDefaultProxy) recommended.dnsProxyServers else settings.dnsProxyServers,
        dnsProxyType = recommended.dnsProxyType,
        dnsParallelQuery = if (isDefaultDirect && isDefaultProxy) recommended.dnsParallelQuery else settings.dnsParallelQuery,
        pingTimeoutMs = if (settings.pingTimeoutMs == defaults.pingTimeoutMs) recommended.pingTimeoutMs else settings.pingTimeoutMs,
        pingAttempts = if (settings.pingAttempts == defaults.pingAttempts) recommended.pingAttempts else settings.pingAttempts,
        pingConcurrency = if (settings.pingConcurrency == defaults.pingConcurrency) recommended.pingConcurrency else settings.pingConcurrency,
        pingUrl = recommended.pingUrl,
        urlTestUrl = recommended.urlTestUrl
    )
}

private fun regionalProbeUrl(value: String): String =
    if (value.isBlank() || value.trim() == "https://www.gstatic.com/generate_204")
        "https://www.google.com/generate_204" else value
