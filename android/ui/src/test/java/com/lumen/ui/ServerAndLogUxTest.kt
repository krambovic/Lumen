package com.lumen.ui

import com.lumen.ui.components.hasProviderCard
import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class ServerAndLogUxTest {
    private val subscriptions = listOf(
        SubscriptionUiModel("one", "One", "https://example.invalid/one"),
        SubscriptionUiModel("two", "Two", "https://example.invalid/two")
    )

    @Test fun aggregateFiltersNeverShowAnUnrelatedProviderBanner() {
        listOf("all", "sub", "manual", "group:one", "subscription:missing").forEach {
            assertNull(subscriptionForServerFilter(it, subscriptions))
        }
    }

    @Test fun selectingASubscriptionShowsExactlyItsProvider() {
        assertEquals("two", subscriptionForServerFilter("subscription:two", subscriptions)?.id)
    }

    @Test fun announcementLinkWithoutTextStillHasAProviderBlock() {
        assertTrue(hasProviderCard(subscriptions.first().copy(announceUrl = "https://example.invalid/news")))
    }

    @Test fun logSearchIncludesRecordsOlderThanTheLastTwoHundred() {
        val entries = (0 until 1_200).map {
            LogEntryUi(it.toLong(), "12:00:00", "info", "VPN", "event $it")
        }
        assertEquals(1_200, filterLogEntries(entries, LOG_FILTER_ALL, "").size)
        assertEquals("event 0", filterLogEntries(entries, LOG_FILTER_ALL, "event 0").single().message)
    }

    @Test fun logIdentitiesKeepTheViewportAnchoredWhenHistoryArrives() {
        val current = LogEntryUi(20, "12:00:20", "info", "VPN", "connected")
        val previous = LogEntryUi(10, "12:00:10", "debug", "VPN", "starting")
        assertEquals(logIdentityKeys(listOf(current)).single(), logIdentityKeys(listOf(previous, current)).last())
        assertEquals(2, logIdentityKeys(listOf(current, current)).distinct().size)
    }

    @Test fun newControlsAreLocalized() {
        listOf("ru", "en", "fa", "zh").forEach { language ->
            val strings = stringsForLanguage(language)
            listOf(strings.serverGroup, strings.changeServer, strings.logSearch, strings.logLatest, strings.logHistoryLoading, strings.logHistoryEnd)
                .forEach { assertTrue("Missing label for $language", it.isNotBlank()) }
        }
    }

    @Test fun legacyImportGroupAndExplicitSelectionResolveToTheSameFilter() {
        val groups = listOf(ServerGroupUiModel("custom", "Work"))
        assertEquals("subscription:two", resolvedServerFilter("two", subscriptions, groups))
        assertEquals("group:custom", resolvedServerFilter("custom", subscriptions, groups))
        assertEquals("subscription:two", resolvedServerFilter("subscription:two", subscriptions, groups))
        assertEquals("all", resolvedServerFilter("subscription:deleted", subscriptions, groups))
    }

    @Test fun dashboardOnlyReceivesServersFromTheChosenGroup() {
        val nodes = listOf(
            NodeUiModel("a", "One", "vless", "example.invalid", 443, subscriptionId = "one"),
            NodeUiModel("b", "Two", "vless", "example.invalid", 443, subscriptionId = "two"),
            NodeUiModel("c", "Work", "vless", "example.invalid", 443, groupId = "custom")
        )
        assertEquals(listOf("b"), nodesForServerFilter(nodes, "subscription:two").map { it.id })
        assertEquals(listOf("c"), nodesForServerFilter(nodes, "group:custom").map { it.id })
    }

    @Test fun sliderKeepsConnectingAndConnectedOnTheRight() {
        val connecting = com.lumen.ui.components.ConnectionState.Connecting
        val connected = com.lumen.ui.components.ConnectionState.Connected
        val disconnected = com.lumen.ui.components.ConnectionState.Disconnected
        assertTrue(com.lumen.ui.components.sliderAtRight(connecting))
        assertTrue(com.lumen.ui.components.sliderAtRight(connected))
        assertFalse(com.lumen.ui.components.sliderAtRight(disconnected))
        assertTrue(com.lumen.ui.components.sliderDragShouldToggle(connecting, .1f))
        assertFalse(com.lumen.ui.components.sliderDragShouldToggle(connecting, .9f))
        assertTrue(com.lumen.ui.components.sliderDragShouldToggle(disconnected, .9f))
    }
}
