package com.lumen.app

import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import com.lumen.app.profiles.ProtectedProfiles
import com.lumen.app.vm.MainViewModel
import com.lumen.core.config.builder.*
import com.lumen.core.config.crypto.ProtectedProfileCodec
import com.lumen.core.config.parser.LinkParser
import com.lumen.core.engine.EngineType
import com.lumen.core.engine.ProcessEngine
import com.lumen.ui.screens.ImportPhaseUi
import java.io.File
import java.net.ServerSocket
import java.net.Socket
import java.security.KeyPairGenerator
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class HybridProtectedDeviceTest {
    @Test fun deviceKeystoreProtectsImportedProfilesAndBlocksPlainExport() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val key = ProtectedProfileCodec.publicKey(ProtectedProfiles.recipientKey())
        val label = "Protected device test ${System.nanoTime()}"
        val source = "socks://127.0.0.1:9#Secret-endpoint"
        val envelope = ProtectedProfileCodec.seal(label, source, key)
        assertEquals("127.0.0.1", ProtectedProfiles.parse(envelope, context).first.single().server)
        val wrong = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
        assertTrue(runCatching { ProtectedProfileCodec.open(envelope, wrong.private) }.isFailure)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            lateinit var vm: MainViewModel
            scenario.onActivity { vm = ViewModelProvider(it)[MainViewModel::class.java]; vm.prepareImportFileContent(envelope) }
            withTimeout(10_000) { while (vm.importState.value.phase != ImportPhaseUi.AWAITING) delay(20) }
            scenario.onActivity { vm.confirmImport() }
            val node = withTimeout(10_000) {
                while (vm.nodes.value.none { it.name == label }) delay(20)
                vm.nodes.value.first { it.name == label }
            }
            try {
                assertTrue(node.isProtected)
                assertEquals("", node.server)
                assertEquals(0, node.port)
                assertEquals("", vm.exportNodesText(setOf(node.id)))
                assertFalse(vm.exportSubscriptionText(null).contains("Secret-endpoint"))
                assertEquals("", vm.draftForNode(node).server)
                val ref = ProtectedProfiles.parse(envelope, context).first.single().link
                assertFalse(ref.contains(source))
                assertEquals("127.0.0.1", ProtectedProfiles.resolve(ref, context).server)
            } finally { scenario.onActivity { vm.deleteNode(node); vm.dismissImport() } }
        }
    }

    @Test fun xrayAndSingboxSidecarPassRealHttpAndStopIndependently() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(context.cacheDir, "hybrid-device-test").apply { mkdirs() }
        val fixture = ServerSocket(0)
        val peer = Executors.newSingleThreadExecutor()
        peer.submit {
            try { fixture.accept().use { socket ->
                socket.soTimeout = 15_000
                val input = socket.getInputStream(); val output = socket.getOutputStream()
                fun read(n: Int) { repeat(n) { check(input.read() >= 0) } }
                check(input.read() == 5); read(input.read())
                output.write(byteArrayOf(5, 0)); output.flush()
                read(3)
                when(input.read()) { 1 -> read(4); 3 -> read(input.read()); 4 -> read(16); else -> error("SOCKS address") }
                read(2); output.write(byteArrayOf(5,0,0,1,127,0,0,1,0,0)); output.flush()
                check(input.read() >= 0)
                output.write("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toByteArray()); output.flush()
            } } catch (_: Exception) { }
        }
        val port = ServerSocket(0).use { it.localPort }
        val node = LinkParser.parseSingle("socks://127.0.0.1:${fixture.localPort}#Test")
        val root = JSONObject(HybridConfigBuilder.buildConfig(node, SingboxConfigOptions(tunMode=false,
            localSocksPort=port, localHttpPort=0, logLevel="warn"), CoreMode.XRAY) { ServerSocket(0).use { it.localPort } })
        val xrayJson = root.getJSONObject(HybridConfigBuilder.SIDECAR_KEY).toString()
        root.remove(HybridConfigBuilder.SIDECAR_KEY)
        val native = File(context.applicationInfo.nativeLibraryDir)
        val xray = ProcessEngine(EngineType.XRAY, File(native,"libxray.so"), File(dir,"xray"))
        val singbox = ProcessEngine(EngineType.SINGBOX, File(native,"libsingbox.so"), File(dir,"singbox"))
        try {
            xray.start(xrayJson)
            singbox.start(root.toString())
            assertTrue(xray.isRunning && singbox.isRunning)
            Socket("127.0.0.1", port).use { socket ->
                socket.soTimeout = 15_000
                val input=socket.getInputStream();val output=socket.getOutputStream()
                output.write(byteArrayOf(5,1,0));output.flush()
                assertEquals(5,input.read());assertEquals(0,input.read())
                output.write(byteArrayOf(5,1,0,1,(-58).toByte(),18,0,1,0,80));output.flush()
                assertEquals(5,input.read());assertEquals(0,input.read());repeat(8) { input.read() }
                output.write("GET / HTTP/1.1\r\nHost: test.invalid\r\nConnection: close\r\n\r\n".toByteArray());output.flush()
                assertTrue(input.bufferedReader().readLine().contains("204"))
            }
            singbox.stop()
            assertFalse(singbox.isRunning)
            assertTrue("Stopping sing-box must not reap Xray", xray.isRunning)
        } finally {
            singbox.stop();xray.stop();fixture.close();peer.shutdownNow();peer.awaitTermination(2,TimeUnit.SECONDS)
            dir.deleteRecursively()
        }
    }
}
