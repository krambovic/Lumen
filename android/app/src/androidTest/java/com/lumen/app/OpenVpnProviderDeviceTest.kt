package com.lumen.app

import android.net.VpnService
import androidx.core.content.ContextCompat
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import com.lumen.app.vm.MainViewModel
import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.core.config.builder.SingboxConfigOptions
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.vpn.LumenVpnService
import com.lumen.core.vpn.VpnLogBus
import com.lumen.ui.screens.NodeDraft
import com.lumen.ui.screens.openVpnDraftFromProfile
import java.io.File
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Opt-in provider verification. Profile and password arrive as test arguments, never source. */
class OpenVpnProviderDeviceTest {
    @Test fun encryptedProviderProfilePassesNativeCheckAndConnects() = runBlocking {
        val arguments = InstrumentationRegistry.getArguments()
        val path = arguments.getString("providerProfilePath")
        assumeTrue("No provider fixture supplied", path != null)
        val password = arguments.getString("providerKeyPassword").orEmpty()
        val username = arguments.getString("providerUsername").orEmpty()
        val loginPassword = arguments.getString("providerPassword").orEmpty()
        val readinessOnly = arguments.getString("nativeReadinessOnly") == "true"
        val source = File(requireNotNull(path)).readText()
        val profile = com.lumen.core.config.parser.OpenVpnCredentials.persist(LinkParser.parseOpenVpnConfig(source),
            com.lumen.core.config.parser.OpenVpnSecrets(username, loginPassword, password)).link
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val node = LinkParser.parseOpenVpnConfig(profile)
        val config = File(context.cacheDir, "provider-native-check.json")
        try {
            config.writeText(SingboxConfigBuilder.buildConfig(node, SingboxConfigOptions(
                tunMode = false, localSocksPort = 10878, localHttpPort = 0, cacheFileEnabled = false)))
            val check = ProcessBuilder(File(context.applicationInfo.nativeLibraryDir, "libsingbox.so").absolutePath,
                "check", "-c", config.absolutePath).redirectErrorStream(true).start()
            assertTrue("Native config check must terminate", check.waitFor(20, TimeUnit.SECONDS))
            assertEquals("The shipped ARM core must accept the decrypted profile", 0, check.exitValue())
        } finally { config.delete() }
        assumeTrue("VPN approval must be granted before the live test", VpnService.prepare(context) == null)
        ActivityScenario.launch(MotionTestActivity::class.java).use { scenario ->
            lateinit var vm: MainViewModel
            scenario.onActivity { vm = ViewModelProvider(it)[MainViewModel::class.java] }
            val original = vm.settings.value
            val originalSelected = vm.nodes.value.firstOrNull { it.isSelected }
            val name = "OpenVPN review ${System.nanoTime()}"
            try {
                val saved = CompletableDeferred<String?>()
                scenario.onActivity {
                    vm.updateSettings(original.copy(pingType = "http", pingAttempts = 1, pingTimeoutMs = 5_000,
                        pingAutoDeleteUnreachable = false, validateProxyDataPath = false,
                        urlTestUrl = if (readinessOnly) "https://connectivity-probe.invalid/" else original.urlTestUrl))
                    vm.saveDraft(openVpnDraftFromProfile(NodeDraft(name = name, protocol = "openvpn"), source)
                        .copy(ovpnUsername = "", ovpnPassword = "", ovpnKeyPassword = "")) {
                        saved.complete(it)
                    }
                }
                assertNull(withTimeout(10_000) { saved.await() })
                val selected = withTimeout(10_000) {
                    while (vm.nodes.value.none { it.name == name }) delay(100)
                    vm.nodes.value.first { it.name == name }
                }
                scenario.onActivity { vm.selectNode(selected) }
                assertNull("Missing credentials must trigger the connection prompt", vm.buildStartIntent(context))
                assertNotNull(vm.openVpnCredentialRequest.value)
                val supplied = CompletableDeferred<Unit>()
                scenario.onActivity { vm.supplyOpenVpnCredentials(username, loginPassword, password) { supplied.complete(Unit) } }
                withTimeout(10_000) { supplied.await() }
                val edited = vm.draftForNode(selected)
                assertTrue("Prompt username must appear in Edit", edited.ovpnUsername == username)
                assertTrue("Prompt password must appear in Edit", edited.ovpnPassword == loginPassword)
                assertTrue("Prompt key password must appear in Edit", edited.ovpnKeyPassword == password)
                val intent = vm.buildStartIntent(context)
                assertNotNull(vm.connectError.value.orEmpty(), intent)
                VpnLogBus.clearLastError()
                scenario.onActivity { vm.markConnecting(); ContextCompat.startForegroundService(context, requireNotNull(intent)) }
                withTimeout(90_000) {
                    while (!LumenVpnService.isRunning.value && VpnLogBus.lastError.value == null) delay(200)
                }
                assertTrue(VpnLogBus.lastError.value.orEmpty(), LumenVpnService.isRunning.value)
                withTimeout(10_000) {
                    while (vm.connectionState.value != com.lumen.ui.components.ConnectionState.Connected ||
                        LumenVpnService.activeProxySession.value == null) delay(50)
                }
                if (!readinessOnly) {
                    scenario.onActivity { vm.checkConnectedPing("Unreachable", "No server") }
                    withTimeout(90_000) { while (vm.checkingConnectedPing.value) delay(100) }
                    assertTrue("Connected HTTP check must pass through the existing tunnel: ${vm.connectedPing.value}",
                        vm.connectedPing.value?.endsWith(" ms") == true)
                }
            } finally {
                File(context.filesDir, "review").mkdirs()
                File(context.filesDir, "review/provider-log.txt").writeText(
                    VpnLogBus.entries.value.joinToString("\n") { "${it.component}: ${it.message}" })
                scenario.onActivity {
                    vm.stopPing()
                    context.startService(vm.buildStopIntent(context))
                    vm.updateSettings(original)
                }
                withTimeout(15_000) { while (LumenVpnService.isRunning.value || LumenVpnService.isStarting.value) delay(100) }
                scenario.onActivity {
                    vm.nodes.value.firstOrNull { it.name == name }?.let(vm::deleteNode)
                    originalSelected?.let(vm::selectNode)
                }
            }
        }
    }
}
