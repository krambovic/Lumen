package com.lumen.core.database

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import com.lumen.core.database.model.ServerGroupEntity
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * `fallbackToDestructiveMigration` was removed because it silently wiped users'
 * servers, so every step has to be a real migration: a v3 file must open at the current
 * version with every node and subscription still in it.
 *
 * Room is left to run the migrations itself, which also makes it validate the resulting
 * schema against the compiled entities — a migration that produced the wrong tables
 * would fail here instead of on a user's device.
 */
@RunWith(RobolectricTestRunner::class)
class AppDatabaseMigrationTest {

    private val dbName = "migration-test.db"
    private lateinit var context: Context

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        context.getDatabasePath(dbName).also { file ->
            file.parentFile?.mkdirs()
            if (file.exists()) file.delete()
        }
    }

    @After
    fun tearDown() {
        context.getDatabasePath(dbName).delete()
    }

    /** The schema as version 3 left it, written by hand so no v4 code can influence it. */
    private fun createVersion3Database() {
        val db = SQLiteDatabase.openOrCreateDatabase(context.getDatabasePath(dbName), null)
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `nodes` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, " +
                "`protocol` TEXT NOT NULL, `server` TEXT NOT NULL, `port` INTEGER NOT NULL, " +
                "`link` TEXT NOT NULL, `outboundJson` TEXT NOT NULL, `pingMs` INTEGER, " +
                "PRIMARY KEY(`id`))"
        )
        // Added by MIGRATION_1_2, exactly as it adds them.
        db.execSQL("ALTER TABLE `nodes` ADD COLUMN `subscriptionId` TEXT DEFAULT NULL")
        db.execSQL("ALTER TABLE `nodes` ADD COLUMN `isAutoNode` INTEGER NOT NULL DEFAULT 0")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `subscriptions` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, " +
                "`url` TEXT NOT NULL, `lastUpdated` INTEGER NOT NULL, " +
                "`autoUpdateEnabled` INTEGER NOT NULL, PRIMARY KEY(`id`))"
        )
        // Added by MIGRATION_2_3.
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_nodes_subscriptionId` ON `nodes` (`subscriptionId`)"
        )
        db.execSQL(
            "INSERT INTO `subscriptions` VALUES ('sub-1', 'Provider', 'https://example.com/sub', 42, 1)"
        )
        db.execSQL(
            "INSERT INTO `nodes` (`id`, `name`, `protocol`, `server`, `port`, `link`, " +
                "`outboundJson`, `pingMs`, `subscriptionId`, `isAutoNode`) VALUES " +
                "('n-manual', 'Hand typed', 'trojan', 'manual.example', 8443, 'trojan://x', '', 77, NULL, 0)"
        )
        db.execSQL(
            "INSERT INTO `nodes` (`id`, `name`, `protocol`, `server`, `port`, `link`, " +
                "`outboundJson`, `pingMs`, `subscriptionId`, `isAutoNode`) VALUES " +
                "('n-sub', 'From provider', 'vless', 'sub.example', 443, 'vless://y', '{}', NULL, 'sub-1', 0)"
        )
        db.version = 3
        db.close()
    }

    private fun openAtCurrentVersion(): AppDatabase =
        Room.databaseBuilder(context, AppDatabase::class.java, dbName)
            .addMigrations(
                AppDatabase.MIGRATION_1_2,
                AppDatabase.MIGRATION_2_3,
                AppDatabase.MIGRATION_3_4,
                AppDatabase.MIGRATION_4_5
            )
            .allowMainThreadQueries()
            .build()

    @Test
    fun migrate3To4KeepsEveryServerAndSubscription() = runBlocking {
        createVersion3Database()

        val db = openAtCurrentVersion()
        try {
            val nodes = db.nodeDao().getNodes().first().associateBy { it.id }
            assertEquals(2, nodes.size)

            val manual = requireNotNull(nodes["n-manual"])
            assertEquals("Hand typed", manual.name)
            assertEquals("trojan", manual.protocol)
            assertEquals("manual.example", manual.server)
            assertEquals(8443, manual.port)
            assertEquals("trojan://x", manual.link)
            assertEquals(77, manual.pingMs)
            assertEquals(null, manual.subscriptionId)

            val fromSub = requireNotNull(nodes["n-sub"])
            assertEquals("sub-1", fromSub.subscriptionId)
            assertEquals("vless://y", fromSub.link)
            assertEquals(null, fromSub.pingMs)

            val subs = db.subscriptionDao().getSubscriptions().first()
            assertEquals(1, subs.size)
            assertEquals("https://example.com/sub", subs[0].url)
            assertEquals(42L, subs[0].lastUpdated)
            assertTrue(subs[0].autoUpdateEnabled)
        } finally {
            db.close()
        }
    }

    @Test
    fun migrate3To4AddsUsableGroupTables() = runBlocking {
        createVersion3Database()

        val db = openAtCurrentVersion()
        try {
            val groupDao = db.serverGroupDao()
            assertTrue(groupDao.getGroups().first().isEmpty())

            groupDao.insertGroup(ServerGroupEntity(id = "g1", name = "Work"))
            groupDao.assignNodes(listOf("id:n-manual"), "g1")

            assertEquals(listOf("Work"), groupDao.getGroups().first().map { it.name })
            assertEquals(listOf("id:n-manual"), groupDao.membersOf("g1").map { it.nodeKey })
        } finally {
            db.close()
        }
    }

    @Test
    fun reopeningTheMigratedDatabaseKeepsTheGroups() = runBlocking {
        createVersion3Database()

        val first = openAtCurrentVersion()
        try {
            first.serverGroupDao().insertGroup(ServerGroupEntity(id = "g1", name = "Work"))
            first.serverGroupDao().assignNodes(listOf("id:n-manual"), "g1")
        } finally {
            first.close()
        }

        val second = openAtCurrentVersion()
        try {
            assertEquals(listOf("g1"), second.serverGroupDao().getGroups().first().map { it.id })
            assertEquals("g1", second.serverGroupDao().getMembers().first().single().groupId)
            assertEquals(2, second.nodeDao().getNodes().first().size)
        } finally {
            second.close()
        }
    }

    @Test
    fun migrate4To5AddsEmptyProviderMetadataToExistingSubscriptions() = runBlocking {
        createVersion3Database()

        val db = openAtCurrentVersion()
        try {
            val sub = db.subscriptionDao().getSubscriptions().first().single()
            assertEquals("Provider", sub.name)
            // A subscription that predates the metadata columns reads back as "nothing
            // sent yet" rather than failing to map.
            assertEquals("", sub.announce)
            assertEquals("", sub.announceUrl)
            assertEquals("", sub.supportUrl)
            assertEquals("", sub.websiteUrl)
            assertEquals("", sub.bannerText)
            assertEquals("", sub.sortOrder)
            assertEquals(false, sub.hideUrl)
            assertEquals(0, sub.updateIntervalHours)

            db.subscriptionDao().updateSubscription(
                sub.copy(announce = "Hello", supportUrl = "https://t.me/x", updateIntervalHours = 6)
            )
        } finally {
            db.close()
        }

        val reopened = openAtCurrentVersion()
        try {
            val sub = reopened.subscriptionDao().getSubscriptions().first().single()
            assertEquals("Hello", sub.announce)
            assertEquals("https://t.me/x", sub.supportUrl)
            assertEquals(6, sub.updateIntervalHours)
        } finally {
            reopened.close()
        }
    }
}
