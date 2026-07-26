package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class VpnLogBusTest {

    @get:Rule
    val temp = TemporaryFolder()

    @Before
    fun setUp() {
        // The bus is a process singleton: give every test its own directory and
        // put the settings back to the defaults.
        VpnLogBus.updateSettings(VpnLogSettings())
        VpnLogBus.attach(temp.newFolder())
        VpnLogBus.clear()
        VpnLogBus.flushPersisted()
    }

    @Test
    fun testLastErrorResetOnClearLastError() {
        VpnLogBus.error("TEST", "Fatal crash")
        assertEquals("Fatal crash", VpnLogBus.lastError.value)

        VpnLogBus.clearLastError()
        assertNull(VpnLogBus.lastError.value)
    }

    @Test
    fun testBeginSessionClearsLastError() {
        VpnLogBus.error("TEST", "Previous connection failed")
        assertEquals("Previous connection failed", VpnLogBus.lastError.value)

        VpnLogBus.beginSession("sing-box extended")
        assertNull(VpnLogBus.lastError.value)
    }

    @Test
    fun testNonErrorLogsDoNotSetLastError() {
        VpnLogBus.info("CORE", "sing-box: connect error: connection refused")
        assertNull(VpnLogBus.lastError.value)

        VpnLogBus.warning("CORE", "sing-box: connect error: connection refused")
        assertNull(VpnLogBus.lastError.value)

        VpnLogBus.debug("CORE", "debug error message")
        assertNull(VpnLogBus.lastError.value)
    }

    @Test
    fun testLevelFiltersWhatIsStoredAndShown() {
        VpnLogBus.updateSettings(VpnLogSettings(level = VpnLogLevel.WARNING))

        VpnLogBus.debug("CORE", "chatty")
        VpnLogBus.info("CORE", "routine")
        VpnLogBus.warning("CORE", "attention")
        VpnLogBus.error("CORE", "fatal")

        assertTrue(VpnLogBus.flushPersisted())
        assertEquals(
            listOf("attention", "fatal"),
            VpnLogBus.readPersisted(50).map { it.message }
        )
        assertEquals(
            listOf("attention", "fatal"),
            VpnLogBus.entries.value.map { it.message }
        )
    }

    @Test
    fun testPersistDisabledWritesNothing() {
        VpnLogBus.updateSettings(VpnLogSettings(persist = false))

        VpnLogBus.error("CORE", "fatal")

        assertTrue(VpnLogBus.flushPersisted())
        assertEquals(0L, VpnLogBus.persistedSize())
        assertTrue(VpnLogBus.readPersisted(10).isEmpty())
        // The live view keeps working while nothing is written.
        assertEquals(listOf("fatal"), VpnLogBus.entries.value.map { it.message })
    }

    @Test
    fun testClearRemovesThePersistedLog() {
        repeat(10) { VpnLogBus.warning("CORE", "line $it") }
        assertTrue(VpnLogBus.flushPersisted())
        assertTrue(VpnLogBus.persistedSize() > 0L)

        VpnLogBus.clear()

        assertTrue(VpnLogBus.flushPersisted())
        assertEquals(0L, VpnLogBus.persistedSize())
        assertTrue(VpnLogBus.readPersisted(100).isEmpty())
        assertTrue(VpnLogBus.entries.value.isEmpty())
    }

    @Test
    fun testRetentionSettingBoundsThePersistedLog() {
        VpnLogBus.updateSettings(VpnLogSettings(retention = VpnLogRetention(maxEntries = 100)))

        repeat(300) { VpnLogBus.warning("CORE", "line $it") }

        assertTrue(VpnLogBus.flushPersisted())
        val stored = VpnLogBus.readPersisted(1_000)
        assertTrue("entries=${stored.size}", stored.size <= 100)
        assertEquals("line 299", stored.last().message)
    }

    @Test
    fun testExportTextRendersThePersistedTail() {
        VpnLogBus.error("CORE", "fatal")
        assertTrue(VpnLogBus.flushPersisted())

        val text = VpnLogBus.exportText()

        assertTrue(text, text.contains("[ERROR] [CORE] fatal"))
    }
}
