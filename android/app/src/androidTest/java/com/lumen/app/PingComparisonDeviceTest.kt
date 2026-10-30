package com.lumen.app

import android.os.Bundle
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import com.lumen.app.vm.MainViewModel
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Test

/** Opt-in live regression check: same stored server, same settings, single then bulk. */
class PingComparisonDeviceTest {
    @Test fun compareSingleAndBatchForTheSameServer() = runBlocking {
        ActivityScenario.launch(MotionTestActivity::class.java).use { scenario ->
            lateinit var vm: MainViewModel
            scenario.onActivity { vm = ViewModelProvider(it)[MainViewModel::class.java] }
            val collector = launch(Dispatchers.Default) { vm.nodes.collect {} }
            val original = vm.settings.value
            try {
                withTimeout(15_000) { while (vm.nodes.value.isEmpty()) delay(100) }
                // No cleanup/removal may run during this comparison, even if enabled by the user.
                vm.updateSettings(original.copy(pingAutoDeleteUnreachable = false, pingAttempts = 1))
                val candidates = vm.nodes.value.filter { !it.isAutoNode }
                val target = candidates.firstOrNull { it.isSelected } ?: candidates.first()
                val oldPing = target.pingMs
                vm.pingNode(target)
                delay(60)
                if (target.id in vm.pingingNodeIds.value) assertEquals("A manual probe must keep its sort anchor",
                    oldPing, vm.nodes.value.first { it.id == target.id }.pingMs)
                delay(200)
                withTimeout(180_000) { while (target.id in vm.pingingNodeIds.value) delay(200) }
                val single = vm.nodes.value.first { it.id == target.id }.pingMs
                val batch = (listOf(target) + candidates).distinctBy { it.id }.take(4)
                vm.pingNodes(batch)
                delay(200)
                withTimeout(240_000) { while (vm.isPinging.value) delay(200) }
                val bulk = vm.nodes.value.first { it.id == target.id }.pingMs
                InstrumentationRegistry.getInstrumentation().sendStatus(0, Bundle().apply {
                    putString("stream", "\nSame server: single=$single ms, bulk=$bulk ms; group size=${batch.size}\n")
                })
                if (single != null && single > 0) assertTrue("Single succeeded ($single) but bulk failed ($bulk)", bulk != null && bulk > 0)
            } finally {
                vm.stopPing()
                vm.updateSettings(original)
                collector.cancelAndJoin()
            }
        }
    }
}
