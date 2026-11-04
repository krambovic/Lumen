package com.lumen.app

import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import com.lumen.app.vm.MainViewModel
import com.lumen.core.config.builder.*
import com.lumen.core.config.parser.LinkParser
import com.lumen.ui.screens.ImportPhaseUi
import java.io.File
import java.net.ServerSocket
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** The provider link stays outside the repository and test reports. */
class ProvidedVlessDeviceTest {
    @Test fun suppliedAdvancedLinkPassesNativeCheckAndHttpProbe() = runBlocking {
        val path = InstrumentationRegistry.getArguments().getString("vlessProfilePath")
        assumeTrue(path != null)
        val source = File(requireNotNull(path)).readText().lineSequence().first { it.startsWith("vless://") }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val parsed = LinkParser.parseSingle(source)
        assertTrue(HybridConfigBuilder.needsXray(parsed))
        val root = JSONObject(HybridConfigBuilder.buildConfig(parsed, SingboxConfigOptions(tunMode=false),
            CoreMode.HYBRID) { ServerSocket(0).use { it.localPort } })
        val checkFile = File(context.cacheDir, "vless-native-check.json")
        try {
            checkFile.writeText(root.getJSONObject(HybridConfigBuilder.SIDECAR_KEY).toString())
            val output = File(context.cacheDir,"vless-native-check.log")
            val check = ProcessBuilder(File(context.applicationInfo.nativeLibraryDir,"libxray.so").absolutePath,
                "run","-test","-c",checkFile.absolutePath).redirectErrorStream(true).redirectOutput(output).start()
            assertTrue(check.waitFor(15,TimeUnit.SECONDS))
            assertEquals("Native Xray rejected the supplied profile; see private device diagnostic",0,check.exitValue())
        } finally { checkFile.delete() }
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            lateinit var vm: MainViewModel
            scenario.onActivity { vm=ViewModelProvider(it)[MainViewModel::class.java] }
            val original=vm.settings.value
            val label="Xray-provider-test-${System.nanoTime()}"
            scenario.onActivity {
                vm.updateSettings(original.copy(engine="HYBRID",pingType="http",pingAttempts=1,pingTimeoutMs=10_000))
                vm.prepareImportFileContent(source.substringBefore('#')+"#"+java.net.URLEncoder.encode(label,"UTF-8"))
            }
            withTimeout(10_000) { while(vm.importState.value.phase!=ImportPhaseUi.AWAITING) delay(20) }
            scenario.onActivity { vm.confirmImport() }
            val node=withTimeout(10_000) { while(vm.nodes.value.none { it.name==label }) delay(20); vm.nodes.value.first { it.name==label } }
            try {
                scenario.onActivity { vm.pingNode(node) }
                withTimeout(70_000) { while(vm.nodes.value.first { it.id==node.id }.pingMs==null) delay(100) }
                val ping=vm.nodes.value.first { it.id==node.id }.pingMs
                File(context.filesDir,"review/vless-probe.txt").apply { parentFile!!.mkdirs();writeText("HTTP ping: $ping ms") }
                assertTrue("Provider did not pass HTTP through Xray", ping!=null && ping>0)
            } finally { scenario.onActivity { vm.stopPing();vm.deleteNode(node);vm.updateSettings(original);vm.dismissImport() } }
        }
    }
}
