package com.lumen.core.database.model

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

/**
 * Membership of one server in one custom group, keyed on [nodeGroupKey] instead of
 * the node row id. A subscription refresh deletes every node of that subscription and
 * re-inserts the freshly parsed ones with new UUIDs, so an id-keyed assignment would
 * silently disappear on the next update.
 */
@Entity(tableName = "node_group_members", indices = [Index("groupId")])
data class NodeGroupMemberEntity(
    @PrimaryKey
    val nodeKey: String,
    val groupId: String
)

/**
 * Identity of a node that survives whatever rewrites its row.
 *
 * Subscription nodes are re-imported from the provider payload, so their row id is
 * new every refresh; what stays the same is the config link, and that is their key.
 * Manual nodes are never re-imported, but editing one rewrites its link, so those key
 * on the row id, which an edit preserves.
 *
 * Two subscription entries carrying the same link therefore share one key and one
 * group. They are the same server config, so that is the intended reading.
 */
fun nodeGroupKey(id: String, subscriptionId: String?, link: String): String {
    val trimmedLink = link.trim()
    return if (subscriptionId.isNullOrBlank() || trimmedLink.isEmpty()) {
        "id:$id"
    } else {
        "sub:$subscriptionId|$trimmedLink"
    }
}

fun NodeEntity.groupKey(): String = nodeGroupKey(id, subscriptionId, link)
