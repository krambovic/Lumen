package com.lumen.core.vpn

import android.app.Notification
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.TrafficStats as AndroidTrafficStats
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.os.Process
import android.util.Log
import androidx.core.app.ServiceCompat
import com.lumen.core.engine.EngineManager
import com.lumen.core.engine.EngineState
import com.lumen.core.engine.EngineType
import com.lumen.core.engine.ProcessEngine
import com.lumen.core.engine.TrafficStats
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CancellationException
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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.amnezia.awg.hevtunnel.TProxyService
import java.io.File
import java.io.InputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.atomic.AtomicBoolean
import javax.net.ssl.SSLSocket
import javax.net.ssl.SSLSocketFactory
import kotlin.concurrent.thread

class LumenVpnService : VpnService() {
    private val serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    @Volatile private var vpnInterface: ParcelFileDescriptor? = null
    private val tunnelStarted = AtomicBoolean(false)
    // bringUpRuntime/stopRuntime are reachable from five independent coroutines;
    // without this they race on tunnelStarted and on vpnInterface.
    private val runtimeMutex = Mutex()
    // Async teardown must not cancel a start that arrived in the meantime.
    @Volatile private var lastStartId = 0
    private var stateJob: Job? = null
    private var trafficStatsJob: Job? = null
    private var tunnelJob: Job? = null
    private var startJob: Job? = null
    private var sessionUploaded = 0L
    private var sessionDownloaded = 0L

    // Everything needed to bring the tunnel back up on a different network. AWG/WireGuard
    // are UDP based: after a Wi-Fi <-> mobile switch the core keeps sockets bound to the
    // dead network and silently stops passing traffic, so the runtime has to be rebuilt.
    private data class StartParams(
        val engineType: EngineType,
        // Resolved from [configPath] on the IO thread; an intent from an older
        // install may still carry it inline.
        val configJson: String,
        val configPath: String,
        val mtu: Int,
        val localSocksPort: Int,
        val dnsMode: String,
        val splitConfig: SplitTunnelingConfig,
        // Applies to every protocol; can be switched off in the settings.
        val reconnectOnNetworkChange: Boolean,
        // OpenVPN "Use proxy" with obfs2/obfs3: the transport is terminated by the
        // in-app relay, so the core only sees a plain loopback SOCKS5 detour.
        val obfsType: String = "",
        val obfsHost: String = "",
        val obfsPort: Int = 0
    )

    @Volatile private var activeParams: StartParams? = null
    @Volatile private var obfsRelay: ObfsRelay? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null
    private var restartJob: Job? = null
    @Volatile private var lastUnderlyingNetwork: Network? = null
    @Volatile private var pendingUnderlyingNetwork: Network? = null
    // Set while the runtime is deliberately torn down for a reconnect, so the engine exit
    // callback does not treat the planned shutdown as a crash and kill the service.
    @Volatile private var restartingForNetwork = false

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

