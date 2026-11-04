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

class ProvidedSubscriptionDeviceTest {
    @Test fun suppliedSubscriptionUsesXrayAndPassesNativeChecksAndLiveProbes() = runBlocking {
        val path=InstrumentationRegistry.getArguments().getString("subscriptionProfilePath")
        assumeTrue(path!=null)
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val source=File(requireNotNull(path)).readText()
        val (nodes,errors)=LinkParser.parseLinksText(source)
        assertTrue(errors.toString(),errors.isEmpty());assertTrue(nodes.isNotEmpty())
        val native=File(context.applicationInfo.nativeLibraryDir,"libxray.so")
        val config=File(context.cacheDir,"subscription-native-check.json")
        val diagnostic=File(context.cacheDir,"subscription-native-check.log")
        try {
            for(node in nodes) {
                val root=JSONObject(HybridConfigBuilder.buildConfig(node,SingboxConfigOptions(tunMode=false),CoreMode.HYBRID) { ServerSocket(0).use { it.localPort } })
                assertTrue("An encrypted profile must use Xray",root.has(HybridConfigBuilder.SIDECAR_KEY))
                config.writeText(root.getJSONObject(HybridConfigBuilder.SIDECAR_KEY).toString())
                val process=ProcessBuilder(native.absolutePath,"run","-test","-c",config.absolutePath)
                    .redirectErrorStream(true).redirectOutput(diagnostic).start()
                assertTrue(process.waitFor(15,TimeUnit.SECONDS));assertEquals("Subscription native validation failed; private diagnostic saved",0,process.exitValue())
            }
        } finally { config.delete() }
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            lateinit var vm:MainViewModel
            scenario.onActivity { vm=ViewModelProvider(it)[MainViewModel::class.java] }
            val original=vm.settings.value
            val prefix="Subscription-probe-${System.nanoTime()}-"
            val imports=source.lineSequence().filter { it.startsWith("vless://") }.mapIndexed { i,line -> line.substringBefore('#')+"#$prefix$i" }.joinToString("\n")
            var targets=emptyList<com.lumen.ui.screens.NodeUiModel>()
            try {
                scenario.onActivity {
                    vm.updateSettings(original.copy(engine="HYBRID",pingType="http",pingAttempts=1,pingConcurrency=4,pingTimeoutMs=7_000,pingAutoDeleteUnreachable=false))
                    vm.prepareImportFileContent(imports)
                }
                withTimeout(10_000) { while(vm.importState.value.phase!=ImportPhaseUi.AWAITING) delay(25) }
                scenario.onActivity { vm.confirmImport() }
                targets=withTimeout(10_000) { while(vm.nodes.value.count { it.name.startsWith(prefix) }<nodes.size) delay(25);vm.nodes.value.filter { it.name.startsWith(prefix) } }
                scenario.onActivity { vm.pingNodes(targets) }
                withTimeout(150_000) { while(vm.isPinging.value) delay(100) }
                val values=vm.nodes.value.filter { it.name.startsWith(prefix) }.map { it.pingMs ?: 0 }
                File(context.filesDir,"review/subscription-probes.txt").apply { parentFile!!.mkdirs();writeText("Native checks: ${nodes.size}/${nodes.size}\nReachable: ${values.count { it>0 }}/${values.size}\nHTTP pings (ms): $values") }
                assertTrue("No supplied subscription endpoint passed HTTP; see private diagnostic",values.any { it>0 })
            } finally {
                scenario.onActivity { vm.stopPing();targets.forEach { vm.deleteNode(it) };vm.updateSettings(original);vm.dismissImport() }
            }
        }
    }
}
