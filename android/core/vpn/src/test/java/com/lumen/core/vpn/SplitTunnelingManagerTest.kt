package com.lumen.core.vpn

import android.content.pm.PackageManager
import android.net.VpnService
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import org.mockito.kotlin.mock

class SplitTunnelingManagerTest {

    @Test
    fun `applySplitTunneling in DISABLED mode makes no builder calls`() {
        val builder = mock<VpnService.Builder>()
        val config = SplitTunnelingConfig(
            mode = SplitTunnelingMode.DISABLED,
            packages = setOf("com.example.app1", "com.example.app2")
        )

        val applied = SplitTunnelingManager.applySplitTunneling(builder, config)

        assertTrue(applied.isEmpty())
        verify(builder, never()).addAllowedApplication(org.mockito.kotlin.any())
        verify(builder, never()).addDisallowedApplication(org.mockito.kotlin.any())
    }

    @Test
    fun `applySplitTunneling in ALLOW_LIST mode adds allowed applications`() {
        val builder = mock<VpnService.Builder>()
        val packages = setOf("com.example.app1", "com.example.app2")
        val config = SplitTunnelingConfig(
            mode = SplitTunnelingMode.ALLOW_LIST,
            packages = packages
        )

        val applied = SplitTunnelingManager.applySplitTunneling(builder, config)

        assertEquals(2, applied.size)
        assertTrue(applied.containsAll(packages))
        verify(builder).addAllowedApplication("com.example.app1")
        verify(builder).addAllowedApplication("com.example.app2")
        verify(builder, never()).addDisallowedApplication(org.mockito.kotlin.any())
    }

    @Test
    fun `applySplitTunneling in DISALLOW_LIST mode adds disallowed applications`() {
        val builder = mock<VpnService.Builder>()
        val packages = setOf("com.example.app1", "com.example.app2")
        val config = SplitTunnelingConfig(
            mode = SplitTunnelingMode.DISALLOW_LIST,
            packages = packages
        )

        val applied = SplitTunnelingManager.applySplitTunneling(builder, config)

        assertEquals(2, applied.size)
        assertTrue(applied.containsAll(packages))
        verify(builder).addDisallowedApplication("com.example.app1")
        verify(builder).addDisallowedApplication("com.example.app2")
        verify(builder, never()).addAllowedApplication(org.mockito.kotlin.any())
    }

    @Test
    fun `applySplitTunneling handles NameNotFoundException gracefully`() {
        val builder = mock<VpnService.Builder>()
        val validApp = "com.example.valid"
        val missingApp = "com.example.missing"
        val packages = setOf(validApp, missingApp)
        val config = SplitTunnelingConfig(
            mode = SplitTunnelingMode.ALLOW_LIST,
            packages = packages
        )

        val exception = mock<PackageManager.NameNotFoundException>()
        doThrow(exception)
            .`when`(builder).addAllowedApplication(missingApp)


        val applied = SplitTunnelingManager.applySplitTunneling(builder, config)

        assertEquals(1, applied.size)
        assertEquals(validApp, applied[0])
        verify(builder).addAllowedApplication(validApp)
        verify(builder).addAllowedApplication(missingApp)
    }

    @Test
    fun `an allow-list that applied at least one package keeps us out implicitly`() {
        assertFalse(
            SplitTunnelingManager.requiresSelfDisallow(
                SplitTunnelingMode.ALLOW_LIST,
                listOf("com.example.app1")
            )
        )
    }

    @Test
    fun `an allow-list whose packages are all uninstalled still disallows our own package`() {
        assertTrue(
            SplitTunnelingManager.requiresSelfDisallow(SplitTunnelingMode.ALLOW_LIST, emptyList())
        )
    }

    @Test
    fun `DISABLED and DISALLOW_LIST always disallow our own package`() {
        assertTrue(
            SplitTunnelingManager.requiresSelfDisallow(SplitTunnelingMode.DISABLED, emptyList())
        )
        assertTrue(
            SplitTunnelingManager.requiresSelfDisallow(
                SplitTunnelingMode.DISALLOW_LIST,
                listOf("com.example.app1")
            )
        )
    }

    @Test
    fun `an allow-list of uninstalled packages leaves the builder without any filter`() {
        val builder = mock<VpnService.Builder>()
        val missingApp = "com.example.missing"
        val config = SplitTunnelingConfig(
            mode = SplitTunnelingMode.ALLOW_LIST,
            packages = setOf(missingApp)
        )
        doThrow(mock<PackageManager.NameNotFoundException>())
            .`when`(builder).addAllowedApplication(missingApp)

        val applied = SplitTunnelingManager.applySplitTunneling(builder, config)

        assertTrue(applied.isEmpty())
        assertTrue(SplitTunnelingManager.requiresSelfDisallow(config.mode, applied))
    }
}
