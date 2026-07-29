package com.lumen.core.vpn

/**
 * Pure policy for selecting the physical network underneath [LumenVpnService].
 *
 * Keeping this free of Android framework objects makes the handover rules testable:
 * a candidate must have validated Internet access and Android's active default must
 * win over a simultaneously available standby transport.
 */
internal object UnderlyingNetworkPolicy {

    fun isReady(
        hasInternet: Boolean,
        isValidated: Boolean,
        isVpn: Boolean
    ): Boolean = hasInternet && isValidated && !isVpn

    fun <T> select(
        active: T?,
        pending: T?,
        previous: T?,
        isReady: (T) -> Boolean
    ): T? = active?.takeIf(isReady)
        ?: pending?.takeIf(isReady)
        ?: previous?.takeIf(isReady)
}
