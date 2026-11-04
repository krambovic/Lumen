package com.lumen.core.vpn

/** Sampling totals less often keeps their accuracy; rates use actual elapsed time. */
internal fun trafficPollingIntervalMs(uiVisible: Boolean, interactive: Boolean, showNotificationSpeed: Boolean): Long =
    when {
        !interactive -> 10_000L
        uiVisible -> 1_000L
        showNotificationSpeed -> 5_000L
        else -> 10_000L
    }