    /**
     * A tile or widget start brings this service up without the app ever being
     * created, so the persistent log has to be attached from here as well.
     */
    override fun onCreate() {
        super.onCreate()
        VpnLogBus.init(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        lastStartId = startId
        if (intent == null) {
            stopSelf(startId)
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
                    StartParams(
                        engineType = engineType,
                        // Kept only for a PendingIntent built by an older install:
                        // the config travels as a path now, never as an extra.
                        configJson = intent.getStringExtra(EXTRA_CONFIG_JSON).orEmpty(),
                        configPath = intent.getStringExtra(EXTRA_CONFIG_PATH).orEmpty(),
                        mtu = intent.getIntExtra(EXTRA_MTU, DEFAULT_MTU).coerceIn(1280, 9000),
                        localSocksPort = intent.getIntExtra(EXTRA_LOCAL_SOCKS_PORT, LOCAL_SOCKS_PORT).coerceIn(1024, 65535),
                        dnsMode = intent.getStringExtra(EXTRA_DNS_MODE) ?: "automatic",
                        splitConfig = SplitTunnelingConfig(
                            splitMode,
                            (intent.getStringArrayListExtra(EXTRA_SPLIT_PACKAGES) ?: arrayListOf()).toSet()
                        ),
                        reconnectOnNetworkChange = intent.getBooleanExtra(
                            EXTRA_RECONNECT_ON_NETWORK_CHANGE,
                            true
                        ),
                        obfsType = intent.getStringExtra(EXTRA_OBFS_TYPE).orEmpty(),
                        obfsHost = intent.getStringExtra(EXTRA_OBFS_HOST).orEmpty(),
                        obfsPort = intent.getIntExtra(EXTRA_OBFS_PORT, 0)
                    )
                )
            }
            ACTION_STOP_VPN -> stopVpn()
        }
        // Every start is explicit and carries the complete generated config.
        // Recreating this service without that intent can only create a stop/start loop.
        return START_NOT_STICKY
    }

    private fun startVpn(params: StartParams) {
        // A failure must only stop the start it belongs to: a newer start intent
        // moves lastStartId on, and stopSelf() with that id would kill it instead.
        val thisStartId = lastStartId
        // Every caller uses startForegroundService(), which arms a watchdog that
        // kills the process unless startForeground() runs before anything tears the
        // service down. Claim the slot before any branch that can return.
        NotificationHelper.invalidate()
        startForegroundCompat(
            if (_isRunning.value) {
                NotificationHelper.buildNotification(this, isConnected = true)
            } else {
                NotificationHelper.buildConnectingNotification(this)
            }
        )
        reportNotificationVisibility()
        if (_isStarting.value) {
            VpnLogBus.warning("VPN", "Ignoring a duplicate start while a connection is already being established")
            return
        }
        if (_isRunning.value) {
            VpnLogBus.warning("VPN", "Ignoring a duplicate start while VPN is already connected")
            return
        }
        VpnLogBus.clearLastError()
        restartJob?.cancel()
        restartJob = null
        _isStarting.value = true
        startJob = serviceScope.launch {
            try {
                // An imported AUTO pool is hundreds of kilobytes, so the config is
                // never carried by the intent; reading it back is IO and belongs here.
                val resolved = if (params.configJson.isNotBlank()) {
                    params
                } else {
                    params.copy(
                        configJson = VpnConfigStore.read(this@LumenVpnService, params.configPath)
                    )
                }
                if (!VpnStartIntentFactory.isUsableConfig(resolved.configJson)) {
                    VpnLogBus.error("VPN", "Cannot start VPN: invalid or empty server configuration")
                    stopForeground(STOP_FOREGROUND_REMOVE)
                    stopSelf(thisStartId)
                    return@launch
                }
                VpnLogBus.beginSession("sing-box extended")
                VpnLogBus.info(
                    "VPN",
                    "Start requested; MTU=${resolved.mtu}, split mode=${resolved.splitConfig.mode}"
                )
                activeParams = resolved
                bringUpRuntime(resolved)
                // Watch the default network only once the tunnel is actually up.
                if (resolved.reconnectOnNetworkChange) {
                    registerNetworkMonitor()
                } else {
                    VpnLogBus.info("VPN", "Reconnect on network change is disabled in settings")
                }
            } catch (cancelled: CancellationException) {
                VpnLogBus.info("VPN", "Connection attempt cancelled")
                throw cancelled
            } catch (t: Throwable) {
                Log.e(TAG, "Error starting VPN", t)
                VpnLogBus.error("VPN", "Connection failed", t)
                clearSessionState()
                stopRuntime(closeInterface = true)
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf(thisStartId)
            } finally {
                _isStarting.value = false
                startJob = null
            }
        }
    }

    /**
     * Android 14 rejects a startForeground() whose type is not one the manifest
     * declares, and kills the process for it. There is no VPN foreground service
     * type, so :core:vpn declares specialUse and that is what has to be passed.
     */
    private fun startForegroundCompat(notification: Notification) {
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                ServiceCompat.startForeground(
                    this,
                    NotificationHelper.NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
                )
            } else {
                startForeground(NotificationHelper.NOTIFICATION_ID, notification)
            }
        }.onFailure {
            VpnLogBus.error("VPN", "Could not enter the foreground: ${it.message}", it)
        }
    }

    /**
     * With POST_NOTIFICATIONS denied on Android 13+ the tunnel comes up but its
     * notification never appears, which looks exactly like a broken connection.
     * Publish that instead of leaving it silent.
     */
    private fun reportNotificationVisibility() {
        val blocked = !NotificationHelper.notificationsEnabled(this)
        if (blocked && !_notificationsBlocked.value) {
            VpnLogBus.warning(
                "VPN",
                "Notifications are turned off for Lumen: the tunnel runs but its status " +
                    "notification stays hidden. Allow notifications in Android settings."
            )
        }
        _notificationsBlocked.value = blocked
    }

    /**
     * Builds the TUN interface, starts the proxy core and the tun2socks bridge. Used both for
     * the initial connection and for re-establishing the tunnel on a different network.
     */
    private suspend fun bringUpRuntime(params: StartParams) = runtimeMutex.withLock {
        bringUpRuntimeLocked(params)
    }

    private suspend fun bringUpRuntimeLocked(params: StartParams) {
        val engineType = params.engineType
        val mtu = params.mtu
        val dnsMode = params.dnsMode
        val splitConfig = params.splitConfig
        stopRuntimeLocked(closeInterface = true)
        // Capture the real path before Android publishes our VPN as a network.
        val connectivityManager = getSystemService(ConnectivityManager::class.java)
        lastUnderlyingNetwork = connectivityManager?.activeNetwork?.takeIf {
            isUsableUnderlyingNetwork(connectivityManager, it)
        } ?: lastUnderlyingNetwork
        startForegroundCompat(NotificationHelper.buildConnectingNotification(this))
        // obfs2/obfs3 "Use proxy": the local transport must listen before the core
        // dials its loopback detour.
        startObfsRelay(params)
        enforcePrivateDnsPolicy(dnsMode)

        VpnLogBus.info("CORE", "Starting proxy core")
        val localSocksPort = startCoreOnFreeLocalPort(params)
        val validateDataPath = getSharedPreferences("lumen_prefs", MODE_PRIVATE)
            .getBoolean("validate_proxy_data_path", false)
        if (validateDataPath) {
            verifyProxyDataPathWithRetries(localSocksPort)
            check(engineManager.singboxEngine.isRunning) {
                "Proxy core stopped after the connectivity check"
            }
        }

        val builder = Builder()
            .setMtu(mtu)
            .addAddress(DEFAULT_IPV4_ADDRESS, DEFAULT_IPV4_PREFIX)
            .addAddress(DEFAULT_IPV6_ADDRESS, DEFAULT_IPV6_PREFIX)
            .addRoute("0.0.0.0", 0)
            .addRoute("::", 0)
            .setSession("Lumen VPN")
            .setBlocking(true)
        val vpnDnsServers = if (dnsMode.lowercase() in setOf("android", "system")) {
            currentNetworkDnsServers()
        } else {
            listOf(INTERNAL_DNS_ADDRESS)
        }
        vpnDnsServers.forEach(builder::addDnsServer)
        // The core's outbound sockets must bypass this VPN to avoid a routing loop.
        // ALLOW_LIST caveats: our own package must never be allowed, and an
        // empty allow-list would make Android capture ALL apps.
        val effectiveSplit = when {
            splitConfig.mode == SplitTunnelingMode.ALLOW_LIST &&
                (splitConfig.packages - packageName).isEmpty() ->
                SplitTunnelingConfig(SplitTunnelingMode.DISABLED)
            splitConfig.mode == SplitTunnelingMode.ALLOW_LIST ->
                splitConfig.copy(packages = splitConfig.packages - packageName)
            else -> splitConfig
        }
        val appliedSplit = SplitTunnelingManager.applySplitTunneling(builder, effectiveSplit)
        // An allow-list whose apps are all uninstalled leaves the builder without any
        // per-app filter, which captures every UID including ours, so fall back to
        // disallowing only our own package.
        if (SplitTunnelingManager.requiresSelfDisallow(effectiveSplit.mode, appliedSplit)) {
            if (effectiveSplit.mode == SplitTunnelingMode.ALLOW_LIST) {
                VpnLogBus.warning(
                    "VPN",
                    "No allow-listed app is installed; split tunneling is off for this session"
                )
            }
            builder.addDisallowedApplication(packageName)
        }

        val pfd = checkNotNull(builder.establish()) { "Failed to establish VPN interface" }
        vpnInterface = pfd
        VpnLogBus.info("VPN", "TUN interface established; starting tun2socks")

        // The shipped libhev-socks5-tunnel only knows misc.log-level / misc.log-file:
        // a top-level `log:` section parses but is dropped, so the level never applied.
        val hevConfig = File(cacheDir, "hev-tunnel.yaml").apply {
            writeText(
                """tunnel:
  mtu: $mtu
socks5:
  port: $localSocksPort
  address: 127.0.0.1
  udp: 'udp'
misc:
  log-level: info
  log-file: stderr
  task-stack-size: 20480
  connect-timeout: 5000
  read-write-timeout: 60000
"""
            )
        }
        tunnelStarted.set(true)
        VpnLogBus.info("TUN2SOCKS", "Starting hev-socks5-tunnel on fd=${pfd.fd}")
        tunnelJob = serviceScope.launch {
            try {
                TProxyService.TProxyStartService(hevConfig.absolutePath, pfd.fd)
                VpnLogBus.debug("TUN2SOCKS", "Native tunnel start was accepted")
            } catch (t: Throwable) {
                tunnelStarted.set(false)
                VpnLogBus.error("TUN2SOCKS", "Native tunnel failed", t)
                stopRuntime(closeInterface = true)
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf(lastStartId)
            }
        }
        delay(350)
        check(tunnelStarted.get()) { "hev-socks5-tunnel failed to start" }
        check(engineManager.singboxEngine.isRunning) {
            "Proxy core stopped before the VPN tunnel became ready"
        }
        _isRunning.value = true
        ensureSessionStarted()
        startTrafficStats()
        // The session start time and the connected title only exist now, so the
        // cached builder from the connecting stage has to go.
        NotificationHelper.invalidate()
        startForegroundCompat(NotificationHelper.buildNotification(this, isConnected = true))
        reportNotificationVisibility()
        notifyWidgets()
        observeEngineState()
        VpnLogBus.info("VPN", "Connected (${engineType.name}); traffic is handled by sing-box + tun2socks")
        Log.i(TAG, "VPN started with ${engineType.name}")
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

    /**
     * Starts the core on a local SOCKS port that is genuinely free and returns it.
     *
     * The configured port stays the preferred one; it is only given up while the
     * previous session's core still holds it. Losing that race used to be fatal:
     * the core exits immediately with "bind: address already in use" and the app
     * then spent its whole readiness timeout waiting for a port nobody opened,
     * which is the slow, generic failure seen on reconnects and server switches.
     */
    private suspend fun startCoreOnFreeLocalPort(params: StartParams): Int {
        var lastError: Throwable? = null
        repeat(CORE_START_ATTEMPTS) { attempt ->
            val port = LocalSocksPort.resolve(params.localSocksPort)
            if (port != params.localSocksPort) {
                VpnLogBus.warning(
                    "CORE",
                    "Local SOCKS port ${params.localSocksPort} is taken; using $port for this session"
                )
            }
            // The optional local HTTP inbound races for its port exactly like the
            // SOCKS one, and losing it kills the core just as dead:
            // `start inbound/http[http-in]: listen tcp 127.0.0.1:10809: bind:
            // address already in use`. Its port never reaches us as a parameter,
            // so take it from the config we are about to start.
            var configJson = LocalSocksPort.applyToConfig(params.configJson, port)
            LocalSocksPort.portOf(configJson, LocalSocksPort.HTTP_INBOUND_TAG)?.let { wanted ->
                val httpPort = LocalSocksPort.resolve(wanted)
                if (httpPort != wanted) {
                    VpnLogBus.warning(
                        "CORE",
                        "Local HTTP port $wanted is taken; using $httpPort for this session"
                    )
                    configJson = LocalSocksPort.applyToConfig(
                        configJson,
                        httpPort,
                        LocalSocksPort.HTTP_INBOUND_TAG
                    )
                }
            }
            val result = runCatching {
                engineManager.startEngine(params.engineType, configJson, null)
                val state = engineManager.state.value
                check(state is EngineState.Running) {
                    (state as? EngineState.Error)?.message ?: "Engine failed to start"
                }
                waitForLocalSocks(port)
            }
            if (result.isSuccess) return port
            lastError = result.exceptionOrNull()
            if (!ProcessEngine.isAddressInUse(lastError?.message) ||
                attempt == CORE_START_ATTEMPTS - 1
            ) {
                throw lastError ?: IllegalStateException("Proxy core failed to start")
            }
            VpnLogBus.warning("CORE", "A local inbound port was still in use; retrying")
            runCatching { engineManager.stopEngine() }
            delay(CORE_START_RETRY_DELAY_MS)
        }
        throw lastError ?: IllegalStateException("Proxy core failed to start")
    }

    private suspend fun waitForLocalSocks(localSocksPort: Int) {
        repeat(SOCKS_READY_ATTEMPTS) { attempt ->
            val ready = runCatching {
                Socket().use { it.connect(InetSocketAddress("127.0.0.1", localSocksPort), 200) }
                true
            }.getOrDefault(false)
            if (ready) {
                VpnLogBus.info("CORE", "Local SOCKS5 is ready on 127.0.0.1:$localSocksPort")
                return
            }
            // A core that already died will never open the port, and burning the
            // rest of the timeout only makes the failure slower, not clearer.
            if (!engineManager.singboxEngine.isRunning) error(coreExitReason())
            if (attempt % 10 == 0) VpnLogBus.debug("CORE", "Waiting for local SOCKS5…")
            delay(100)
        }
        error("Proxy core did not open local SOCKS5 port $localSocksPort")
    }

    /** The core's own last word; a bare exit code tells the user nothing. */
    private fun coreExitReason(): String {
        val fatal = engineManager.singboxEngine.lastFatalLine
        return if (fatal.isBlank()) {
            "core exited during startup"
        } else {
            "core exited during startup: $fatal"
        }
    }

    /**
     * A running process and an open local SOCKS port do not prove that traffic reaches the server.
     * Complete a real SOCKS5 CONNECT through the selected core before reporting Connected.
     */
    private fun verifyProxyDataPath(localSocksPort: Int) {
        val probeHost = "www.gstatic.com"
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

            val hostBytes = probeHost.toByteArray(Charsets.US_ASCII)
            check(hostBytes.size in 1..255) { "Invalid proxy probe host" }
            output.write(byteArrayOf(0x05, 0x01, 0x00, 0x03, hostBytes.size.toByte()))
            output.write(hostBytes)
            output.write(byteArrayOf((probePort ushr 8).toByte(), probePort.toByte()))
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

            // A successful SOCKS CONNECT only proves that the remote TCP socket
            // opened. Complete TLS and read an HTTP status so AUTO cannot be
            // reported as connected while its selected member drops all data.
            val tls = (SSLSocketFactory.getDefault() as SSLSocketFactory)
                .createSocket(socket, probeHost, probePort, false) as SSLSocket
            tls.use { ssl ->
                ssl.soTimeout = PROXY_PROBE_TIMEOUT_MS
                ssl.startHandshake()
                ssl.outputStream.write(
                    (
                        "GET /generate_204 HTTP/1.1\r\n" +
                            "Host: $probeHost\r\n" +
                            "User-Agent: Lumen-Android\r\n" +
                            "Connection: close\r\n\r\n"
                        ).toByteArray(Charsets.US_ASCII)
                )
                ssl.outputStream.flush()
                val statusLine = ssl.inputStream.bufferedReader(Charsets.US_ASCII).readLine().orEmpty()
                val status = statusLine.split(' ').getOrNull(1)?.toIntOrNull() ?: 0
                check(status in 200..399) {
                    "Proxy HTTPS probe failed (${statusLine.ifBlank { "no HTTP response" }})"
                }
            }
        }
        VpnLogBus.info("VPN", "Proxy HTTPS data path verified")
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

    /**
     * AWG/WireGuard has to complete a handshake before a single packet gets through, and a
     * freshly started core may still be resolving its endpoint. A single probe therefore
     * fails the whole connection "the first time" even though the tunnel is up a moment
     * later, which is exactly the flakiness seen on AWG servers.
     */
    private suspend fun verifyProxyDataPathWithRetries(localSocksPort: Int) {
        var lastError: Throwable? = null
        repeat(PROXY_PROBE_ATTEMPTS) { attempt ->
            val result = runCatching { verifyProxyDataPath(localSocksPort) }
            if (result.isSuccess) return
            lastError = result.exceptionOrNull()
            VpnLogBus.warning(
                "VPN",
                "Proxy probe ${attempt + 1}/$PROXY_PROBE_ATTEMPTS failed: ${lastError?.message}"
            )
            delay(PROXY_PROBE_RETRY_DELAY_MS)
        }
        throw lastError ?: IllegalStateException("Proxy data path could not be verified")
    }

    /**
     * The proxy core binds its outbound sockets to the network it started on. After a
     * Wi-Fi <-> mobile switch those sockets are dead and UDP based transports (AWG,
     * WireGuard) stop passing traffic without reporting any error, so the runtime is
     * rebuilt on the new network instead.
     */
    private fun registerNetworkMonitor() {
        if (networkCallback != null) return
        val manager = getSystemService(ConnectivityManager::class.java) ?: return
        lastUnderlyingNetwork = lastUnderlyingNetwork ?: manager.activeNetwork?.takeIf {
            isUsableUnderlyingNetwork(manager, it)
        }
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                onDefaultNetworkMayHaveChanged(network)
            }

            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                if (capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) &&
                    capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
                ) {
                    onDefaultNetworkMayHaveChanged(network)
                }
            }

            override fun onLost(network: Network) {
                if (network == pendingUnderlyingNetwork) pendingUnderlyingNetwork = null
                if (network == lastUnderlyingNetwork) {
                    lastUnderlyingNetwork = null
                    VpnLogBus.info("VPN", "Default network lost; waiting for a new one")
                    onDefaultNetworkMayHaveChanged()
                }
            }
        }
        val request = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            .build()
        runCatching { manager.registerNetworkCallback(request, callback) }
            .onSuccess { networkCallback = callback }
            .onFailure { VpnLogBus.warning("VPN", "Network monitor unavailable: ${it.message}") }
    }

    private fun unregisterNetworkMonitor() {
        val manager = getSystemService(ConnectivityManager::class.java)
        networkCallback?.let { callback -> runCatching { manager?.unregisterNetworkCallback(callback) } }
        networkCallback = null
        lastUnderlyingNetwork = null
        pendingUnderlyingNetwork = null
    }

    /**
     * Returns the physical network used by the app process, never the VPN network
     * created by this service. Keeping the previous validated network preferred
     * also prevents Wi-Fi and cellular callbacks from making the tunnel oscillate.
     */
    private fun isUsableUnderlyingNetwork(
        manager: ConnectivityManager,
        network: Network
    ): Boolean {
        val capabilities = manager.getNetworkCapabilities(network) ?: return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) &&
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
    }

    private fun currentUnderlyingNetwork(
        manager: ConnectivityManager
    ): Network? {
        return manager.activeNetwork?.takeIf {
            isUsableUnderlyingNetwork(manager, it)
        } ?: lastUnderlyingNetwork?.takeIf {
            isUsableUnderlyingNetwork(manager, it)
        } ?: pendingUnderlyingNetwork?.takeIf {
            isUsableUnderlyingNetwork(manager, it)
        }
    }

    private fun onDefaultNetworkMayHaveChanged(candidate: Network? = null) {
        if (activeParams == null || restartingForNetwork) return
        if (candidate != null) pendingUnderlyingNetwork = candidate
        restartJob?.cancel()
        restartJob = serviceScope.launch {
            // Multiple onAvailable/onCapabilitiesChanged callbacks describe one
            // transition. Debounce them before deciding whether the route changed.
            delay(NETWORK_SETTLE_DELAY_MS)
            val manager = getSystemService(ConnectivityManager::class.java) ?: return@launch
            val network = currentUnderlyingNetwork(manager) ?: return@launch
            if (network == lastUnderlyingNetwork) return@launch
            val params = activeParams ?: return@launch
            lastUnderlyingNetwork = network
            pendingUnderlyingNetwork = null
            VpnLogBus.info("VPN", "Underlying network changed; re-establishing the tunnel once")
            restartingForNetwork = true
            _isStarting.value = true
            try {
                _isRunning.value = false
                notifyWidgets()
                var lastError: Throwable? = null
                repeat(NETWORK_RESTART_ATTEMPTS) { attempt ->
                    val result = runCatching { bringUpRuntime(params) }
                    if (result.isSuccess) {
                        VpnLogBus.info("VPN", "Tunnel restored on the new network")
                        return@launch
                    }
                    lastError = result.exceptionOrNull()
                    VpnLogBus.warning(
                        "VPN",
                        "Reconnect ${attempt + 1}/$NETWORK_RESTART_ATTEMPTS failed: ${lastError?.message}"
                    )
                    delay(NETWORK_RESTART_RETRY_DELAY_MS * (attempt + 1))
                }
                VpnLogBus.error(
                    "VPN",
                    "Could not restore the tunnel after the network changed: ${lastError?.message}"
                )
                clearSessionState()
                stopRuntime(closeInterface = true)
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf(lastStartId)
            } finally {
                restartingForNetwork = false
                _isStarting.value = false
            }
        }
    }

    private fun onCoreExit(name: String, code: Int) {
        // A planned teardown during a reconnect is not a crash.
        if (restartingForNetwork) return
        if (!_isRunning.value && vpnInterface == null) {
            if (_isStarting.value) {
                VpnLogBus.error("CORE", "$name stopped during connection setup (exit code $code)")
            }
            return
        }
        VpnLogBus.error("CORE", "$name stopped unexpectedly (exit code $code)")
        _isRunning.value = false
        clearSessionState()
        serviceScope.launch {
            stopRuntime(closeInterface = true)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf(lastStartId)
        }
    }

    private fun observeEngineState() {
        stateJob?.cancel()
        stateJob = serviceScope.launch {
            engineManager.state.collect { state ->
                when (state) {
                    // The notification is driven by the traffic tick, not by this
                    // flow: a StateFlow only emits on change, so with the speed
                    // readout off nothing ever re-rendered and a toggled setting
                    // never reached the shade. Cancelling it here was worse still —
                    // cancel() is a no-op on a foreground service notification, so
                    // "show notification = off" just froze it instead.
                    is EngineState.Running -> Unit
                    is EngineState.Error -> {
                        Log.e(TAG, "Engine error: ${state.message}")
                        VpnLogBus.error("CORE", state.message)
                    }
                    else -> Unit
                }
            }
        }
    }

    private fun ensureSessionStarted() {
        val prefs = getSharedPreferences("lumen_prefs", MODE_PRIVATE)
        if (prefs.getLong("session_started_at", 0L) <= 0L) {
            prefs.edit().putLong("session_started_at", System.currentTimeMillis()).apply()
            sessionUploaded = 0L
            sessionDownloaded = 0L
        }
    }

    private fun clearSessionState() {
        getSharedPreferences("lumen_prefs", MODE_PRIVATE)
            .edit()
            .remove("session_started_at")
            .apply()
        sessionUploaded = 0L
        sessionDownloaded = 0L
        _trafficStats.value = TrafficStats()
    }

    /**
     * The command-line core cannot publish libbox traffic callbacks. Android's
     * per-UID counters cover the bundled sing-box process without including
     * unrelated device traffic, so deltas provide stable bytes/second for both
     * the dashboard and foreground notification.
     */
    private fun startTrafficStats() {
        trafficStatsJob?.cancel()
        trafficStatsJob = serviceScope.launch {
            val uid = Process.myUid()
            var lastRx = AndroidTrafficStats.getUidRxBytes(uid)
            var lastTx = AndroidTrafficStats.getUidTxBytes(uid)
            while (true) {
                delay(1_000)
                val rx = AndroidTrafficStats.getUidRxBytes(uid)
                val tx = AndroidTrafficStats.getUidTxBytes(uid)
                val rxDelta = if (lastRx >= 0L && rx >= lastRx) rx - lastRx else 0L
                val txDelta = if (lastTx >= 0L && tx >= lastTx) tx - lastTx else 0L
                lastRx = rx
                lastTx = tx

                val enabled = getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, MODE_PRIVATE)
                    .getBoolean(NotificationHelper.PREF_SPEED_STATS, true)
                val stats = if (enabled) {
                    sessionDownloaded += rxDelta
                    sessionUploaded += txDelta
                    TrafficStats(
                        uploadSpeed = txDelta,
                        downloadSpeed = rxDelta,
                        totalUploaded = sessionUploaded,
                        totalDownloaded = sessionDownloaded
                    )
                } else {
                    TrafficStats(
                        totalUploaded = sessionUploaded,
                        totalDownloaded = sessionDownloaded
                    )
                }
                _trafficStats.value = stats
                engineManager.updateTrafficStats(stats)
                // The only steady clock the notification has. updateNotification
                // re-posts solely when the rendered result changed, so this both
                // keeps the speeds current and lets a setting toggled in the app
                // take effect within a second without any churn in between.
                if (_isRunning.value) {
                    NotificationHelper.updateNotification(this@LumenVpnService, stats, true)
                }
            }
        }
    }

    private fun stopVpn() {
        // stopSelf() only stops when the id is the most recent one, so the teardown
        // must name the start it is ending. Reading lastStartId once the coroutine
        // runs would name the reconnect that arrived in the meantime and kill it.
        val stoppingStartId = lastStartId
        // A user initiated stop must not be undone by a pending network reconnect.
        unregisterNetworkMonitor()
        startJob?.cancel()
        startJob = null
        restartJob?.cancel()
        restartJob = null
        restartingForNetwork = false
        _isStarting.value = false
        activeParams = null
        clearSessionState()
        serviceScope.launch {
            stopRuntime(closeInterface = true)
            stopForeground(STOP_FOREGROUND_REMOVE)
            // stopForeground alone leaves the notification behind on some ROMs when
            // the service is not destroyed straight away.
            NotificationHelper.cancel(this@LumenVpnService)
            stopSelf(stoppingStartId)
        }
    }

    /**
     * Starts the loopback obfs2/obfs3 transport when the selected OpenVPN profile
     * routes through an obfsproxy bridge. Plain http/socks proxies need no relay:
     * the core dials them directly through its detour outbound.
     */
    private fun startObfsRelay(params: StartParams) {
        stopObfsRelay()
        if (params.obfsType.isBlank() || params.obfsHost.isBlank() || params.obfsPort !in 1..65535) return
        val relay = ObfsRelay(
            localPort = OBFS_LOCAL_PORT,
            type = params.obfsType,
            bridgeHost = params.obfsHost,
            bridgePort = params.obfsPort,
            protect = { socket -> protect(socket) }
        )
        runCatching { relay.start() }
            .onSuccess { obfsRelay = relay }
            .onFailure { VpnLogBus.error("OBFS", "Failed to start the ${params.obfsType} relay", it) }
    }

    private fun stopObfsRelay() {
        obfsRelay?.let { runCatching { it.stop() } }
        obfsRelay = null
    }

    private suspend fun stopRuntime(closeInterface: Boolean) = runtimeMutex.withLock {
        stopRuntimeLocked(closeInterface)
    }

    private suspend fun stopRuntimeLocked(closeInterface: Boolean) {
        stopObfsRelay()
        trafficStatsJob?.cancel()
        trafficStatsJob = null
        stateJob?.cancel()
        stateJob = null
        if (tunnelStarted.compareAndSet(true, false)) {
            VpnLogBus.info("TUN2SOCKS", "Stopping native tunnel")
            runCatching { TProxyService.TProxyStopService() }
                .onFailure { VpnLogBus.warning("TUN2SOCKS", "Stop failed: ${it.message}") }
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
        stopObfsRelay()
        unregisterNetworkMonitor()
        restartJob?.cancel()
        restartJob = null
        startJob?.cancel()
        startJob = null
        restartingForNetwork = false
        _isStarting.value = false
        activeParams = null
        clearSessionState()
        trafficStatsJob?.cancel()
        trafficStatsJob = null
        stateJob?.cancel()
        if (tunnelStarted.compareAndSet(true, false)) {
            runCatching { TProxyService.TProxyStopService() }
        }
        tunnelJob?.cancel()
        tunnelJob = null
        // onDestroy runs on the main thread and a system initiated destroy (revoked
        // consent, another VPN app) can land while a start still holds the engine
        // mutex, so never wait for the shutdown here.
        val manager = engineManager
        thread(name = "lumen-engine-shutdown") {
            runBlocking { runCatching { manager.stopEngine() } }
        }
        runCatching { vpnInterface?.close() }
        vpnInterface = null
        _isRunning.value = false
        _notificationsBlocked.value = false
        NotificationHelper.cancel(this)
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
        // AWG/WireGuard needs a handshake before the first probe can succeed.
        private const val PROXY_PROBE_ATTEMPTS = 4
        private const val PROXY_PROBE_RETRY_DELAY_MS = 1_200L
        // Losing the local port to a core that is still shutting down is retried,
        // never reported as a dead connection.
        private const val CORE_START_ATTEMPTS = 3
        private const val CORE_START_RETRY_DELAY_MS = 600L
        private const val SOCKS_READY_ATTEMPTS = 50
        // Reconnect behaviour after the default network changed.
        private const val NETWORK_SETTLE_DELAY_MS = 900L
        private const val NETWORK_RESTART_ATTEMPTS = 3
        private const val NETWORK_RESTART_RETRY_DELAY_MS = 1_500L
        // Kept as a literal (and not a reference to the app module) because
        // :core:vpn must not depend on :app.
        const val WIDGET_UPDATE_ACTION = "com.lumen.app.widget.ACTION_UPDATE_STATE"
        const val ACTION_START_VPN = "com.lumen.core.vpn.START_VPN"
        const val ACTION_STOP_VPN = "com.lumen.core.vpn.STOP_VPN"
        const val EXTRA_ENGINE_TYPE = "extra_engine_type"
        const val EXTRA_CONFIG_JSON = "extra_config_json"
        const val EXTRA_CONFIG_PATH = "extra_config_path"
        const val EXTRA_SPLIT_MODE = "extra_split_mode"
        const val EXTRA_SPLIT_PACKAGES = "extra_split_packages"
        const val EXTRA_MTU = "extra_mtu"
        const val EXTRA_LOCAL_SOCKS_PORT = "extra_local_socks_port"
        const val EXTRA_DNS_MODE = "extra_dns_mode"
        const val EXTRA_RECONNECT_ON_NETWORK_CHANGE = "extra_reconnect_on_network_change"
        const val EXTRA_OBFS_TYPE = "extra_obfs_type"
        const val EXTRA_OBFS_HOST = "extra_obfs_host"
        const val EXTRA_OBFS_PORT = "extra_obfs_port"
        // Must match SingboxConfigBuilder.OBFS_LOCAL_PORT.
        const val OBFS_LOCAL_PORT = 10871
        const val DEFAULT_IPV4_ADDRESS = "172.19.0.1"
        const val DEFAULT_IPV4_PREFIX = 30
        const val DEFAULT_IPV6_ADDRESS = "fdfe:dcba:9876::1"
        const val DEFAULT_IPV6_PREFIX = 126
        const val DEFAULT_MTU = 1500
        const val INTERNAL_DNS_ADDRESS = "172.19.0.2"
        val DEFAULT_DNS_SERVERS = listOf("1.1.1.1", "8.8.8.8")
        private val _isRunning = MutableStateFlow(false)
        val isRunning: StateFlow<Boolean> = _isRunning.asStateFlow()
        private val _isStarting = MutableStateFlow(false)
        val isStarting: StateFlow<Boolean> = _isStarting.asStateFlow()
        private val _trafficStats = MutableStateFlow(TrafficStats())
        val trafficStats: StateFlow<TrafficStats> = _trafficStats.asStateFlow()
        private val _notificationsBlocked = MutableStateFlow(false)

        /**
         * True while the tunnel runs but Android will not show its notification,
         * which on 13+ is the ordinary outcome of denying POST_NOTIFICATIONS. The
         * service keeps working, so nothing else makes the state visible.
         */
        val notificationsBlocked: StateFlow<Boolean> = _notificationsBlocked.asStateFlow()
    }
}
