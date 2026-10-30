package com.lumen.ui

import com.lumen.ui.components.DockScrollVisibility
import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class ServerPreparationAndDockTest {
    private val nodes = listOf(
        NodeUiModel("a", "Zurich", "vless", "ch.example", 443, countryCode = "CH", pingMs = 28, subscriptionId = "one"),
        NodeUiModel("b", "amsterdam", "trojan", "nl.example", 443, countryCode = "NL", pingMs = 0, subscriptionId = "two"),
        NodeUiModel("c", "Berlin", "vless", "de.example", 443, countryCode = "DE", pingMs = 15, groupId = "work"),
        NodeUiModel("d", "Dublin", "vless", "ie.example", 443, countryCode = "IE", pingMs = null)
    )

    @Test fun preparedResultsPreserveAllFilterAndCountSemantics() {
        val all = prepareServerList(nodes, ServerListQuery("all"))
        assertEquals(listOf("c", "a", "b", "d"), all.visible.map { it.id })
        assertEquals(2, all.subscribedCount)
        assertEquals(1, all.manualCount)
        val protocol = protocolKey(nodes.first())
        val matched = prepareServerList(nodes, ServerListQuery("all", "EXAMPLE", protocol, onlyReachable = true))
        assertEquals(listOf("c", "a"), matched.visible.map { it.id })
        assertEquals(listOf("a"), prepareServerList(nodes, ServerListQuery("subscription:one")).visible.map { it.id })
        assertEquals(listOf("c"), prepareServerList(nodes, ServerListQuery("group:work")).visible.map { it.id })
        assertTrue(prepareServerList(nodes, ServerListQuery("all", "no match")).visible.isEmpty())
    }

    @Test fun selectionFastPathNeverHidesLatencyOrMembershipUpdates() {
        assertTrue(selectionOnlyUpdate(nodes, nodes.map { it.copy(isSelected = it.id == "c") }))
        assertFalse(selectionOnlyUpdate(nodes, nodes.map { if (it.id == "c") it.copy(pingMs = 900) else it }))
        assertFalse(selectionOnlyUpdate(nodes, nodes.map { if (it.id == "c") it.copy(groupId = "new") else it }))
        assertFalse(selectionOnlyUpdate(nodes, nodes.reversed()))
    }

    @Test fun allSortModesRetainServersAndPutUnavailablePingsLast() {
        ServerSort.entries.forEach { sort ->
            assertEquals(nodes.map { it.id }.toSet(), prepareServerList(nodes, ServerListQuery("all", sort = sort)).visible.map { it.id }.toSet())
        }
        assertEquals(listOf("c", "a", "b", "d"), prepareServerList(nodes, ServerListQuery("all", sort = ServerSort.PING)).visible.map { it.id })
        assertEquals(listOf("a", "d", "c", "b"), prepareServerList(nodes, ServerListQuery("all", sort = ServerSort.NAME_DESC)).visible.map { it.id })
    }

    @Test fun largeProviderListsAreNotTruncatedAndKeepDeterministicOrder() {
        val large = (9_999 downTo 0).map {
            NodeUiModel("$it", "Server ${it.toString().padStart(5, '0')}", "vless", "example.invalid", 443)
        }
        val result = prepareServerList(large, ServerListQuery("all"))
        assertEquals(10_000, result.visible.size)
        assertEquals("0", result.visible.first().id)
        assertEquals("9999", result.visible.last().id)
    }

    @Test fun dockIgnoresSmallAlternatingScrollsAndReversesAfterThreshold() {
        val dock = DockScrollVisibility(32f)
        repeat(30) { assertTrue(dock.onScroll(if (it % 2 == 0) 3f else -3f, 0f)) }
        assertTrue(dock.onScroll(-20f, 0f))
        assertTrue(dock.onScroll(-20f, 0f))
        assertFalse(dock.onScroll(-60f, 0f))
        assertFalse(dock.onScroll(4f, 0f))
        assertTrue(dock.onScroll(30f, 0f))
    }

    @Test fun bottomOverscrollDoesNotReverseTheDockAndSliderAnimation() {
        val dock = DockScrollVisibility(32f, keepHiddenAtBottom = true)
        assertFalse(dock.onScroll(-100f, 0f))
        repeat(30) { assertFalse(dock.onScroll(0f, -4f)) }
        assertTrue(dock.onScroll(40f, 0f))
        dock.reset()
        assertTrue(dock.visible)
        repeat(30) { assertTrue(dock.onScroll(0f, -4f)) } // A non-scrollable page.
        assertTrue(dock.onScroll(-1f, 0f))
    }

    @Test fun otherPagesKeepTheirVisibleNavigationAtTheBottom() {
        val dock = DockScrollVisibility(32f)
        assertFalse(dock.onScroll(-100f, 0f))
        assertTrue(dock.onScroll(0f, -4f))
    }

    @Test fun latencyCeilingExcludesUnknownAndUnreachableResults() {
        assertEquals(listOf("c"), prepareServerList(nodes, ServerListQuery("all", maxPing = 20)).visible.map { it.id })
        assertEquals(listOf("c", "a"), prepareServerList(nodes, ServerListQuery("all", maxPing = 100)).visible.map { it.id })
        assertEquals(listOf("a", "c", "d", "b"),
            prepareServerList(nodes, ServerListQuery("all", groupByCountry = true)).visible.map { it.id })
    }

    @Test fun flingNoiseDoesNotRevealDockButDeliberateUpwardFlingDoes() {
        val dock = DockScrollVisibility(32f)
        assertTrue(dock.onScroll(-30f, 0f))
        assertFalse(dock.onScroll(-100f, 0f))
        assertFalse(dock.onScroll(5f, 0f, direct = false))
        assertTrue(dock.onFling(900f))
        assertFalse(dock.onFling(-1800f))
        assertTrue(dock.onScroll(0f, 3f))
    }

    @Test fun countryGroupingRetainsLatencySortWithinEachCountry() {
        val sameCountry = listOf(
            nodes[0].copy(id = "slow", name = "Alpha", pingMs = 80),
            nodes[0].copy(id = "fast", name = "Zulu", pingMs = 10),
            nodes[2]
        )
        assertEquals(listOf("fast", "slow", "c"), prepareServerList(sameCountry,
            ServerListQuery("all", sort = ServerSort.PING, groupByCountry = true)).visible.map { it.id })
    }
}
