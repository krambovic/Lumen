package com.lumen.ui

import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class DashboardServerPresentationTest {
    private fun node(id: String, ping: Int? = null) = NodeUiModel(id, id, "vless", "example.invalid", 443, pingMs = ping)

    @Test fun successfulPingsSortBeforeFailuresAndUnknownNodes() {
        val nodes = listOf(node("Unknown"), node("Failed", 0), node("Slow", 150), node("Fast", 12))
        assertEquals(listOf("Fast", "Slow", "Failed", "Unknown"), dashboardServersByPing(nodes, "").map { it.id })
        assertEquals(listOf("Fast", "Failed"), dashboardServersByPing(nodes, "fa").map { it.id })
        assertEquals(listOf("Slow", "Fast", "Failed", "Unknown"),
            dashboardServersByPing(nodes.map { if (it.id == "Slow") it.copy(pingMs = 5) else it }, "").map { it.id })
    }

    @Test fun technicalTagsAreHiddenWithoutDamagingRealNames() {
        assertEquals("London", cleanServerDisplayName("London · proxy"))
        assertEquals("NL 02", cleanServerDisplayName("NL 02 · proxy-2 · direct"))
        assertEquals("Office · Backup", cleanServerDisplayName("Office · Backup"))
        assertEquals("my.proxy.example", cleanServerDisplayName("my.proxy.example"))
        assertEquals("proxy", cleanServerDisplayName("proxy"))
    }

    @Test fun eachProbeResultImmediatelyChangesOrder() {
        val before = listOf(node("Fast", 12), node("Slow", 150), node("Unknown"))
        val partial = before.map { if (it.id == "Slow") it.copy(pingMs = 8) else it }
        assertEquals(listOf("Slow", "Fast", "Unknown"),
            dashboardServersByPing(partial, "").map { it.id })
        val next = partial.map { if (it.id == "Unknown") it.copy(pingMs = 5) else it }
        assertEquals(listOf("Unknown", "Slow", "Fast"),
            dashboardServersByPing(next, "").map { it.id })
    }

    @Test fun manualIsAlwaysSecondEvenWithNoManualServers() {
        val groups = listOf(ServerGroupUiModel("work", "Work"))
        val subscriptions = listOf(SubscriptionUiModel("provider", "Provider", "https://example.invalid"))
        assertEquals(listOf("all", "manual", "group:work", "subscription:provider"),
            serverFilterChoices(emptyList(), groups, subscriptions, stringsForLanguage("en")).map { it.first })
        assertEquals("all", resolvedServerFilter("sub", subscriptions, groups))
    }
}
