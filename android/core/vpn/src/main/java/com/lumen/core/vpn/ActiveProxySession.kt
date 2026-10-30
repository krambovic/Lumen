package com.lumen.core.vpn

/** Actual running inbound, including automatic port fallback. Never log credentials. */
data class ActiveProxySession(val port: Int, val username: String?, val password: String?, val generation: Long) {
    override fun toString(): String = "ActiveProxySession(port=$port, authenticated=${username != null}, generation=$generation)"
}
