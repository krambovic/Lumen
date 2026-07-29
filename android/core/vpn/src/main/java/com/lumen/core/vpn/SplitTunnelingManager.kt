package com.lumen.core.vpn

import android.content.pm.PackageManager
import android.net.VpnService
import android.util.Log

enum class SplitTunnelingMode {
    DISABLED,
    ALLOW_LIST,
    DISALLOW_LIST
}

data class SplitTunnelingConfig(
    val mode: SplitTunnelingMode = SplitTunnelingMode.DISABLED,
    val packages: Set<String> = emptySet()
)

object SplitTunnelingManager {
    private const val TAG = "SplitTunnelingManager"

    /**
     * The VPN process must never be routed into its own TUN. Remove duplicate
     * user state in every mode; non-allow modes explicitly disallow it later.
     */
    fun withoutOwnPackage(
        config: SplitTunnelingConfig,
        ownPackage: String
    ): SplitTunnelingConfig = config.copy(packages = config.packages - ownPackage)

    /**
     * Android interprets an allow-list with zero successfully added packages as
     * "no app filter", which captures every app. Refuse that unsafe inversion.
     */
    fun requireSafeAllowList(
        config: SplitTunnelingConfig,
        appliedPackages: List<String>? = null
    ) {
        if (config.mode != SplitTunnelingMode.ALLOW_LIST) return
        require(config.packages.isNotEmpty()) {
            "Split tunneling allow-list is empty; select at least one application"
        }
        if (appliedPackages != null) {
            require(appliedPackages.isNotEmpty()) {
                "None of the allow-listed applications is installed"
            }
        }
    }

    /**
     * Applies split tunneling configuration to the provided VpnService.Builder.
     *
     * Handles PackageManager.NameNotFoundException gracefully if a package in the set
     * is not installed on the device.
     *
     * @param builder The VpnService.Builder instance
     * @param config The SplitTunnelingConfig defining mode and target packages
     * @return List of package names successfully added to the builder
     */
    fun applySplitTunneling(
        builder: VpnService.Builder,
        config: SplitTunnelingConfig
    ): List<String> {
        val appliedPackages = mutableListOf<String>()

        when (config.mode) {
            SplitTunnelingMode.DISABLED -> {
                // No per-app filtering applied; standard VPN routing
            }

            SplitTunnelingMode.ALLOW_LIST -> {
                for (pkg in config.packages) {
                    try {
                        builder.addAllowedApplication(pkg)
                        appliedPackages.add(pkg)
                    } catch (e: PackageManager.NameNotFoundException) {
                        Log.w(TAG, "Package not found for allow-list: $pkg", e)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to add allowed application: $pkg", e)
                    }
                }
            }

            SplitTunnelingMode.DISALLOW_LIST -> {
                for (pkg in config.packages) {
                    try {
                        builder.addDisallowedApplication(pkg)
                        appliedPackages.add(pkg)
                    } catch (e: PackageManager.NameNotFoundException) {
                        Log.w(TAG, "Package not found for disallow-list: $pkg", e)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to add disallowed application: $pkg", e)
                    }
                }
            }
        }

        return appliedPackages
    }

    /**
     * Our own package must never end up inside the tunnel: the core's outbound
     * sockets would be routed back into the TUN they feed. An allow-list keeps us
     * out implicitly, but only while the framework actually accepted at least one
     * package. Empty or entirely invalid allow-lists are rejected before the
     * interface is established, so they can never invert into a full-device VPN.
     *
     * @param mode The mode that was applied to the builder
     * @param applied The packages [applySplitTunneling] reported as accepted
     */
    fun requiresSelfDisallow(mode: SplitTunnelingMode, applied: List<String>): Boolean =
        mode != SplitTunnelingMode.ALLOW_LIST || applied.isEmpty()
}
