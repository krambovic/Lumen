package com.lumen.core.database

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.lumen.core.database.dao.NodeDao
import com.lumen.core.database.dao.ServerGroupDao
import com.lumen.core.database.dao.SubscriptionDao
import com.lumen.core.database.model.NodeEntity
import com.lumen.core.database.model.NodeGroupMemberEntity
import com.lumen.core.database.model.ServerGroupEntity
import com.lumen.core.database.model.SubscriptionEntity

@Database(
    entities = [
        NodeEntity::class,
        SubscriptionEntity::class,
        ServerGroupEntity::class,
        NodeGroupMemberEntity::class
    ],
    version = 5,
    exportSchema = false
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun nodeDao(): NodeDao
    abstract fun subscriptionDao(): SubscriptionDao
    abstract fun serverGroupDao(): ServerGroupDao

    companion object {
        val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    "CREATE TABLE IF NOT EXISTS `subscriptions` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, `url` TEXT NOT NULL, `lastUpdated` INTEGER NOT NULL, `autoUpdateEnabled` INTEGER NOT NULL, PRIMARY KEY(`id`))"
                )
                if (!db.hasColumn("nodes", "subscriptionId")) {
                    db.execSQL("ALTER TABLE `nodes` ADD COLUMN `subscriptionId` TEXT DEFAULT NULL")
                }
                if (!db.hasColumn("nodes", "isAutoNode")) {
                    db.execSQL("ALTER TABLE `nodes` ADD COLUMN `isAutoNode` INTEGER NOT NULL DEFAULT 0")
                }
            }
        }

        val MIGRATION_2_3 = object : Migration(2, 3) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("CREATE INDEX IF NOT EXISTS `index_nodes_subscriptionId` ON `nodes` (`subscriptionId`)")
            }
        }

        /**
         * User-made server groups. Purely additive: two new tables, nothing dropped and
         * no row rewritten, so every server and subscription survives the upgrade.
         */
        val MIGRATION_3_4 = object : Migration(3, 4) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    "CREATE TABLE IF NOT EXISTS `server_groups` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, `createdAt` INTEGER NOT NULL, PRIMARY KEY(`id`))"
                )
                db.execSQL(
                    "CREATE TABLE IF NOT EXISTS `node_group_members` (`nodeKey` TEXT NOT NULL, `groupId` TEXT NOT NULL, PRIMARY KEY(`nodeKey`))"
                )
                db.execSQL(
                    "CREATE INDEX IF NOT EXISTS `index_node_group_members_groupId` ON `node_group_members` (`groupId`)"
                )
            }
        }

        /**
         * Provider metadata carried by the subscription headers (announcement, support
         * and website links, banner, sort order). Purely additive: columns are appended
         * to `subscriptions` with defaults, so no row is rewritten and nothing is lost.
         */
        val MIGRATION_4_5 = object : Migration(4, 5) {
            private val textColumns = listOf(
                "description", "announce", "announceUrl", "telegramUrl", "supportUrl",
                "supportEmail", "websiteUrl", "premiumUrl", "bannerText", "bannerButtonText",
                "bannerButtonUrl", "bannerBgColor", "bannerButtonColor", "sortOrder"
            )

            override fun migrate(db: SupportSQLiteDatabase) {
                textColumns.forEach { column ->
                    if (!db.hasColumn("subscriptions", column)) {
                        db.execSQL("ALTER TABLE `subscriptions` ADD COLUMN `$column` TEXT NOT NULL DEFAULT ''")
                    }
                }
                if (!db.hasColumn("subscriptions", "hideUrl")) {
                    db.execSQL("ALTER TABLE `subscriptions` ADD COLUMN `hideUrl` INTEGER NOT NULL DEFAULT 0")
                }
                if (!db.hasColumn("subscriptions", "updateIntervalHours")) {
                    db.execSQL("ALTER TABLE `subscriptions` ADD COLUMN `updateIntervalHours` INTEGER NOT NULL DEFAULT 0")
                }
            }
        }

        private fun SupportSQLiteDatabase.hasColumn(table: String, column: String): Boolean =
            query("PRAGMA table_info(`$table`)").use { cursor ->
                val nameIndex = cursor.getColumnIndex("name")
                while (cursor.moveToNext()) {
                    if (nameIndex >= 0 && cursor.getString(nameIndex) == column) return@use true
                }
                false
            }
    }
}
