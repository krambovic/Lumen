package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * The log used to live only in a 200 entry in-memory ring, so every crash took the
 * lines that explained it. These tests pin the durable replacement: it is bounded,
 * it keeps the newest entries, and clearing it really does remove the file.
 */
class VpnLogStoreTest {

    @get:Rule
    val temp = TemporaryFolder()

    private fun store(
        retention: VpnLogRetention = VpnLogRetention(),
        directory: File = temp.newFolder()
    ) = VpnLogStore(directory, retention)

    private fun entry(index: Int, level: VpnLogLevel = VpnLogLevel.INFO) = VpnLogEntry(
        timestamp = 1_700_000_000_000L + index,
        formattedTime = "00:00:00.000",
        level = level,
        component = "TEST",
        message = "line $index"
    )

    @Test
    fun testRotationKeepsTheLogInsideTheSizeBudget() {
        val budget = VpnLogRetention.MIN_MAX_BYTES
        val store = store(VpnLogRetention(maxBytes = budget, maxEntries = VpnLogRetention.MAX_MAX_ENTRIES))
        repeat(4_000) { store.append(entry(it)) }

        // One entry of slack: the segment is only rotated once it is full.
        assertTrue("size=${store.size()}", store.size() <= budget + 256)
        val stored = store.readTail(10_000)
        assertTrue(stored.size < 4_000)
        assertEquals("line 3999", stored.last().message)
        assertFalse(stored.any { it.message == "line 0" })
    }

    @Test
    fun testRotationKeepsTheLogInsideTheEntryBudget() {
        val store = store(VpnLogRetention(maxEntries = 100))
        repeat(1_000) { store.append(entry(it)) }

        val stored = store.readTail(10_000)
        assertTrue("entries=${stored.size}", stored.size <= 100)
        assertEquals("line 999", stored.last().message)
    }

    @Test
    fun testReadTailReturnsTheNewestEntriesInOrder() {
        val store = store()
        repeat(5) { store.append(entry(it)) }

        assertEquals(
            listOf("line 2", "line 3", "line 4"),
            store.readTail(3).map { it.message }
        )
        // Paging further back for the log screen.
        assertEquals(
            listOf("line 1", "line 2"),
            store.readTail(2, skipFromEnd = 2).map { it.message }
        )
        assertTrue(store.readTail(0).isEmpty())
    }

    @Test
    fun testMultilineEntriesSurviveTheRoundTrip() {
        val store = store()
        val stackTrace = "panic: boom\n\tat com.lumen.Foo\tbar\\baz"
        store.append(entry(0).copy(level = VpnLogLevel.ERROR, message = stackTrace))

        val restored = store.readTail(10).single()
        assertEquals(stackTrace, restored.message)
        assertEquals(VpnLogLevel.ERROR, restored.level)
        assertEquals("TEST", restored.component)
    }

    @Test
    fun testClearRemovesTheFileContents() {
        val directory = temp.newFolder()
        val store = store(directory = directory)
        repeat(50) { store.append(entry(it)) }
        assertTrue(store.size() > 0L)

        store.clear()

        assertEquals(0L, store.size())
        assertTrue(store.readTail(100).isEmpty())
        assertFalse(File(directory, VpnLogStore.ACTIVE_NAME).exists())
        assertFalse(File(directory, VpnLogStore.ARCHIVE_NAME).exists())

        // The store stays usable afterwards.
        store.append(entry(99))
        assertEquals("line 99", store.readTail(10).single().message)
    }

    @Test
    fun testReopeningContinuesTheExistingSegment() {
        val directory = temp.newFolder()
        val retention = VpnLogRetention(maxEntries = 100)
        val first = VpnLogStore(directory, retention)
        repeat(40) { first.append(entry(it)) }
        first.close()

        val second = VpnLogStore(directory, retention)
        repeat(40) { second.append(entry(100 + it)) }

        val stored = second.readTail(1_000)
        assertTrue("entries=${stored.size}", stored.size <= 100)
        assertEquals("line 139", stored.last().message)
        assertEquals("line 0", stored.first().message)
    }

    @Test
    fun testExportWritesEveryStoredEntry() {
        val store = store(VpnLogRetention(maxEntries = 100))
        repeat(200) { store.append(entry(it)) }
        val target = File(temp.newFolder(), "export.txt")

        val written = store.exportTo(target)

        assertEquals(store.readTail(10_000).size.toLong(), written)
        val text = target.readText()
        assertTrue(text.contains("[INFO] [TEST] line 199"))
        assertFalse(text.contains("[INFO] [TEST] line 0\n"))
    }

    @Test
    fun testLoweredRetentionTakesEffectImmediately() {
        val store = store(VpnLogRetention(maxEntries = VpnLogRetention.MAX_MAX_ENTRIES))
        repeat(500) { store.append(entry(it)) }

        store.setRetention(VpnLogRetention(maxEntries = 100))
        store.append(entry(500))

        val stored = store.readTail(10_000)
        assertEquals("line 500", stored.last().message)
        // The segments written under the old budget no longer fit the new one.
        assertTrue("entries=${stored.size}", stored.size <= 100)
    }
}
