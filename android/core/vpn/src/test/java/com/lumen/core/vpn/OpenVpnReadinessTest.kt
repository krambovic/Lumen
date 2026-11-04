package com.lumen.core.vpn

import org.junit.Assert.*
import org.junit.Test

class OpenVpnReadinessTest {
    @Test fun selectedOpenVpnRequiresNativeReadinessWithoutAnyProbeWebsite() {
        assertEquals("proxy", openVpnStartupTag("""{"endpoints":[{"type":"openvpn-client","tag":"proxy"}]}"""))
        assertEquals("vpn", openVpnStartupTag("""{"outbounds":[{"type":"openvpn","tag":"vpn"}],"route":{"final":"vpn"}}"""))
        assertNull(openVpnStartupTag("""{"endpoints":[{"type":"wireguard","tag":"proxy"}]}"""))
        assertNull(openVpnStartupTag("{}"))
    }

    @Test fun unusedAutoPoolMembersAndDirectRoutesCannotBlockStartup() {
        assertNull(openVpnStartupTag("""{"endpoints":[{"type":"openvpn-client","tag":"proxy-0"}],"outbounds":[{"type":"urltest","tag":"proxy","outbounds":["proxy-0","proxy-1"]}],"route":{"final":"proxy"}}"""))
        assertNull(openVpnStartupTag("""{"endpoints":[{"type":"openvpn-client","tag":"proxy"}],"route":{"final":"direct"}}"""))
    }
}
