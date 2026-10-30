package com.lumen.app.vm

import java.net.InetAddress
import java.io.IOException
import org.junit.Assert.*
import org.junit.Test

class TrustedEndpointResolverTest {
    private val publicIp = InetAddress.getByName("93.184.216.34")
    private val sinkhole = InetAddress.getByName("10.10.34.35")

    @Test fun regionalBootstrapRetainsWorkingCarrierEndpointInsteadOfGlobalCdnAnswer() {
        assertEquals(listOf(publicIp), TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { error("working carrier answer must not be overridden") },
            system = { listOf(publicIp) }, preferSystem = true))
    }

    @Test fun regionalBootstrapStillRejectsSinkholesAndUsesAuthenticatedFallback() {
        assertEquals(listOf(publicIp), TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { listOf(publicIp) }, system = { listOf(sinkhole) }, preferSystem = true))
        assertTrue(TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { throw IOException("DoH blocked") }, system = { listOf(sinkhole) },
            preferSystem = true).isEmpty())
    }

    @Test fun trustedAnswerWinsOverPoisonedCarrierDns() {
        val addresses = TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { listOf(publicIp) }, system = { error("must not ask carrier") })
        assertEquals(listOf(publicIp), addresses)
    }

    @Test fun blockedDohFallsBackToVettedSystemAddresses() {
        assertEquals(listOf(publicIp), TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { throw IOException("blocked") }, system = { listOf(sinkhole, publicIp) }))
        assertTrue(TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { emptyList() }, system = { listOf(sinkhole) }).isEmpty())
    }

    @Test fun trustedNxDomainMustNotBecomeACarrierBlockPageOrStaleAddress() {
        assertTrue(TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { throw TrustedNameNotFound() }, system = { error("must not use ISP") },
            remembered = listOf(publicIp)).isEmpty())
    }

    @Test fun vettedRememberedAddressSurvivesATemporaryResolverOutage() {
        assertEquals(listOf(publicIp), TrustedEndpointResolver.selectAddresses("vpn.example",
            trusted = { throw IOException() }, system = { throw IOException() },
            remembered = listOf(sinkhole, publicIp)))
    }

    @Test fun literalEndpointsDoNotNeedExternalDnsIncludingPrivateServers() {
        val literal = "192.168.1.3"
        assertEquals(listOf(InetAddress.getByName(literal)),
            TrustedEndpointResolver.selectAddresses(literal,
                trusted = { error("literal must not be queried") }, system = { error("literal") }))
    }

    @Test fun malformedAndTruncatedDnsPacketsCannotProducePinnedAddresses() {
        val response = byteArrayOf(0x12, 0x34, 0x83.toByte(), 0x80.toByte(), 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184.toByte(), 216.toByte(), 34)
        assertTrue(ServerAddressPinner.parseAnswers(response, response.size, 0x1234).isEmpty())
        response[2] = 0x81.toByte()
        assertEquals(listOf(publicIp), ServerAddressPinner.parseAnswers(response, response.size, 0x1234))
        assertTrue(ServerAddressPinner.parseAnswers(response, response.size - 1, 0x1234).isEmpty())
        assertTrue(ServerAddressPinner.parseAnswers(response, response.size, 0x5678).isEmpty())
    }
}
