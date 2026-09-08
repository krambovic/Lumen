package com.lumen.app.vm

import com.lumen.core.database.model.NodeEntity
import org.json.JSONArray
import org.json.JSONObject
import java.util.ArrayDeque

internal data class SubscriptionReconciliation(
    val nodes: List<NodeEntity>,
    val added: Int,
    val updated: Int,
    val removed: Int
)

/** Canonical object ordering; array order and every credential value remain significant. */
private fun canonicalSubscriptionValue(value: Any?): String = when (value) {
    null, JSONObject.NULL -> "null"
    is JSONObject -> value.keys().asSequence().toList().sorted().joinToString(",", "{", "}") {
        JSONObject.quote(it) + ":" + canonicalSubscriptionValue(value.get(it))
    }
    is JSONArray -> (0 until value.length()).joinToString(",", "[", "]") {
        canonicalSubscriptionValue(value.get(it))
    }
    is String -> JSONObject.quote(value)
    else -> value.toString()
}

private data class SubscriptionConnectionKey(val endpoint: String, val payload: String)

private fun NodeEntity.endpointKey(): String = subscriptionNodeKey(server, port, protocol, name)

private fun NodeEntity.connectionKey(): SubscriptionConnectionKey {
    val payload = if (outboundJson.isBlank()) link else runCatching {
        val obj = JSONObject(outboundJson)
        // A simple outbound's tag is its display/routing label, not a credential.
        // Do not strip tags from a full routing graph or its nested outbounds.
        if (!obj.has("outbounds") && !obj.has("endpoints")) obj.remove("tag")
        canonicalSubscriptionValue(obj)
    }.getOrDefault(outboundJson)
    return SubscriptionConnectionKey(endpointKey(), payload)
}

/**
 * Match one-to-one, never associateBy(endpoint) and reuse its single row id.
 * Several credentials/transports can share one server:port. REPLACE inserts with
 * that shared id used to collapse hundreds of otherwise valid imported rows.
 */
internal fun reconcileSubscriptionNodes(
    previous: List<NodeEntity>,
    incoming: List<NodeEntity>
): SubscriptionReconciliation {
    val byConnection = previous.groupBy { it.connectionKey() }
        .mapValues { (_, nodes) -> ArrayDeque(nodes) }
    val usedIds = mutableSetOf<String>()
    val matches = arrayOfNulls<NodeEntity>(incoming.size)
    // Reserve exact matches before a rotated credential can consume another node's id.
    incoming.forEachIndexed { index, node ->
        byConnection[node.connectionKey()]?.pollFirst()?.let {
            matches[index] = it
            usedIds += it.id
        }
    }
    val byEndpoint = previous.filter { it.id !in usedIds }.groupBy { it.endpointKey() }
        .mapValues { (_, nodes) -> ArrayDeque(nodes) }
    incoming.forEachIndexed { index, node ->
        if (matches[index] == null) byEndpoint[node.endpointKey()]?.pollFirst()?.let {
            matches[index] = it
            usedIds += it.id
        }
    }
    var updated = 0
    val nodes = incoming.mapIndexed { index, fresh ->
        val old = matches[index] ?: return@mapIndexed fresh
        if (fresh.name != old.name || fresh.link != old.link || fresh.outboundJson != old.outboundJson) updated++
        fresh.copy(id = old.id, pingMs = old.pingMs)
    }
    check(nodes.map { it.id }.toSet().size == nodes.size) { "Subscription row identity collision" }
    return SubscriptionReconciliation(
        nodes = nodes,
        added = matches.count { it == null },
        updated = updated,
        removed = previous.count { it.id !in usedIds }
    )
}
