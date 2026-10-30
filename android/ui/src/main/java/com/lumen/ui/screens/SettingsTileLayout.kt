package com.lumen.ui.screens

/** Stable storage IDs: changing a label or locale must not change the user's layout. */
internal enum class SettingsTileId(val storageId: String) {
    APPEARANCE("appearance"), ROUTING("routing"), DNS("dns"), PING("ping"),
    SUBSCRIPTIONS("subscriptions"), APP("app"), TRAFFIC("traffic"), LOGS("logs"), UPDATES("updates"), COMMUNITY("community")
}

internal enum class SettingsTileSize(val columns: Int, val rows: Int) {
    SMALL(1, 1), WIDE(2, 1), TALL(1, 2), FULL(2, 2);

    fun next(): SettingsTileSize = entries[(ordinal + 1) % entries.size]
}

internal data class SettingsTileSpec(val id: SettingsTileId, val size: SettingsTileSize)
internal data class SettingsTilePlacement(val tile: SettingsTileSpec, val column: Int, val row: Int)

internal fun defaultSettingsTiles(): List<SettingsTileSpec> = listOf(
    SettingsTileSpec(SettingsTileId.APPEARANCE, SettingsTileSize.WIDE),
    SettingsTileSpec(SettingsTileId.ROUTING, SettingsTileSize.TALL),
    SettingsTileSpec(SettingsTileId.DNS, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.PING, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.SUBSCRIPTIONS, SettingsTileSize.WIDE),
    SettingsTileSpec(SettingsTileId.APP, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.TRAFFIC, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.LOGS, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.UPDATES, SettingsTileSize.SMALL),
    SettingsTileSpec(SettingsTileId.COMMUNITY, SettingsTileSize.WIDE)
)

internal fun encodeSettingsTiles(tiles: List<SettingsTileSpec>): String =
    "v2|" + tiles.distinctBy { it.id }.joinToString(",") { "${it.id.storageId}:${it.size.name}" }

internal fun decodeSettingsTiles(value: String): List<SettingsTileSpec> {
    if (!value.startsWith("v1|") && !value.startsWith("v2|")) return defaultSettingsTiles()
    // An intentionally empty layout is valid; the edit button always remains accessible.
    if (value == "v1|" || value == "v2|") return emptyList()
    val tiles = value.substringAfter('|').split(',').mapNotNull { token ->
        val parts = token.split(':', limit = 2)
        val id = SettingsTileId.entries.find { it.storageId == parts.first() } ?: return@mapNotNull null
        val size = SettingsTileSize.entries.find { it.name == parts.getOrNull(1) }
            ?: defaultSettingsTiles().first { it.id == id }.size
        SettingsTileSpec(id, size)
    }.distinctBy { it.id }
    if (tiles.isEmpty()) return defaultSettingsTiles()
    // Add the new destination to saved layouts once. v2 then preserves an intentional hide.
    return if (value.startsWith("v1|") && tiles.none { it.id == SettingsTileId.COMMUNITY })
        tiles + SettingsTileSpec(SettingsTileId.COMMUNITY, SettingsTileSize.WIDE) else tiles
}

internal fun moveSettingsTile(tiles: List<SettingsTileSpec>, from: Int, to: Int): List<SettingsTileSpec> {
    if (from !in tiles.indices || to !in tiles.indices || from == to) return tiles
    return tiles.toMutableList().apply { add(to, removeAt(from)) }
}

/** First-fit packing retains ordering while filling spaces beside tall tiles. */
internal fun placeSettingsTiles(tiles: List<SettingsTileSpec>, columns: Int = 2): List<SettingsTilePlacement> {
    require(columns >= 2)
    val occupied = mutableSetOf<Pair<Int, Int>>()
    return tiles.map { tile ->
        var cell = 0
        while (true) {
            val row = cell / columns
            val column = cell % columns
            val fits = column + tile.size.columns <= columns &&
                (0 until tile.size.rows).all { y ->
                    (0 until tile.size.columns).all { x -> (column + x to row + y) !in occupied }
                }
            if (fits) {
                repeat(tile.size.rows) { y -> repeat(tile.size.columns) { x -> occupied += column + x to row + y } }
                return@map SettingsTilePlacement(tile, column, row)
            }
            cell++
        }
        @Suppress("UNREACHABLE_CODE") error("No grid position")
    }
}
