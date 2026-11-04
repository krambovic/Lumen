package com.lumen.core.vpn

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.SystemClock
import com.lumen.core.config.crypto.LumenProfilePolicy
import java.io.File
import org.json.JSONObject

/** Applied to app, tile and widget starts, and periodically to a running session. */
object ProfilePolicyEnforcer {
    const val CONFIG_KEY = "_lumen_profile_policy"
    const val PREFS_NAME = "protected_profile_policy"
    private val processStartWall = System.currentTimeMillis()
    private val processStartElapsed = SystemClock.elapsedRealtime()

    fun effectiveNow(context: Context): Long {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val previous = prefs.getLong("last_seen_time", 0L)
        val now = maxOf(System.currentTimeMillis(), previous,
            processStartWall + (SystemClock.elapsedRealtime() - processStartElapsed).coerceAtLeast(0))
        if (now - previous >= 60_000) prefs.edit().putLong("last_seen_time", now).apply()
        return now
    }

    fun check(context: Context, metadata: JSONObject): LumenProfilePolicy {
        val policy = LumenProfilePolicy.fromJson(metadata)
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val caps = manager?.activeNetwork?.let(manager::getNetworkCapabilities)
        val physical = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN) == true
        policy.requireUsable(effectiveNow(context),
            wifi = physical && caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true,
            unmetered = physical && caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true)
        return policy
    }

    fun noteExpired(context: Context, metadata: JSONObject) {
        val hash = metadata.optString("storeHash")
        if (!hash.matches(Regex("[0-9a-f]{64}"))) return
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        prefs.edit().putLong("expiry_$hash", 1L).apply()
        File(File(context.noBackupFilesDir, "protected-profiles"), "$hash.lumen").delete()
    }
}
