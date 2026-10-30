package com.lumen.app

import androidx.room.Room
import androidx.room.withTransaction
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.lumen.core.database.AppDatabase
import com.lumen.core.database.model.NodeEntity
import com.lumen.core.database.model.SubscriptionEntity
import com.lumen.core.database.model.ServerGroupEntity
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** This database is in-memory; the user's lumen.db is never opened. */
@RunWith(AndroidJUnit4::class)
class ResetDatabaseTest {
    @Test fun serverResetClearsSubscriptionsNodesAndGroupsAtomically() = runBlocking {
        val database = Room.inMemoryDatabaseBuilder(ApplicationProvider.getApplicationContext(), AppDatabase::class.java).build()
        try {
            database.subscriptionDao().insertSubscription(SubscriptionEntity(id = "sub", name = "Test", url = "https://example.invalid/sub"))
            database.nodeDao().insertNodes(listOf(NodeEntity(name = "Test", protocol = "socks", server = "example.invalid", port = 1080, link = "socks://example.invalid:1080", subscriptionId = "sub")))
            database.serverGroupDao().insertGroup(ServerGroupEntity(id = "group", name = "Test"))
            database.withTransaction {
                database.serverGroupDao().clearAllMembers()
                database.serverGroupDao().clearAllGroups()
                database.nodeDao().clearAllNodes()
                database.subscriptionDao().clearAllSubscriptions()
            }
            assertTrue(database.nodeDao().getNodes().first().isEmpty())
            assertTrue(database.subscriptionDao().getSubscriptions().first().isEmpty())
            assertTrue(database.serverGroupDao().getGroups().first().isEmpty())
            assertTrue(database.serverGroupDao().getMembers().first().isEmpty())
        } finally { database.close() }
    }
}
