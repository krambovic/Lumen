package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class VpnLogBusTest {

    @Before
    fun setUp() {
        VpnLogBus.clear()
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
}
