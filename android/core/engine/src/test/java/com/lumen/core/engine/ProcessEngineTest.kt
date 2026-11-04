package com.lumen.core.engine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProcessEngineTest {

    @Test fun `native readiness only accepts the current process token and exact tag framing`() {
        assertEquals("proxy", ProcessEngine.parseOpenVpnReadyTag("LUMEN_OPENVPN_READY session cHJveHk", "session"))
        assertEquals(null, ProcessEngine.parseOpenVpnReadyTag("LUMEN_OPENVPN_READY old cHJveHk", "session"))
        assertEquals(null, ProcessEngine.parseOpenVpnReadyTag("ERROR LUMEN_OPENVPN_READY session cHJveHk", "session"))
        assertEquals(null, ProcessEngine.parseOpenVpnReadyTag("LUMEN_OPENVPN_READY session !!!", "session"))
        assertEquals(null, ProcessEngine.parseOpenVpnReadyTag("LUMEN_OPENVPN_READY session cHJveHk extra", "session"))
        assertEquals(null, ProcessEngine.parseOpenVpnReadyTag("LUMEN_OPENVPN_READY session Cg", "session"))
    }

    @Test fun `endpoint readiness noise does not replace a terminal OpenVPN error`() {
        assertTrue(ProcessEngine.isTerminalOpenVpnFailure(
            "ERROR endpoint/openvpn-client[proxy]: client terminated: authentication failed"))
        assertFalse(ProcessEngine.isTerminalOpenVpnFailure(
            "ERROR connection: outbound/openvpn-client[proxy]: endpoint is not ready yet"))
        assertFalse(ProcessEngine.isTerminalOpenVpnFailure("ERROR connection: dial timeout"))
    }

    /** Verbatim from the core, minus the colour escapes it wraps FATAL in. */
    private val bindConflictWindows =
        "FATAL[0000] start service: start inbound/socks[socks-in]: " +
            "listen tcp 127.0.0.1:10808: bind: Only one usage of each socket address " +
            "(protocol/network address/port) is normally permitted."
    private val bindConflictLinux =
        "FATAL[0000] start service: start inbound/socks[socks-in]: " +
            "listen tcp 127.0.0.1:10808: bind: address already in use"

    @Test
    fun `a lost port race is recognised on both platforms`() {
        assertTrue(ProcessEngine.isAddressInUse(bindConflictLinux))
        assertTrue(ProcessEngine.isAddressInUse(bindConflictWindows))
        // The wrapper the service reports also has to classify.
        assertTrue(
            ProcessEngine.isAddressInUse(
                "Connection failed: SINGBOX exited during startup: $bindConflictLinux"
            )
        )
    }

    @Test
    fun `an unrelated startup failure is not retried as a port conflict`() {
        assertFalse(ProcessEngine.isAddressInUse(null))
        assertFalse(ProcessEngine.isAddressInUse(""))
        assertFalse(
            ProcessEngine.isAddressInUse(
                "FATAL[0000] start service: dependency[proxy-28] not found for outbound[proxy]"
            )
        )
        assertFalse(ProcessEngine.isAddressInUse("Proxy core did not open local SOCKS5 port 10808"))
    }

    @Test
    fun `core output is stripped of the colours it emits through a pipe`() {
        val coloured = "\u001B[31mFATAL\u001B[0m[0000] start service: bind: address already in use"
        assertEquals(
            "FATAL[0000] start service: bind: address already in use",
            ProcessEngine.stripAnsi(coloured)
        )
        // A plain line must survive untouched, brackets included.
        assertEquals(
            "2026-01-01 [DEBUG] router: match",
            ProcessEngine.stripAnsi("2026-01-01 [DEBUG] router: match")
        )
    }
}
