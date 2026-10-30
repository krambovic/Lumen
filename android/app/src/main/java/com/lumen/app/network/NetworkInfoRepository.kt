package com.lumen.app.network

import com.lumen.app.subscription.SubscriptionHttp
import com.lumen.core.config.NetworkInfoEndpoints
import com.lumen.core.vpn.ActiveProxySession
import com.lumen.ui.screens.NetworkInfoSnapshot
import java.io.IOException
import java.net.URL
import java.util.concurrent.CancellationException
import org.json.JSONObject

internal class NetworkInfoRepository(
    private val fetch: (String, ActiveProxySession?, () -> Boolean) -> String = { url, session, cancelled ->
        val response = SubscriptionHttp.get(URL(url), mapOf("Accept" to "application/json",
            "User-Agent" to "Lumen/Android IP-check"), session?.port, session?.username, session?.password,
            cancelled, timeoutMs = 12_000)
        if (response.code != 200 || response.body.size > 131_072) throw IOException("IP lookup unavailable")
        response.body.toString(Charsets.UTF_8)
    }
) {
    fun lookup(session: ActiveProxySession?, cancelled: () -> Boolean): NetworkInfoSnapshot {
        for ((index, url) in NetworkInfoEndpoints.urls.withIndex()) {
            if (cancelled()) throw CancellationException()
            try {
                val result = parseNetworkInfo(fetch(url, session, cancelled), index == 0)
                if (cancelled()) throw CancellationException()
                return result
            } catch (e: CancellationException) { throw e }
            catch (_: IOException) { /* Try another provider over the SAME route. */ }
            catch (_: org.json.JSONException) { /* A malformed provider response is not a valid IP. */ }
        }
        throw IOException("No IP information provider answered")
    }
}

internal fun parseNetworkInfo(raw: String, ipQuery: Boolean): NetworkInfoSnapshot {
    val json = JSONObject(raw)
    if (json.has("success") && !json.optBoolean("success")) throw IOException("IP lookup rejected")
    fun JSONObject.text(key: String): String? = optString(key).takeIf { it.isNotBlank() && it != "null" }?.take(512)
    fun JSONObject.number(key: String, range: ClosedFloatingPointRange<Double>): Double? =
        optDouble(key, Double.NaN).takeIf { it.isFinite() && it in range }
    fun JSONObject.flag(key: String): Boolean? = if (has(key) && !isNull(key) && opt(key) is Boolean) getBoolean(key) else null
    val ip = json.text("ip") ?: throw IOException("Missing IP")
    val valid = if (':' in ip && ip.matches(Regex("[0-9a-fA-F:.]+")))
        runCatching { java.net.InetAddress.getByName(ip) }.isSuccess
    else ip.split('.').let { parts -> parts.size == 4 && parts.all { it.isNotEmpty() && it.all(Char::isDigit) && it.toIntOrNull() in 0..255 } }
    if (!valid) throw IOException("Invalid IP")
    val location = if (ipQuery) json.optJSONObject("location") ?: JSONObject() else json
    val connection = json.optJSONObject(if (ipQuery) "isp" else "connection") ?: JSONObject()
    val risk = json.optJSONObject(if (ipQuery) "risk" else "security") ?: JSONObject()
    val asn = connection.text("asn")?.let { if (it.startsWith("AS", true)) it.uppercase() else "AS$it" }
    return NetworkInfoSnapshot(ip = ip, country = location.text("country"), countryCode = location.text("country_code"),
        region = location.text(if (ipQuery) "state" else "region"), city = location.text("city"),
        postal = location.text(if (ipQuery) "zipcode" else "postal"),
        latitude = location.number("latitude", -90.0..90.0), longitude = location.number("longitude", -180.0..180.0),
        timezone = if (ipQuery) location.text("timezone") else json.optJSONObject("timezone")?.text("id"),
        isp = connection.text("isp"), organization = connection.text("org"), asn = asn,
        vpn = risk.flag(if (ipQuery) "is_vpn" else "vpn"), proxy = risk.flag(if (ipQuery) "is_proxy" else "proxy"),
        tor = risk.flag(if (ipQuery) "is_tor" else "tor"), datacenter = risk.flag(if (ipQuery) "is_datacenter" else "hosting"),
        mobile = risk.flag(if (ipQuery) "is_mobile" else "mobile"),
        source = if (ipQuery) "IPQuery" else "ipwho.is", sourceUrl = if (ipQuery) "https://ipquery.io/" else "https://ipwhois.io/")
}
