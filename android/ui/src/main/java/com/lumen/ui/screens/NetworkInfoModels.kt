package com.lumen.ui.screens

import androidx.compose.runtime.Immutable

@Immutable
data class NetworkInfoSnapshot(
    val ip: String, val country: String? = null, val countryCode: String? = null,
    val region: String? = null, val city: String? = null, val postal: String? = null,
    val latitude: Double? = null, val longitude: Double? = null, val timezone: String? = null,
    val isp: String? = null, val organization: String? = null, val asn: String? = null,
    val vpn: Boolean? = null, val proxy: Boolean? = null, val tor: Boolean? = null,
    val datacenter: Boolean? = null, val mobile: Boolean? = null,
    val source: String, val sourceUrl: String
)

@Immutable
data class NetworkInfoUiState(
    val data: NetworkInfoSnapshot? = null, val loading: Boolean = true,
    val failed: Boolean = false, val waiting: Boolean = false,
    val throughVpn: Boolean = false, val updatedAt: Long? = null
)
