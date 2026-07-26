package com.lumen.ui

import com.lumen.ui.screens.LumenStrings
import com.lumen.ui.screens.NodeUiModel
import com.lumen.ui.screens.SERVERS_SORT_PREF
import com.lumen.ui.screens.ServerSort
import com.lumen.ui.screens.serverSortComparator
import com.lumen.ui.screens.serverSortLabel
import com.lumen.ui.screens.serverSortOf
import com.lumen.ui.screens.serverSortOptions
import com.lumen.ui.screens.stringsForLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The dashboard and the Servers tab share one comparator, one label list and one
 * persisted key. These guard that they cannot drift apart again.
 */
class ServerSortTest {

    private fun node(
        name: String,
        ping: Int? = null,
        country: String = "",
        protocol: String = "vless"
    ) = NodeUiModel(
        id = name,
        name = name,
        protocol = protocol,
        server = "1.2.3.4",
        port = 443,
        pingMs = ping,
        countryCode = country,
        displayProtocol = protocol.uppercase()
    )

    @Test
    fun bothScreensPersistUnderTheServersTabKey() {
        assertEquals("servers_last_sort", SERVERS_SORT_PREF)
    }

    @Test
    fun everySortHasAMenuEntryInEveryLanguage() {
        listOf("en", "ru", "zh", "fa").forEach { language ->
            val strings: LumenStrings = stringsForLanguage(language)
            val options = serverSortOptions(strings)
            assertEquals(ServerSort.values().toList(), options.map { it.first })
            options.forEach { (sort, label) ->
                assertTrue("$language is missing a label for $sort", label.isNotBlank())
                assertEquals(label, serverSortLabel(sort, strings))
            }
        }
    }

    @Test
    fun unknownStoredValueFallsBackToDefault() {
        assertEquals(ServerSort.DEFAULT, serverSortOf("NOT_A_SORT"))
        assertEquals(ServerSort.PROTOCOL, serverSortOf("PROTOCOL"))
    }

    @Test
    fun comparatorsOrderAsTheMenuPromises() {
        val nodes = listOf(
            node("beta", ping = 40, country = "NL", protocol = "trojan"),
            node("alpha", ping = 0, country = "DE", protocol = "vless"),
            node("gamma", ping = 10, country = "US", protocol = "shadowsocks")
        )

        assertEquals(
            listOf("alpha", "beta", "gamma"),
            nodes.sortedWith(serverSortComparator(ServerSort.NAME)).map { it.name }
        )
        assertEquals(
            listOf("alpha", "beta", "gamma"),
            nodes.sortedWith(serverSortComparator(ServerSort.DEFAULT)).map { it.name }
        )
        assertEquals(
            listOf("gamma", "beta", "alpha"),
            nodes.sortedWith(serverSortComparator(ServerSort.NAME_DESC)).map { it.name }
        )
        // A stored 0 means the last probe failed, so it sorts last rather than first.
        assertEquals(
            listOf("gamma", "beta", "alpha"),
            nodes.sortedWith(serverSortComparator(ServerSort.PING)).map { it.name }
        )
        assertEquals(
            listOf("alpha", "beta", "gamma"),
            nodes.sortedWith(serverSortComparator(ServerSort.COUNTRY)).map { it.name }
        )
        assertEquals(
            listOf("gamma", "beta", "alpha"),
            nodes.sortedWith(serverSortComparator(ServerSort.PROTOCOL)).map { it.name }
        )
    }
}
