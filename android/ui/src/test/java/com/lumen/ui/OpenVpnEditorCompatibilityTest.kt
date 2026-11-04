package com.lumen.ui

import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.openVpnDraftFromProfile
import com.lumen.ui.screens.openVpnProfileFromDraft
import org.junit.Assert.*
import org.junit.Test

class OpenVpnEditorCompatibilityTest {
    private fun profile(options: String) = """
        client
        dev tun
        remote vpn.example.org 1194 udp
        cipher AES-256-CBC
        $options
        <ca>
        CA
        </ca>
        <key>
        KEY
        </key>
    """.trimIndent()

    @Test fun enteringKeyPasswordDoesNotRestrictNegotiationToCbc() {
        val draft = openVpnDraftFromProfile(NodeDraft(protocol = "openvpn"), profile("keepalive 10 20"))
        val edited = openVpnProfileFromDraft(draft.copy(ovpnKeyPassword = "test passphrase"))
        assertTrue(edited.contains("cipher AES-256-CBC"))
        assertFalse(edited.contains("data-ciphers AES-256-CBC"))
        assertTrue(edited.contains("ping 10"))
        assertTrue(edited.contains("ping-restart 20"))
        assertTrue(edited.contains("<askpass>\ntest passphrase\n</askpass>"))
    }

    @Test fun editingKeepsNegotiatedCiphersFallbackAndTlsRestrictions() {
        val original = profile("data-ciphers AES-256-GCM:AES-128-GCM\ndata-ciphers-fallback AES-128-CBC\ntls-version-min 1.2")
        val draft = openVpnDraftFromProfile(NodeDraft(protocol = "openvpn"), original)
        val edited = openVpnProfileFromDraft(draft.copy(name = "Renamed"))
        assertEquals("AES-256-CBC", draft.ovpnCipher)
        assertTrue(edited.contains("data-ciphers AES-256-GCM:AES-128-GCM"))
        assertTrue(edited.contains("data-ciphers-fallback AES-128-CBC"))
        assertTrue(edited.contains("tls-version-min 1.2"))
    }
}
