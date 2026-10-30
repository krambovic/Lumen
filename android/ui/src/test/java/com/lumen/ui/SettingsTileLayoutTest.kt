package com.lumen.ui

import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class SettingsTileLayoutTest {
    @Test fun oldInstallUsesCuratedLayoutAndIntentionalEmptyLayoutStaysEmpty() {
        assertEquals(defaultSettingsTiles(), decodeSettingsTiles(""))
        assertEquals(defaultSettingsTiles(), decodeSettingsTiles("unreadable"))
        assertTrue(decodeSettingsTiles(encodeSettingsTiles(emptyList())).isEmpty())
    }

    @Test fun orderSizesAndHiddenTilesSurviveStorageRoundTrip() {
        val tiles = defaultSettingsTiles().reversed().drop(2).map {
            it.copy(size = it.size.next())
        }
        assertEquals(tiles, decodeSettingsTiles(encodeSettingsTiles(tiles)))
    }

    @Test fun storageIgnoresUnknownAndDuplicateIdsWithoutLosingKnownTiles() {
        val tiles = decodeSettingsTiles("v1|unknown:SMALL,dns:TALL,dns:WIDE,app:invalid")
        assertEquals(listOf(SettingsTileId.DNS, SettingsTileId.APP, SettingsTileId.COMMUNITY), tiles.map { it.id })
        assertEquals(SettingsTileSize.TALL, tiles.first().size)
        assertEquals(SettingsTileSize.SMALL, tiles.first { it.id == SettingsTileId.APP }.size)
        assertEquals(defaultSettingsTiles(), decodeSettingsTiles("v1|unknown:WIDE"))
    }

    @Test fun communityIsAddedOnceToOldLayoutsAndCanThenBeHidden() {
        val migrated = decodeSettingsTiles("v1|dns:FULL")
        assertEquals(SettingsTileSize.FULL, migrated.first().size)
        assertEquals(SettingsTileId.COMMUNITY, migrated.last().id)
        val hidden = migrated.filterNot { it.id == SettingsTileId.COMMUNITY }
        assertEquals(hidden, decodeSettingsTiles(encodeSettingsTiles(hidden)))
    }

    @Test fun movingATilePreservesItsSizeAndEveryOtherTilesRelativeOrder() {
        val tiles = defaultSettingsTiles()
        val moved = moveSettingsTile(tiles, 0, tiles.lastIndex)
        assertEquals(tiles.first(), moved.last())
        assertEquals(tiles.drop(1), moved.dropLast(1))
        assertEquals(tiles, moveSettingsTile(moved, moved.lastIndex, 0))
        assertEquals(tiles, moveSettingsTile(tiles, -1, 0))
    }

    @Test fun mixedSizesNeverOverlapOrEscapeTheGridAfterReorderAndResize() {
        for (columns in listOf(2, 4)) {
            for (rotation in SettingsTileId.entries.indices) {
                for (size in SettingsTileSize.entries) {
                    val defaults = defaultSettingsTiles()
                    val tiles = (defaults.drop(rotation) + defaults.take(rotation)).mapIndexed { index, tile ->
                        if (index % 2 == 0) tile.copy(size = size) else tile
                    }
                    val placements = placeSettingsTiles(tiles, columns)
                    assertEquals(tiles, placements.map { it.tile })
                    val cells = mutableSetOf<Pair<Int, Int>>()
                    placements.forEach { placement ->
                        assertTrue(placement.column >= 0)
                        assertTrue(placement.column + placement.tile.size.columns <= columns)
                        repeat(placement.tile.size.rows) { y -> repeat(placement.tile.size.columns) { x ->
                            assertTrue("Overlapping tiles", cells.add(placement.column + x to placement.row + y))
                        } }
                    }
                }
            }
        }
    }

    @Test fun editorActionsAreTranslatedInEveryLanguage() {
        listOf("en", "ru", "zh", "fa").forEach { language ->
            val s = stringsForLanguage(language)
            listOf(s.settingsEditTiles, s.settingsTileEditHint, s.settingsAddTile, s.settingsHideTile,
                s.settingsResizeTile, s.settingsMoveEarlier, s.settingsMoveLater, s.settingsTileSmall,
                s.settingsTileWide, s.settingsTileTall, s.settingsAllTilesAdded, s.settingsResetTiles)
                .forEach { assertTrue(it.isNotBlank()) }
        }
    }
}
