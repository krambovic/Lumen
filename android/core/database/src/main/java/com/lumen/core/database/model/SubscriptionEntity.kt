package com.lumen.core.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey
import java.util.UUID

@Entity(tableName = "subscriptions")
data class SubscriptionEntity(
    @PrimaryKey
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val url: String,
    val lastUpdated: Long = System.currentTimeMillis(),
    val autoUpdateEnabled: Boolean = true,
    // Provider metadata from the subscription headers, stored already decoded so the
    // card survives a restart. An empty string means "the panel has never sent it";
    // a refresh that omits a field keeps the last value it did send.
    val description: String = "",
    val announce: String = "",
    val announceUrl: String = "",
    val telegramUrl: String = "",
    val supportUrl: String = "",
    val supportEmail: String = "",
    val websiteUrl: String = "",
    val premiumUrl: String = "",
    val bannerText: String = "",
    val bannerButtonText: String = "",
    val bannerButtonUrl: String = "",
    val bannerBgColor: String = "",
    val bannerButtonColor: String = "",
    val hideUrl: Boolean = false,
    val sortOrder: String = "",
    /** `profile-update-interval`, in hours; 0 means the panel did not request one. */
    val updateIntervalHours: Int = 0
)
