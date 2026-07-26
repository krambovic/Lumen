package com.lumen.ui

import com.lumen.ui.screens.NodeUiModel
import com.lumen.ui.screens.ServerGroupUiModel
import com.lumen.ui.screens.SubscriptionUiModel
import com.lumen.ui.screens.nodeInGroup
import com.lumen.ui.screens.nodesInGroup
import com.lumen.ui.screens.serverGroupIds
import com.lumen.ui.screens.stringsForLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The group selector, the dashboard and the filters all read membership through
 * [nodeInGroup], so the rule it encodes is pinned here.
 */
class ServerGroupingTest {

    // The two built-in ids, read off the public selector list. They are also the values
    // stored in the "servers_last_group" preference, so they must not drift.
    private val builtIn = serverGroupIds(emptyList(), emptyList())
    private val groupAll = builtIn[0]
    private val groupManual = builtIn[1]

    private fun node(
        id: String,
        subscriptionId: String? = null,
        groupId: String? = null
    ) = NodeUiModel(
        id = id,
        name = id,
        protocol = "vless",
        server = "1.2.3.4",
        port = 443,
        subscriptionId = subscriptionId,
        groupId = groupId
    )

    private val manual = node("manual")
    private val fromSub = node("fromSub", subscriptionId = "sub-1")
    private val manualInGroup = node("manualInGroup", groupId = "g1")
    private val subInGroup = node("subInGroup", subscriptionId = "sub-1", groupId = "g1")
    private val all = listOf(manual, fromSub, manualInGroup, subInGroup)

    @Test
    fun defaultStaysSelectableWithNoGroupsAtAll() {
        assertEquals(listOf("all", "manual"), builtIn)
    }

    @Test
    fun allServersKeepsShowingEverything() {
        assertEquals(all, nodesInGroup(all, groupAll))
    }

    @Test
    fun aCustomGroupTakesOverFromDefaultAndFromTheSubscription() {
        assertEquals(listOf(manualInGroup, subInGroup), nodesInGroup(all, "g1"))
        // Both leave the bucket they would otherwise sit in.
        assertEquals(listOf(manual), nodesInGroup(all, groupManual))
        assertEquals(listOf(fromSub), nodesInGroup(all, "sub-1"))
    }

    @Test
    fun clearingTheGroupReturnsAServerToItsDefaultBucket() {
        val released = manualInGroup.copy(groupId = null)
        assertTrue(nodeInGroup(released, groupManual))
        assertFalse(nodeInGroup(released, "g1"))

        // A subscription server released from a group goes back to its subscription,
        // not to Default: subscription membership is the other axis and never moved.
        val releasedSub = subInGroup.copy(groupId = null)
        assertTrue(nodeInGroup(releasedSub, "sub-1"))
        assertFalse(nodeInGroup(releasedSub, groupManual))
    }

    @Test
    fun aServerIsNeverInTwoSelectorGroupsAtOnce() {
        val selectorIds = serverGroupIds(
            listOf(ServerGroupUiModel("g1", "Work")),
            listOf(SubscriptionUiModel("sub-1", "Provider", "https://example.com"))
        ) - groupAll
        all.forEach { candidate ->
            assertEquals(
                "${candidate.id} must appear in exactly one selector group",
                1,
                selectorIds.count { nodeInGroup(candidate, it) }
            )
        }
    }

    @Test
    fun selectorListsAllThenDefaultThenCustomGroupsThenSubscriptions() {
        val ids = serverGroupIds(
            listOf(ServerGroupUiModel("g1", "Work"), ServerGroupUiModel("g2", "Home")),
            listOf(
                SubscriptionUiModel("sub-1", "Provider", "https://example.com"),
                SubscriptionUiModel("sub-2", "Other", "https://example.org")
            )
        )
        assertEquals(listOf(groupAll, groupManual, "g1", "g2", "sub-1", "sub-2"), ids)
    }

    @Test
    fun everyLanguageLabelsTheGroupActions() {
        listOf("en", "ru", "zh", "fa").forEach { code ->
            val s = stringsForLanguage(code)
            listOf(
                s.newGroup, s.groupNameLabel, s.createAction, s.renameGroup,
                s.deleteGroup, s.deleteGroupConfirm, s.moveToGroup, s.groupNoGroup
            ).forEach { label ->
                assertTrue("blank group label in $code", label.isNotBlank())
            }
        }
        // Not just the English fallback copied around.
        val english = stringsForLanguage("en")
        listOf("ru", "zh", "fa").forEach { code ->
            assertFalse(
                "$code did not translate moveToGroup",
                stringsForLanguage(code).moveToGroup == english.moveToGroup
            )
        }
    }
}
