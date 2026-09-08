package com.lumen.app.vm

import com.lumen.core.database.model.NodeEntity
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SubscriptionRefreshRegressionTest {
    private fun node(index: Int, id: String = "old-$index") = NodeEntity(
        id = id,
        name = "Node ${index % 250}",
        protocol = "vless",
        server = "server-${index % 250}.example",
        port = 443,
        link = "vless://credential-$index@server-${index % 250}.example:443",
        outboundJson = JSONObject(mapOf("uuid" to "credential-$index", "type" to "vless")).toString(),
        subscriptionId = "sub",
        pingMs = index
    )

    @Test fun refreshKeepsAll958RowsSharing250Endpoints() {
        val old = (0 until 958).map { node(it) }
        val fresh = old.reversed().mapIndexed { index, n -> n.copy(id = "new-$index", pingMs = null) }
        val result = reconcileSubscriptionNodes(old, fresh)
        assertEquals(958, result.nodes.size)
        assertEquals(958, result.nodes.map { it.id }.toSet().size)
        assertEquals(old.associate { it.link to it.id }, result.nodes.associate { it.link to it.id })
        assertEquals(0, result.added)
        assertEquals(0, result.removed)
        assertEquals(0, result.updated)
    }

    @Test fun firstImportAndEveryLaterRefreshHaveTheSameCardinality() {
        val fresh = (0 until 958).map { node(it, "incoming-$it") }
        var stored = reconcileSubscriptionNodes(emptyList(), fresh).nodes
        repeat(3) {
            stored = reconcileSubscriptionNodes(stored, fresh.reversed()).nodes
            assertEquals(958, stored.map { it.id }.toSet().size)
        }
    }

    @Test fun exactMatchesAreReservedBeforeRotatedCredentials() {
        val old = listOf(node(0), node(250))
        val rotated = node(500, "new-rotated")
        val result = reconcileSubscriptionNodes(old, listOf(rotated, node(0, "new-exact")))
        assertEquals("old-250", result.nodes[0].id)
        assertEquals("old-0", result.nodes[1].id)
        assertEquals(1, result.updated)
        assertEquals(2, result.nodes.map { it.id }.toSet().size)
    }

    @Test fun identicalDuplicateRowsNeverShareAnId() {
        val old = listOf(node(0), node(0, "old-duplicate"))
        val result = reconcileSubscriptionNodes(old, listOf(node(0, "new-a"), node(0, "new-b")))
        assertEquals(setOf("old-0", "old-duplicate"), result.nodes.map { it.id }.toSet())
    }

    @Test fun aCompleteLegitimateShrinkStillRemovesMissingNodes() {
        val old = (0 until 958).map { node(it) }
        val result = reconcileSubscriptionNodes(old, old.take(250))
        assertEquals(250, result.nodes.size)
        assertEquals(708, result.removed)
    }

    @Test fun jsonKeyOrderDoesNotSwapRowsAtASharedEndpoint() {
        val old = node(0).copy(outboundJson = "{\"uuid\":\"a\",\"type\":\"vless\"}")
        val new = old.copy(id = "new", outboundJson = "{\"type\":\"vless\",\"uuid\":\"a\"}")
        assertEquals(old.id, reconcileSubscriptionNodes(listOf(old), listOf(new)).nodes.single().id)
    }

    @Test fun offIsPreservedByPersistenceNormalization() {
        assertEquals(0, normalizedSubscriptionAutoUpdateMinutes(0))
        assertEquals(0, normalizedSubscriptionAutoUpdateMinutes(-1))
        assertEquals(15, normalizedSubscriptionAutoUpdateMinutes(1))
        assertEquals(240, normalizedSubscriptionAutoUpdateMinutes(240))
    }

    @Test fun providerIntervalNeverOverridesTheUsersOptOut() {
        assertNull(subscriptionAutomaticIntervalMinutes(0, true, 1))
        assertNull(subscriptionAutomaticIntervalMinutes(240, false, 1))
        assertEquals(60, subscriptionAutomaticIntervalMinutes(240, true, 1))
        assertEquals(240, subscriptionAutomaticIntervalMinutes(240, true, 0))
    }

    @Test fun disabledAndReenabledAutoUpdateRejectsAnOlderResponse() {
        assertFalse(canApplySubscriptionRefresh(true, 1, 3, 240, true, "https://example/sub", "https://example/sub"))
        assertFalse(canApplySubscriptionRefresh(true, 1, 1, 0, true, "https://example/sub", "https://example/sub"))
        assertTrue(canApplySubscriptionRefresh(true, 1, 1, 240, true, "https://example/sub", "https://example/sub"))
    }

    @Test fun manualRefreshWorksWhenAutoUpdateIsOffButCannotResurrectADeletedGroup() {
        assertTrue(canApplySubscriptionRefresh(false, 1, 2, 0, false, "https://example/sub", "https://example/sub"))
        assertFalse(canApplySubscriptionRefresh(false, 1, 2, 0, false, "https://example/sub", null))
        assertFalse(canApplySubscriptionRefresh(false, 1, 2, 0, false, "https://example/sub", "https://example/changed"))
    }

    @Test fun partialParsingCannotDeleteThePreviouslySavedList() {
        assertFalse(canReplaceSubscriptionNodes(958, 250, 250, 1))
        assertFalse(canReplaceSubscriptionNodes(958, 958, 250, 0))
        assertFalse(canReplaceSubscriptionNodes(958, 0, 0, 0))
        assertTrue(canReplaceSubscriptionNodes(958, 250, 250, 0))
        assertTrue(canReplaceSubscriptionNodes(0, 250, 250, 1))
    }
}
