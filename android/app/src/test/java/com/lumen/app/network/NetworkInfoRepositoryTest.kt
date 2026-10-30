package com.lumen.app.network

import com.lumen.core.vpn.ActiveProxySession
import org.junit.Assert.*
import org.junit.Test
import java.io.IOException
import java.util.concurrent.CancellationException

class NetworkInfoRepositoryTest {
    private val query = """{"ip":"203.0.113.7","isp":{"asn":"AS204044","isp":"Example ISP","org":"Example Ltd"},"location":{"country":"United Kingdom","country_code":"GB","state":"England","city":"London","latitude":51.5,"longitude":0.1,"timezone":"Europe/London"},"risk":{"is_vpn":false,"is_proxy":false,"is_datacenter":true}}"""
    private val whois = """{"success":true,"ip":"2001:db8::1","country":"Germany","city":"Berlin","connection":{"asn":123,"isp":"Backup ISP"},"timezone":{"id":"Europe/Berlin"}}"""

    @Test fun parsesProviderLocationAndClassificationWithoutInventingMissingFields() {
        val result = parseNetworkInfo(query, true)
        assertEquals("203.0.113.7", result.ip)
        assertEquals("AS204044", result.asn)
        assertEquals("Example ISP", result.isp)
        assertEquals("England", result.region)
        assertEquals(false, result.vpn)
        assertEquals(true, result.datacenter)
        assertNull(result.mobile)
        assertNull(result.postal)
    }

    @Test fun fallbackKeepsTheExactProxyAndAuthenticationRatherThanLeakingDirect() {
        val session = ActiveProxySession(12000, "user", "secret", 42)
        var calls = 0
        val repository = NetworkInfoRepository { _, actual, _ ->
            assertSame(session, actual)
            if (++calls == 1) throw IOException("provider blocked")
            whois
        }
        val result = repository.lookup(session) { false }
        assertEquals(2, calls)
        assertEquals("AS123", result.asn)
        assertEquals("Europe/Berlin", result.timezone)
        assertNull(result.vpn)
    }

    @Test fun disconnectedLookupUsesCurrentNetwork() {
        val repository = NetworkInfoRepository { _, session, _ -> assertNull(session); query }
        assertEquals("203.0.113.7", repository.lookup(null) { false }.ip)
    }

    @Test(expected = IOException::class) fun bothProvidersFailWithoutDirectRecovery() {
        val session = ActiveProxySession(12000, null, null, 42)
        NetworkInfoRepository { _, actual, _ -> assertSame(session, actual); throw IOException() }
            .lookup(session) { false }
    }

    @Test(expected = CancellationException::class) fun cancellationDoesNotTryAnotherProvider() {
        NetworkInfoRepository { _, _, _ -> error("must not send a request") }.lookup(null) { true }
    }

    @Test(expected = IOException::class) fun rejectsFailedResponsesEvenWithAnIp() {
        parseNetworkInfo("""{"success":false,"ip":"1.1.1.1"}""", false)
    }

    @Test(expected = IOException::class) fun rejectsHostnamesInPlaceOfAnIpWithoutResolvingThem() {
        parseNetworkInfo("""{"ip":"attacker.example"}""", true)
    }

    @Test fun credentialsAreNotPartOfDiagnosticStrings() {
        assertFalse(ActiveProxySession(12000, "user", "secret", 42).toString().contains("secret"))
    }
}
