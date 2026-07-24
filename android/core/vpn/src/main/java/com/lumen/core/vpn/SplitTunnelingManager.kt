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
}
