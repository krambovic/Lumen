package com.lumen.core.database

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.lumen.core.database.dao.NodeDao
import com.lumen.core.database.dao.SubscriptionDao
import com.lumen.core.database.model.NodeEntity
import com.lumen.core.database.model.SubscriptionEntity

@Database(
    entities = [NodeEntity::class, SubscriptionEntity::class],
    version = 3,
    exportSchema = false
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun nodeDao(): NodeDao
    abstract fun subscriptionDao(): SubscriptionDao

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
