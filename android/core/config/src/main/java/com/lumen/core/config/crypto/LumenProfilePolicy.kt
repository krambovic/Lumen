package com.lumen.core.config.crypto

import org.json.JSONObject

enum class ProfilePolicyDenial { EXPIRED, CLOCK, WIFI, UNMETERED }
class ProfilePolicyException(val reason: ProfilePolicyDenial, message: String) : IllegalArgumentException(message)

/** Authenticated restrictions in v3. Time is absolute, measured from creation. */
data class LumenProfilePolicy(
    val createdAtMs: Long = 0,
    val expiresAtMs: Long = 0,
    val wifiOnly: Boolean = false,
    val unmeteredOnly: Boolean = false
) {
    val restricted: Boolean get() = expiresAtMs > 0 || wifiOnly || unmeteredOnly
    fun requireUsable(nowMs: Long, wifi: Boolean = true, unmetered: Boolean = true) {
        if (createdAtMs > 0 && nowMs < createdAtMs - 300_000)
            throw ProfilePolicyException(ProfilePolicyDenial.CLOCK, "Check the device date and time")
        if (expiresAtMs > 0 && nowMs >= expiresAtMs)
            throw ProfilePolicyException(ProfilePolicyDenial.EXPIRED, "Lumen profile has expired")
        if (wifiOnly && !wifi) throw ProfilePolicyException(ProfilePolicyDenial.WIFI, "This Lumen profile requires Wi-Fi")
        if (unmeteredOnly && !unmetered)
            throw ProfilePolicyException(ProfilePolicyDenial.UNMETERED, "This Lumen profile requires an unmetered network")
    }
    fun toJson(): JSONObject = JSONObject().put("createdAtMs", createdAtMs).put("expiresAtMs", expiresAtMs)
        .put("wifiOnly", wifiOnly).put("unmeteredOnly", unmeteredOnly)

    fun validate() {
        require(createdAtMs >= 0 && expiresAtMs >= 0)
        require(expiresAtMs == 0L || (createdAtMs > 0 && expiresAtMs > createdAtMs))
    }

    companion object {
        fun fromJson(json: JSONObject?): LumenProfilePolicy = if (json == null) LumenProfilePolicy() else
            LumenProfilePolicy(json.getLong("createdAtMs"), json.getLong("expiresAtMs"),
                json.getBoolean("wifiOnly"), json.getBoolean("unmeteredOnly")).also { it.validate() }
    }
}
