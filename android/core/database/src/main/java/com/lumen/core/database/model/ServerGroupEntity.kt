package com.lumen.core.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey
import java.util.UUID

/**
 * A group the user made by hand. It owns nothing: membership lives in
 * [NodeGroupMemberEntity], so dropping the group never touches a server row.
 */
@Entity(tableName = "server_groups")
data class ServerGroupEntity(
    @PrimaryKey
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val createdAt: Long = System.currentTimeMillis()
)
