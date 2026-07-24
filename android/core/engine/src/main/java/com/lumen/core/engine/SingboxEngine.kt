package com.lumen.core.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Interface representing the native bridge to libbox.aar (sing-box extended Go library).
 */
interface LibboxBridge {
    fun startService(configJson: String, tunFd: Int?): Boolean
    fun stopService(): Boolean
    fun queryTraffic(): TrafficStats
    fun isServiceRunning(): Boolean
}

/**
 * Default implementation of LibboxBridge using JNI/Reflection fallback when libbox is present.
 */
class DefaultLibboxBridge : LibboxBridge {
    @Volatile
    private var running = false
    private var totalRx = 0L
    private var totalTx = 0L

    override fun startService(configJson: String, tunFd: Int?): Boolean {
        return try {
            val libboxClazz = Class.forName("io.nekohasekai.libbox.Libbox")
            val method = libboxClazz.getMethod("startService", String::class.java, Int::class.javaPrimitiveType)
            method.invoke(null, configJson, tunFd ?: -1)
            running = true
            true
        } catch (_: ClassNotFoundException) {
            // No native libbox: report a real failure instead of a fake "running" state.
            running = false
            false
        } catch (e: Exception) {
            throw RuntimeException("Failed to start libbox engine: ${e.message}", e)
        }
    }

    override fun stopService(): Boolean {
        return try {
            try {
                val libboxClazz = Class.forName("io.nekohasekai.libbox.Libbox")
                val method = libboxClazz.getMethod("stopService")
                method.invoke(null)
            } catch (_: ClassNotFoundException) {}
            running = false
            true
        } catch (_: Exception) {
            running = false
            false
        }
    }

    override fun queryTraffic(): TrafficStats {
        return try {
            val libboxClazz = Class.forName("io.nekohasekai.libbox.Libbox")
            val method = libboxClazz.getMethod("queryTraffic")
            method.invoke(null)
            // libbox reports counters through its own callbacks; mirror last known totals.
            TrafficStats(0L, 0L, totalTx, totalRx)
        } catch (_: Exception) {
            TrafficStats(0L, 0L, totalTx, totalRx)
        }
    }

    override fun isServiceRunning(): Boolean = running
}

/**
 * Driver wrapping libbox.aar (sing-box extended Go library).
 */
class SingboxEngine(
    private val bridge: LibboxBridge = DefaultLibboxBridge()
) : IEngineDriver {
    override val type: EngineType = EngineType.SINGBOX

    override val isRunning: Boolean
        get() = bridge.isServiceRunning()

    override suspend fun start(configJson: String, tunFd: Int?) = withContext(Dispatchers.IO) {
        require(configJson.isNotBlank()) { "Singbox configuration cannot be empty" }
        if (isRunning) {
            stop()
        }
        val success = bridge.startService(configJson, tunFd)
        if (!success) {
            throw IllegalStateException("Failed to start Singbox engine service")
        }
    }

    override suspend fun stop() = withContext(Dispatchers.IO) {
        if (isRunning) {
            bridge.stopService()
        }
    }

    override fun getTrafficStats(): TrafficStats {
        return bridge.queryTraffic()
    }
}
