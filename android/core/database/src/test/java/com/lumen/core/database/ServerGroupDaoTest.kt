package com.lumen.core.database

import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import com.lumen.core.database.dao.NodeDao
import com.lumen.core.database.dao.ServerGroupDao
import com.lumen.core.database.model.NodeEntity
import com.lumen.core.database.model.ServerGroupEntity
import com.lumen.core.database.model.groupKey
import com.lumen.core.database.model.nodeGroupKey
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class ServerGroupDaoTest {

    private lateinit var db: AppDatabase
    private lateinit var nodeDao: NodeDao
    private lateinit var groupDao: ServerGroupDao

    @Before
    fun createDb() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        db = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        nodeDao = db.nodeDao()
        groupDao = db.serverGroupDao()
    }

    @After
    fun closeDb() {
        db.close()
    }

    private fun node(
        id: String,
        link: String,
        subscriptionId: String? = null
    ) = NodeEntity(
        id = id,
        name = "node-$id",
        protocol = "vless",
        server = "1.2.3.4",
        port = 443,
        link = link,
        subscriptionId = subscriptionId
    )

    @Test
    fun subscriptionNodesKeyOnTheirLinkAndManualNodesOnTheirId() {
        val manual = node("m1", "vless://manual")
        val fromSub = node("s1", "vless://sub", subscriptionId = "sub-1")
        assertEquals("id:m1", manual.groupKey())
        assertEquals("sub:sub-1|vless://sub", fromSub.groupKey())
        // Same link under two subscriptions is two different servers.
        assertNotEquals(fromSub.groupKey(), node("s2", "vless://sub", "sub-2").groupKey())
        // A subscription entry without a usable link falls back to the row id.
        assertEquals("id:s3", nodeGroupKey("s3", "sub-1", "   "))
    }

    @Test
    fun assignMovesAndClearsMembership() = runBlocking {
        val group = ServerGroupEntity(id = "g1", name = "Work")
        groupDao.insertGroup(group)
        val other = ServerGroupEntity(id = "g2", name = "Home")
        groupDao.insertGroup(other)

        val a = node("n1", "vless://a")
        val b = node("n2", "vless://b")
        nodeDao.insertNodes(listOf(a, b))

        groupDao.assignNodes(listOf(a.groupKey(), b.groupKey()), "g1")
        assertEquals(2, groupDao.membersOf("g1").size)

        // Moving to another group replaces the row: a node is in at most one group.
        groupDao.assignNodes(listOf(b.groupKey()), "g2")
        assertEquals(listOf(a.groupKey()), groupDao.membersOf("g1").map { it.nodeKey })
        assertEquals(listOf(b.groupKey()), groupDao.membersOf("g2").map { it.nodeKey })
        assertEquals(2, groupDao.getMembers().first().size)

        // A null group clears the assignment.
        groupDao.assignNodes(listOf(a.groupKey()), null)
        assertTrue(groupDao.membersOf("g1").isEmpty())
        assertEquals(1, groupDao.getMembers().first().size)
    }

    @Test
    fun deletingAGroupKeepsItsServers() = runBlocking {
        groupDao.insertGroup(ServerGroupEntity(id = "g1", name = "Work"))
        val a = node("n1", "vless://a")
        val b = node("n2", "vless://b", subscriptionId = "sub-1")
        nodeDao.insertNodes(listOf(a, b))
        groupDao.assignNodes(listOf(a.groupKey(), b.groupKey()), "g1")

        groupDao.deleteGroup("g1")

        assertTrue(groupDao.getGroups().first().isEmpty())
        assertTrue(groupDao.getMembers().first().isEmpty())
        // The whole point of the confirmation wording: both servers are still there.
        assertEquals(setOf("n1", "n2"), nodeDao.getNodes().first().mapTo(mutableSetOf()) { it.id })
    }

    @Test
    fun renameKeepsMembership() = runBlocking {
        groupDao.insertGroup(ServerGroupEntity(id = "g1", name = "Work"))
        val a = node("n1", "vless://a")
        nodeDao.insertNode(a)
        groupDao.assignNodes(listOf(a.groupKey()), "g1")

        groupDao.renameGroup("g1", "Office")

        assertEquals(listOf("Office"), groupDao.getGroups().first().map { it.name })
        assertEquals(1, groupDao.membersOf("g1").size)
    }

    @Test
    fun membershipSurvivesASubscriptionRefresh() = runBlocking {
        groupDao.insertGroup(ServerGroupEntity(id = "g1", name = "Work"))
        val before = node("old-id", "vless://kept", subscriptionId = "sub-1")
        nodeDao.insertNode(before)
        groupDao.assignNodes(listOf(before.groupKey()), "g1")

        // Exactly what refreshSubscriptionInternal does: wipe the subscription's nodes
        // and re-insert the parsed ones, which get brand new row ids.
        nodeDao.deleteNodesBySubscription("sub-1")
        val after = node("new-id", "vless://kept", subscriptionId = "sub-1")
        nodeDao.insertNode(after)

        assertEquals(before.groupKey(), after.groupKey())
        assertEquals("g1", groupDao.getMembers().first().single { it.nodeKey == after.groupKey() }.groupId)
    }
}
