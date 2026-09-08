package com.lumen.app.vm

/** Zero is an explicit opt-out, not an invalid interval to replace with a default. */
internal fun normalizedSubscriptionAutoUpdateMinutes(minutes: Int): Int =
    if (minutes <= 0) 0 else minutes.coerceIn(15, 1440)

internal fun subscriptionAutomaticIntervalMinutes(
    configuredMinutes: Int,
    subscriptionEnabled: Boolean,
    providerHours: Int
): Int? {
    val configured = normalizedSubscriptionAutoUpdateMinutes(configuredMinutes)
    if (configured == 0 || !subscriptionEnabled) return null
    return providerHours.takeIf { it in 1..8760 }?.times(60) ?: configured
}

/** A settings toggle must invalidate already fetched automatic results, even OFF -> ON. */
internal fun canApplySubscriptionRefresh(
    automatic: Boolean,
    startedEpoch: Long,
    currentEpoch: Long,
    configuredMinutes: Int,
    subscriptionEnabled: Boolean,
    requestedUrl: String,
    currentUrl: String?
): Boolean = currentUrl == requestedUrl && (!automatic || (
    startedEpoch == currentEpoch &&
        subscriptionAutomaticIntervalMinutes(configuredMinutes, subscriptionEnabled, 0) != null
    ))

internal fun canReplaceSubscriptionNodes(
    previousCount: Int, parsedCount: Int, validCount: Int, parseErrorCount: Int
): Boolean = validCount > 0 && (previousCount == 0 || (
    parseErrorCount == 0 && parsedCount == validCount
    ))
