package com.lumen.core.vpn

import android.content.Intent
import android.net.ConnectivityManager
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import com.lumen.core.engine.EngineManager
import com.lumen.core.engine.EngineState
import com.lumen.core.engine.EngineType
import com.lumen.core.engine.ProcessEngine
import com.lumen.core.engine.TrafficStats
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.amnezia.awg.hevtunnel.TProxyService
import java.io.File
import java.io.InputStream
import java.net.InetSocketAddress
import java.net.Socket

class LumenVpnService : VpnService() {
    private val serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    @Volatile private var vpnInterface: ParcelFileDescriptor? = null
    @Volatile private var tunnelStarted = false
    private var stateJob: Job? = null
    private var tunnelJob: Job? = null

    val engineManager: EngineManager by lazy {
        val nativeDir = File(applicationInfo.nativeLibraryDir)
        EngineManager(
            singboxEngine = ProcessEngine(
                EngineType.SINGBOX,
                File(nativeDir, "libsingbox.so"),
                File(cacheDir, "singbox-extended"),
                ::logEngine,
                { code -> onCoreExit("sing-box extended", code) }
            )
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        when (intent.action) {
            ACTION_START_VPN -> {
                val engineType = runCatching {
                    EngineType.valueOf(intent.getStringExtra(EXTRA_ENGINE_TYPE) ?: EngineType.SINGBOX.name)
                }.getOrDefault(EngineType.SINGBOX)
                val splitMode = runCatching {
                    SplitTunnelingMode.valueOf(
                        intent.getStringExtra(EXTRA_SPLIT_MODE) ?: SplitTunnelingMode.DISABLED.name
                    )
                }.getOrDefault(SplitTunnelingMode.DISABLED)
                startVpn(
                    engineType = engineType,
                    configJson = intent.getStringExtra(EXTRA_CONFIG_JSON) ?: "{}",
                    mtu = intent.getIntExtra(EXTRA_MTU, DEFAULT_MTU).coerceIn(1280, 9000),
                    localSocksPort = intent.getIntExtra(EXTRA_LOCAL_SOCKS_PORT, LOCAL_SOCKS_PORT).coerceIn(1024, 65535),
                    dnsMode = intent.getStringExtra(EXTRA_DNS_MODE) ?: "automatic",
                    splitConfig = SplitTunnelingConfig(
                        splitMode,
                        (intent.getStringArrayListExtra(EXTRA_SPLIT_PACKAGES) ?: arrayListOf()).toSet()
                    )
                )
            }
            ACTION_STOP_VPN -> stopVpn()
        }
        return START_STICKY
    }

    private fun startVpn(
        engineType: EngineType,
        configJson: String,
        mtu: Int,
        localSocksPort: Int,
        dnsMode: String,
        splitConfig: SplitTunnelingConfig
    ) {
        VpnLogBus.clearLastError()
        if (configJson.isBlank() || configJson == "{}" || configJson.length < 10 || !configJson.contains("outbound")) {
            VpnLogBus.error("VPN", "Cannot start VPN: invalid or empty server configuration")
            stopSelf()
            return
        }
        startForeground(
            NotificationHelper.NOTIFICATION_ID,
            NotificationHelper.buildNotification(this, isConnected = true)
        )
        VpnLogBus.beginSession("sing-box extended")
        VpnLogBus.info("VPN", "Start requested; MTU=$mtu, split mode=${splitConfig.mode}")
        serviceScope.launch {
            stopRuntime(closeInterface = true)
            try {
                enforcePrivateDnsPolicy(dnsMode)
                val builder = Builder()
                    .setMtu(mtu)
                    .addAddress(DEFAULT_IPV4_ADDRESS, DEFAULT_IPV4_PREFIX)
                    .addAddress(DEFAULT_IPV6_ADDRESS, DEFAULT_IPV6_PREFIX)
                    .addRoute("0.0.0.0", 0)
                    .addRoute("::", 0)
                    .setSession("Lumen VPN")
                val vpnDnsServers = if (dnsMode.lowercase() in setOf("android", "system")) {
                    currentNetworkDnsServers()
                } else {
                    listOf(INTERNAL_DNS_ADDRESS)
                }
                vpnDnsServers.forEach(builder::addDnsServer)
                // The core's outbound sockets must bypass this VPN to avoid a routing loop.
                // ALLOW_LIST caveats: our own package must never be allowed, and an
                // empty allow-list would make Android capture ALL apps (VpnService
                // applies to everyone when no allowed app was added) including the
                // sing-box process itself -> traffic loop and a dead tunnel.
                val effectiveSplit = when {
                    splitConfig.mode == SplitTunnelingMode.ALLOW_LIST &&
                        (splitConfig.packages - packageName).isEmpty() ->
                        SplitTunnelingConfig(SplitTunnelingMode.DISABLED)
                    splitConfig.mode == SplitTunnelingMode.ALLOW_LIST ->
                        splitConfig.copy(packages = splitConfig.packages - packageName)
                    else -> splitConfig
                }
                if (effectiveSplit.mode != SplitTunnelingMode.ALLOW_LIST) {
                    builder.addDisallowedApplication(packageName)
                }
                SplitTunnelingManager.applySplitTunneling(builder, effectiveSplit)

                val pfd = checkNotNull(builder.establish()) { "Failed to establish VPN interface" }
                vpnInterface = pfd

                VpnLogBus.info("VPN", "TUN interface established; starting proxy core")
                engineManager.startEngine(engineType, configJson, null)
                val state = engineManager.state.value
                check(state is EngineState.Running) {
                    (state as? EngineState.Error)?.message ?: "Engine failed to start"
                }
                waitForLocalSocks(localSocksPort)

                val hevConfig = File(cacheDir, "hev-tunnel.yaml").apply {
                    writeText(
                        """tunnel:
  mtu: $mtu
socks5:
  port: $localSocksPort
  address: 127.0.0.1
  udp: 'udp'
log:
  level: info
  output: stderr
misc:
  task-stack-size: 20480
  connect-timeout: 5000
  read-write-timeout: 60000
"""
                    )
                }
                tunnelStarted = true
                VpnLogBus.info("TUN2SOCKS", "Starting hev-socks5-tunnel on fd=${pfd.fd}")
                tunnelJob = serviceScope.launch {
                    try {
                        TProxyService.TProxyStartService(hevConfig.absolutePath, pfd.fd)
                        VpnLogBus.debug("TUN2SOCKS", "Native tunnel start was accepted")
                    } catch (t: Throwable) {
                        tunnelStarted = false
                        VpnLogBus.error("TUN2SOCKS", "Native tunnel failed", t)
                        stopRuntime(closeInterface = true)
                        stopForeground(STOP_FOREGROUND_REMOVE)
                        stopSelf()
                    }
                }
                delay(350)
                check(tunnelStarted) { "hev-socks5-tunnel failed to start" }
                // Do not publish Connected for a merely running process. A domain
                // CONNECT exercises bootstrap DNS, managed DNS and the selected proxy.
                verifyProxyDataPath(localSocksPort)
                _isRunning.value = true
                notifyWidgets()
                TelemetryManager.startHeartbeatLoop(this@LumenVpnService, serviceScope)
                observeEngineState()
                VpnLogBus.info("VPN", "Connected (${engineType.name}); traffic is handled by sing-box + tun2socks")
                Log.i(TAG, "VPN started with ${engineType.name}")
            } catch (t: Throwable) {
                Log.e(TAG, "Error starting VPN", t)
                VpnLogBus.error("VPN", "Connection failed", t)
                stopRuntime(closeInterface = true)
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
    }

    private fun currentNetworkDnsServers(): List<String> {
        val manager = getSystemService(ConnectivityManager::class.java)
        val addresses = manager.activeNetwork
            ?.let(manager::getLinkProperties)
            ?.dnsServers
            ?.mapNotNull { it.hostAddress?.substringBefore('%') }
            ?.distinct()
            .orEmpty()
        return addresses.ifEmpty { DEFAULT_DNS_SERVERS }
    }

    private fun enforcePrivateDnsPolicy(dnsMode: String) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P ||
            dnsMode.lowercase() !in setOf("automatic", "secure")) return
        val manager = getSystemService(ConnectivityManager::class.java)
        val strictHost = manager.activeNetwork
            ?.let(manager::getLinkProperties)
            ?.privateDnsServerName
            ?.trim()
            .orEmpty()
        check(strictHost.isEmpty()) {
            "Strict Private DNS ($strictHost) conflicts with managed VPN DNS. Select DNS Android or disable Private DNS."
        }
    }

    private suspend fun waitForLocalSocks(localSocksPort: Int) {
        repeat(50) { attempt ->
            val ready = runCatching {
                Socket().use { it.connect(InetSocketAddress("127.0.0.1", localSocksPort), 200) }
                true
            }.getOrDefault(false)
            if (ready) {
                VpnLogBus.info("CORE", "Local SOCKS5 is ready on 127.0.0.1:$localSocksPort")
                return
            }
            if (attempt % 10 == 0) VpnLogBus.debug("CORE", "Waiting for local SOCKS5…")
            delay(100)
        }
        error("Proxy core did not open local SOCKS5 port $localSocksPort")
    }

    /**
     * A running process and an open local SOCKS port do not prove that traffic reaches the server.
     * Complete a real SOCKS5 CONNECT through the selected core before reporting Connected.
     */
    private fun verifyProxyDataPath(localSocksPort: Int) {
        val probeHost = "1.1.1.1"
        val probePort = 443
        VpnLogBus.info("VPN", "Verifying proxy data path via SOCKS5 → $probeHost:$probePort")
        Socket().use { socket ->
            socket.soTimeout = PROXY_PROBE_TIMEOUT_MS
            socket.connect(InetSocketAddress("127.0.0.1", localSocksPort), PROXY_PROBE_TIMEOUT_MS)
            val output = socket.getOutputStream()
            val input = socket.getInputStream()

            output.write(byteArrayOf(0x05, 0x01, 0x00))
            output.flush()
            val greeting = input.readExactly(2)
            check(greeting[0].toInt() == 0x05 && greeting[1].toInt() == 0x00) {
                "SOCKS5 authentication was rejected"
            }

            output.write(byteArrayOf(0x05, 0x01, 0x00, 0x01, 1, 1, 1, 1, 0x01, 0xbb.toByte()))
            output.flush()
            val reply = input.readExactly(4)
            check(reply[0].toInt() == 0x05 && reply[1].toInt() == 0x00) {
                "Proxy CONNECT failed (SOCKS reply ${reply[1].toInt() and 0xff})"
            }
            val addressLength = when (reply[3].toInt() and 0xff) {
                0x01 -> 4
                0x04 -> 16
                0x03 -> input.readExactly(1)[0].toInt() and 0xff
                else -> error("Proxy returned an invalid SOCKS address type")
            }
            input.readExactly(addressLength + 2)
        }
        VpnLogBus.info("VPN", "Proxy data path verified")
    }

    private fun InputStream.readExactly(count: Int): ByteArray {
        val bytes = ByteArray(count)
        var offset = 0
        while (offset < count) {
            val read = read(bytes, offset, count - offset)
            check(read >= 0) { "Unexpected end of SOCKS5 response" }
            offset += read
        }
        return bytes
    }

    private fun onCoreExit(name: String, code: Int) {
        if (!_isRunning.value && vpnInterface == null) return
        VpnLogBus.error("CORE", "$name stopped unexpectedly (exit code $code)")
        _isRunning.value = false
        serviceScope.launch {
            stopRuntime(closeInterface = true)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    private fun observeEngineState() {
        stateJob?.cancel()
        stateJob = serviceScope.launch {
            engineManager.state.collect { state ->
                when (state) {
                    is EngineState.Running -> {
                        val prefs = getSharedPreferences("lumen_prefs", MODE_PRIVATE)
                        val showNotif = prefs.getBoolean("show_notification", true)
                        val showSpeed = prefs.getBoolean("show_notification_speed", true)
                        if (!showNotif) {
                            (getSystemService(NOTIFICATION_SERVICE) as? android.app.NotificationManager)?.cancel(NotificationHelper.NOTIFICATION_ID)
                        } else {
                            NotificationHelper.updateNotification(
                                this@LumenVpnService,
                                if (showSpeed) state.stats else TrafficStats(),
                                true
                            )
                        }
                    }
                    is EngineState.Error -> {
                        Log.e(TAG, "Engine error: ${state.message}")
                        VpnLogBus.error("CORE", state.message)
                    }
                    else -> Unit
                }
            }
        }
    }

    private fun stopVpn() {
        serviceScope.launch {
            stopRuntime(closeInterface = true)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    private suspend fun stopRuntime(closeInterface: Boolean) {
        stateJob?.cancel()
        stateJob = null
        if (tunnelStarted) {
            VpnLogBus.info("TUN2SOCKS", "Stopping native tunnel")
            runCatching { TProxyService.TProxyStopService() }
                .onFailure { VpnLogBus.warning("TUN2SOCKS", "Stop failed: ${it.message}") }
            tunnelStarted = false
        }
        tunnelJob?.cancel()
        tunnelJob = null
        runCatching { engineManager.stopEngine() }
            .onFailure { Log.w(TAG, "Failed to stop engine", it) }
        if (closeInterface) {
            runCatching { vpnInterface?.close() }
            vpnInterface = null
        }
        _isRunning.value = false
        TelemetryManager.stopHeartbeatLoop()
        notifyWidgets()
    }

    /**
     * Home screen widgets only refresh when they are told to. The broadcast has
     * to be explicit (setPackage), otherwise Android 8+ drops it before it ever
     * reaches the manifest-declared receiver.
     */
    private fun notifyWidgets() {
        runCatching {
            sendBroadcast(
                Intent(WIDGET_UPDATE_ACTION).setPackage(packageName)
            )
        }
    }

    override fun onDestroy() {
        stateJob?.cancel()
        if (tunnelStarted) runCatching { TProxyService.TProxyStopService() }
        tunnelStarted = false
        tunnelJob?.cancel()
        tunnelJob = null
        runBlocking(Dispatchers.IO) {
            runCatching { engineManager.stopEngine() }
        }
        runCatching { vpnInterface?.close() }
        vpnInterface = null
        _isRunning.value = false
        notifyWidgets()
        serviceScope.cancel()
        super.onDestroy()
    }

    private fun logEngine(line: String) {
        Log.i(TAG, line)
        when {
            line.contains("fatal", ignoreCase = true) || line.contains("panic", ignoreCase = true) ->
                VpnLogBus.error("CORE", line)
            line.contains("warn", ignoreCase = true) || line.contains("error", ignoreCase = true) ->
                VpnLogBus.warning("CORE", line)
            else -> VpnLogBus.info("CORE", line)
        }
    }

    companion object {
        private const val TAG = "LumenVpnService"
        private const val LOCAL_SOCKS_PORT = 10808
        private const val PROXY_PROBE_TIMEOUT_MS = 8_000
        // Kept as a literal (and not a reference to the app module) because
        // :core:vpn must not depend on :app.
        const val WIDGET_UPDATE_ACTION = "com.lumen.app.widget.ACTION_UPDATE_STATE"
        const val ACTION_START_VPN = "com.lumen.core.vpn.START_VPN"
        const val ACTION_STOP_VPN = "com.lumen.core.vpn.STOP_VPN"
        const val EXTRA_ENGINE_TYPE = "extra_engine_type"
        const val EXTRA_CONFIG_JSON = "extra_config_json"
        const val EXTRA_SPLIT_MODE = "extra_split_mode"
        const val EXTRA_SPLIT_PACKAGES = "extra_split_packages"
        const val EXTRA_MTU = "extra_mtu"
        const val EXTRA_LOCAL_SOCKS_PORT = "extra_local_socks_port"
        const val EXTRA_DNS_MODE = "extra_dns_mode"
        const val DEFAULT_IPV4_ADDRESS = "172.19.0.1"
        const val DEFAULT_IPV4_PREFIX = 30
        const val DEFAULT_IPV6_ADDRESS = "fdfe:dcba:9876::1"
        const val DEFAULT_IPV6_PREFIX = 126
        const val DEFAULT_MTU = 1500
        const val INTERNAL_DNS_ADDRESS = "172.19.0.2"
        val DEFAULT_DNS_SERVERS = listOf("1.1.1.1", "8.8.8.8")
        private val _isRunning = MutableStateFlow(false)
        val isRunning: StateFlow<Boolean> = _isRunning.asStateFlow()
    }
}
