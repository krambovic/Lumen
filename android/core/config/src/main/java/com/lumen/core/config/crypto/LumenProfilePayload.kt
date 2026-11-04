package com.lumen.core.config.crypto

import com.lumen.core.config.parser.LinkParser
import com.lumen.core.config.parser.ParsedNode
import org.json.JSONArray
import org.json.JSONObject

/** Lossless snapshot for .lumen exports, independent of share-link limitations. */
object LumenProfilePayload {
    private const val FORMAT = "lumen-node-bundle"
    private val wholeProfiles = setOf("auto", "singbox_config", "xray_config")

    fun encode(nodes: List<ParsedNode>): String {
        require(nodes.size in 1..LinkParser.MAX_IMPORT_NODES)
        val entries = nodes.map { node ->
            require(!node.isProtected) { "Protected profiles cannot be exported" }
            validate(node)
            JSONObject().put("name", node.name).put("scheme", node.scheme)
                .put("server", node.server).put("port", node.port).put("link", node.link)
                .put("outbound", JSONObject(node.outbound)).put("description", node.description)
        }
        return JSONObject().put("format", FORMAT).put("version", 1).put("nodes", JSONArray(entries))
            .toString().also { require(it.toByteArray(Charsets.UTF_8).size <= LinkParser.MAX_IMPORT_BYTES) }
    }

    fun decode(config: String): List<ParsedNode> {
        require(config.length <= LinkParser.MAX_IMPORT_BYTES &&
            config.toByteArray(Charsets.UTF_8).size <= LinkParser.MAX_IMPORT_BYTES)
        val root = if (config.trimStart().startsWith("{")) JSONObject(config) else null
        if (root?.optString("format") != FORMAT) {
            // Compatibility with v1 and early v2 exports containing ordinary links/configs.
            val (nodes, errors) = LinkParser.parseLinksText(config)
            require(nodes.isNotEmpty() && errors.isEmpty()) { "Unsupported profile configuration" }
            return nodes.onEach(::validate)
        }
        require(root.getInt("version") == 1) { "Unsupported profile payload version" }
        val entries = root.getJSONArray("nodes")
        require(entries.length() in 1..LinkParser.MAX_IMPORT_NODES)
        return (0 until entries.length()).map { index ->
            val item = entries.getJSONObject(index)
            ParsedNode(item.getString("name"), item.getString("scheme"), item.getString("server"),
                item.getInt("port"), item.getString("link"),
                LinkParser.jsonToMap(item.getJSONObject("outbound")), item.optString("description"))
                .also(::validate)
        }
    }

    fun isValidForImport(node: ParsedNode): Boolean = runCatching { validate(node) }.isSuccess

    private fun validate(node: ParsedNode) {
        require(node.name.length <= 512 && node.server.length <= 512)
        require(node.scheme.isNotBlank() && node.scheme.length <= 32)
        require(node.scheme in wholeProfiles || (node.server.isNotBlank() && node.port in 1..65535))
        require(node.link.isNotBlank() && !node.link.startsWith(ProtectedProfileCodec.PREFIX))
        require(node.link.length <= LinkParser.MAX_IMPORT_BYTES)
    }
}
