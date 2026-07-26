package com.lumen.core.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import com.lumen.core.database.model.NodeGroupMemberEntity
import com.lumen.core.database.model.ServerGroupEntity
import kotlinx.coroutines.flow.Flow

@Dao
interface ServerGroupDao {
    @Query("SELECT * FROM server_groups ORDER BY createdAt ASC, name ASC")
    fun getGroups(): Flow<List<ServerGroupEntity>>

    @Query("SELECT * FROM node_group_members")
    fun getMembers(): Flow<List<NodeGroupMemberEntity>>

    @Query("SELECT * FROM node_group_members WHERE groupId = :groupId")
    suspend fun membersOf(groupId: String): List<NodeGroupMemberEntity>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertGroup(group: ServerGroupEntity)

    @Query("UPDATE server_groups SET name = :name WHERE id = :id")
    suspend fun renameGroup(id: String, name: String)

    @Query("DELETE FROM server_groups WHERE id = :id")
    suspend fun deleteGroupRow(id: String)

    @Query("DELETE FROM node_group_members WHERE groupId = :groupId")
    suspend fun clearGroupMembers(groupId: String)

    /**
     * Drops the group and its assignments. Rows in `nodes` are never touched, so every
     * server that was in the group survives and falls back to its default bucket.
     */
    @Transaction
    suspend fun deleteGroup(id: String) {
        clearGroupMembers(id)
        deleteGroupRow(id)
    }

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertMembers(members: List<NodeGroupMemberEntity>)

    @Query("DELETE FROM node_group_members WHERE nodeKey IN (:nodeKeys)")
    suspend fun deleteMembers(nodeKeys: List<String>)

    /**
     * Moves [nodeKeys] into [groupId]; a null group clears the assignment, which returns
     * those servers to their default bucket. A node is in at most one custom group, so
     * the insert replaces any previous row for the same key.
     */
    @Transaction
    suspend fun assignNodes(nodeKeys: List<String>, groupId: String?) {
        // SQLite binds at most 999 host parameters per statement.
        nodeKeys.chunked(400).forEach { chunk ->
            if (groupId == null) {
                deleteMembers(chunk)
            } else {
                insertMembers(chunk.map { NodeGroupMemberEntity(it, groupId) })
            }
        }
    }
}
