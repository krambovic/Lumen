package com.lumen.core.vpn

internal enum class TrafficCounterSource { TUNNEL, APP_UID }

internal data class TrafficCounters(
    val source: TrafficCounterSource,
    val uploaded: Long,
    val downloaded: Long
)

internal data class TrafficRate(
    val uploadedDelta: Long,
    val downloadedDelta: Long,
    val uploadBytesPerSecond: Long,
    val downloadBytesPerSecond: Long
)

/**
 * The bundled hev JNI returns txPackets, txBytes, rxPackets, rxBytes. `tx` is
 * read from Android's TUN (upload) and `rx` is written back to it (download).
 */
internal fun countersFromHev(stats: LongArray?): TrafficCounters? {
    if (stats == null || stats.size < 4 || stats[1] < 0L || stats[3] < 0L) return null
    return TrafficCounters(TrafficCounterSource.TUNNEL, stats[1], stats[3])
}

internal fun trafficRate(
    previous: TrafficCounters,
    current: TrafficCounters,
    elapsedNanos: Long
): TrafficRate {
    if (previous.source != current.source || elapsedNanos <= 0L) return TrafficRate(0L, 0L, 0L, 0L)
    val uploadDelta = (current.uploaded - previous.uploaded).coerceAtLeast(0L)
    val downloadDelta = (current.downloaded - previous.downloaded).coerceAtLeast(0L)
    return TrafficRate(
        uploadedDelta = uploadDelta,
        downloadedDelta = downloadDelta,
        uploadBytesPerSecond = perSecond(uploadDelta, elapsedNanos),
        downloadBytesPerSecond = perSecond(downloadDelta, elapsedNanos)
    )
}

private fun perSecond(bytes: Long, elapsedNanos: Long): Long {
    if (bytes <= 0L || elapsedNanos <= 0L) return 0L
    // Avoid overflowing `bytes * 1_000_000_000` on very long/high-volume sessions.
    return (bytes.toDouble() * 1_000_000_000.0 / elapsedNanos.toDouble())
        .coerceAtMost(Long.MAX_VALUE.toDouble())
        .toLong()
}
