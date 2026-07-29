package com.lumen.core.vpn

import android.content.Context
import android.content.Intent
import java.io.File
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket

/**
 * Every parameter [LumenVpnService] reads from an ACTION_START_VPN intent.
 *
 * The app, the home screen widget and the quick settings tile all start the
 * service, but only the app can generate a config. The values are therefore
 * mirrored into prefs by whoever built the config and read back by the surfaces
 * that cannot, so a tile or widget start no longer drops the DNS mode, the
 * local SOCKS port, the reconnect flag or the obfs bridge.
 *
 * [configJson] is the one value that never travels in the intent: see
 * [VpnConfigStore]. Only [configPath] does.
 */
data class VpnStartParams(
    val configJson: String = "",
    val configPath: String = "",
    val engineType: String = VpnStartIntentFactory.DEFAULT_ENGINE_TYPE,
    val splitMode: String = VpnStartIntentFactory.DEFAULT_SPLIT_MODE,
    val splitPackages: Set<String> = emptySet(),
    val mtu: Int = LumenVpnService.DEFAULT_MTU,
    val localSocksPort: Int = VpnStartIntentFactory.DEFAULT_LOCAL_SOCKS_PORT,
    val dnsMode: String = VpnStartIntentFactory.DEFAULT_DNS_MODE,
    val reconnectOnNetworkChange: Boolean = true,
    val obfsType: String = "",
    val obfsHost: String = "",
    val obfsPort: Int = 0
)

/**
 * The generated sing-box config, kept out of every Binder transaction.
 *
 * An imported AUTO pool with 300+ members serialises to hundreds of kilobytes and
 * Kotlin strings parcel as UTF-16, so passing it as an Intent extra pushed
 * startForegroundService() past the ~1 MB Binder limit and killed the app with
 * TransactionTooLargeException. SharedPreferences is no better a carrier: it is
 * read and written synchronously and the whole file stays resident. The config
 * therefore lives in one private file and only its path is passed around.
 *
 * Everything here touches the disk and must run off the main thread, except
 * [configFile] and [hasStoredConfig], which only stat.
 */
object VpnConfigStore {
    private const val DIR_NAME = "vpn"
    private const val FILE_NAME = "active-config.json"
    private const val TMP_NAME = "$FILE_NAME.tmp"

    /** Nothing shorter can be a config; matches [VpnStartIntentFactory.isUsableConfig]. */
    private const val MIN_CONFIG_BYTES = 10L

    /** No-backup storage: the config carries every server's credentials. */
    fun configFile(context: Context): File =
        File(File(context.noBackupFilesDir, DIR_NAME), FILE_NAME)

    fun hasStoredConfig(context: Context): Boolean =
        runCatching { configFile(context).length() >= MIN_CONFIG_BYTES }.getOrDefault(false)

    /** Replaces the stored config atomically and returns its path. */
    fun write(context: Context, configJson: String): String {
        val target = configFile(context)
        val dir = target.parentFile
        if (dir != null && !dir.isDirectory) dir.mkdirs()
        dir?.let(::restrictToOwner)
        // Write beside the target and rename: a start that races this must never
        // read half a config, and a crash must leave the previous one intact.
        val tmp = File(dir, TMP_NAME)
        tmp.writeText(configJson)
        restrictToOwner(tmp)
        if (!tmp.renameTo(target)) {
            target.delete()
            check(tmp.renameTo(target)) { "Could not replace $FILE_NAME" }
        }
        restrictToOwner(target)
        return target.absolutePath
    }

    /** Returns an empty string when nothing was stored; never throws. */
    fun read(context: Context, path: String = ""): String {
        val file = resolve(context, path)
        val stored = runCatching { if (file.isFile) file.readText() else "" }.getOrDefault("")
        return stored.ifBlank { migrateFromPrefs(context) }
    }

    fun clear(context: Context) {
        runCatching { configFile(context).delete() }
        context.getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .remove(VpnStartIntentFactory.KEY_CONFIG_JSON)
            .remove(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON)
            .remove(VpnStartIntentFactory.KEY_CONFIG_DIRTY)
            .apply()
    }

    /**
     * Only our own storage is honoured; a start intent never names a foreign file.
     * The parent is compared exactly, because a prefix test also accepts a sibling
     * directory whose name merely starts with ours.
     */
    private fun resolve(context: Context, path: String): File {
        val canonical = configFile(context)
        if (path.isBlank()) return canonical
        val dir = canonical.parentFile?.absolutePath ?: return canonical
        val requested = File(path)
        return if (requested.parentFile?.absolutePath == dir) requested else canonical
    }

    /**
     * Older builds kept the whole config in SharedPreferences. Move it across the
     * first time the service needs it, so the multi-hundred-KB string stops being
     * loaded into memory together with every other preference.
     */
    private fun migrateFromPrefs(context: Context): String {
        val prefs = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        )
        val legacy = prefs.getString(VpnStartIntentFactory.KEY_CONFIG_JSON, null)
            ?: prefs.getString(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON, null)
        if (legacy.isNullOrBlank()) return ""
        runCatching { write(context, legacy) }
        prefs.edit()
            .remove(VpnStartIntentFactory.KEY_CONFIG_JSON)
            .remove(VpnStartIntentFactory.KEY_LEGACY_CONFIG_JSON)
            .apply()
        return legacy
    }

    /** Nothing outside our UID may read the credentials, whatever the ROM's umask is. */
    private fun restrictToOwner(file: File) {
        runCatching {
            file.setReadable(false, false)
            file.setWritable(false, false)
            file.setExecutable(false, false)
            file.setReadable(true, true)
            file.setWritable(true, true)
            if (file.isDirectory) file.setExecutable(true, true)
        }
    }
}

/**
 * Which local SOCKS port the core actually listens on for one session.
 *
 * The port configured in the settings is always preferred and is only abandoned
 * when something still holds it. On a reconnect or a server switch that
 * something is the previous session's core: a listening socket stays alive until
 * its owner is fully reaped, and the new core then dies with
 * "bind: address already in use" before the tunnel ever comes up.
 *
 * The resolved port is written back into the generated config, so the inbound,
 * the tun2socks side and the readiness probe can never disagree about it.
 */
object LocalSocksPort {
    private const val INBOUND_TAG = "socks-in"
    /** The optional local HTTP inbound races for its port exactly like the SOCKS one. */
    const val HTTP_INBOUND_TAG = "http-in"
    private const val MIN_PORT = 1024
    private const val MAX_PORT = 65535
    private const val ALTERNATIVE_ATTEMPTS = 8
    private val LISTEN_PORT = Regex("\"listen_port\"\\s*:\\s*\\d+")

    /** Bind-tests the port; a port held by a dying core reads as taken. */
    fun isFree(port: Int): Boolean = runCatching {
        ServerSocket().use { it.bind(InetSocketAddress("127.0.0.1", port), 1) }
        true
    }.getOrDefault(false)

    private fun ephemeralPort(): Int = runCatching {
        ServerSocket(0, 1, InetAddress.getByName("127.0.0.1")).use { it.localPort }
    }.getOrDefault(0)

    fun resolve(preferred: Int): Int = resolve(preferred, ::isFree, ::ephemeralPort)

    /** Both probes are parameters so the decision itself can be unit tested. */
    fun resolve(preferred: Int, isFree: (Int) -> Boolean, ephemeral: () -> Int): Int {
        val wanted = preferred.coerceIn(MIN_PORT, MAX_PORT)
        if (isFree(wanted)) return wanted
        repeat(ALTERNATIVE_ATTEMPTS) {
            val candidate = ephemeral()
            if (candidate in MIN_PORT..MAX_PORT && candidate != wanted) return candidate
        }
        // Nothing better was found; let the core report the conflict itself.
        return wanted
    }

    /**
     * The port an inbound is configured to listen on, or null when the config
     * carries no such inbound. Lets a caller conflict-check a port it was never
     * told about, like the optional local HTTP one.
     */
    fun portOf(configJson: String, tag: String = INBOUND_TAG): Int? {
        var searchFrom = 0
        while (true) {
            val tagAt = configJson.indexOf("\"$tag\"", searchFrom)
            if (tagAt < 0) return null
            searchFrom = tagAt + tag.length
            val start = configJson.lastIndexOf('{', tagAt)
            val end = configJson.indexOf('}', tagAt)
            if (start < 0 || end < 0) continue
            LISTEN_PORT.find(configJson.substring(start, end))
                ?.value?.substringAfterLast(':')?.trim()?.toIntOrNull()
                ?.let { return it }
        }
    }

    /**
     * Rewrites an inbound's listen_port in a generated config. Returns the config
     * unchanged when it carries no such inbound.
     */
    fun applyToConfig(configJson: String, port: Int, tag: String = INBOUND_TAG): String {
        var searchFrom = 0
        while (true) {
            val tagAt = configJson.indexOf("\"$tag\"", searchFrom)
            if (tagAt < 0) return configJson
            searchFrom = tagAt + tag.length
            val start = configJson.lastIndexOf('{', tagAt)
            val end = configJson.indexOf('}', tagAt)
            if (start < 0 || end < 0) continue
            val block = configJson.substring(start, end)
            val rewritten = LISTEN_PORT.replace(block, "\"listen_port\": $port")
            // The tag also appears in route rules, which carry no listen_port.
            if (rewritten == block) continue
            return configJson.substring(0, start) + rewritten + configJson.substring(end)
        }
    }
}

object VpnStartIntentFactory {
    const val PREFS_NAME = "lumen_prefs"
    const val KEY_CONFIG_JSON = "active_config_json"
    const val KEY_LEGACY_CONFIG_JSON = "config_json"
    const val KEY_CONFIG_DIRTY = "active_config_dirty"
    const val KEY_ENGINE_TYPE = "engine_type"
    const val KEY_SPLIT_MODE = "split_mode"
    const val KEY_SPLIT_PACKAGES = "split_packages"
    const val KEY_MTU = "mtu"
    const val KEY_LOCAL_SOCKS_PORT = "local_socks_port"
    const val KEY_DNS_MODE = "dns_mode"
    const val KEY_RECONNECT_ON_NETWORK_CHANGE = "reconnect_on_network_change"
    const val KEY_OBFS_TYPE = "obfs_type"
    const val KEY_OBFS_HOST = "obfs_host"
    const val KEY_OBFS_PORT = "obfs_port"

    const val DEFAULT_ENGINE_TYPE = "SINGBOX"
    const val DEFAULT_SPLIT_MODE = "DISABLED"
    const val DEFAULT_DNS_MODE = "automatic"
    const val DEFAULT_LOCAL_SOCKS_PORT = 10808

    /** Key plus Parcel bookkeeping per extra; deliberately generous. */
    private const val EXTRA_OVERHEAD_BYTES = 64

    /**
     * The same check [LumenVpnService] applies before it starts, so the widget and
     * the tile can open the app instead of starting a service that stops itself.
     */
    fun isUsableConfig(configJson: String?): Boolean =
        !configJson.isNullOrBlank() && configJson != "{}" &&
            configJson.length >= 10 && configJson.contains("outbound")

    /**
     * Main-thread safe answer to "is there something to connect to". The widget and
     * the tile used to ask [isUsableConfig] with the config loaded out of prefs;
     * this stats the stored file instead of materialising hundreds of kilobytes.
     */
    fun hasUsableConfig(context: Context): Boolean {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        // A node or routing setting changed after this file was generated. Keep the
        // previous file intact for the active service, but never let a tile/widget
        // start a new tunnel from stale credentials or routing rules.
        if (prefs.getBoolean(KEY_CONFIG_DIRTY, false)) return false
        if (VpnConfigStore.hasStoredConfig(context)) return true
        // Upgrade path: prefs still hold the config until the service migrates it.
        return isUsableConfig(prefs.getString(KEY_CONFIG_JSON, null)) ||
            isUsableConfig(prefs.getString(KEY_LEGACY_CONFIG_JSON, null))
    }

    /**
     * Prevents background entry points from starting a stale generated config while
     * preserving the file currently owned by a running VPN service.
     */
    fun markConfigDirty(context: Context) {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_CONFIG_DIRTY, true)
            .commit()
    }

    /**
     * Writes the config to its private file and mirrors the small parameters into
     * prefs. Blocking IO: call it off the main thread. Returns the config path.
     */
    fun persistStartParams(context: Context, params: VpnStartParams): String {
        val path = VpnConfigStore.write(context, params.configJson)
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            // The config is a file now, not a preference; drop any leftover copy.
            .remove(KEY_CONFIG_JSON)
            .remove(KEY_LEGACY_CONFIG_JSON)
            .putBoolean(KEY_CONFIG_DIRTY, false)
            .putString(KEY_ENGINE_TYPE, params.engineType)
            .putString(KEY_SPLIT_MODE, params.splitMode)
            .putStringSet(KEY_SPLIT_PACKAGES, params.splitPackages)
            .putInt(KEY_MTU, params.mtu)
            .putInt(KEY_LOCAL_SOCKS_PORT, params.localSocksPort)
            .putString(KEY_DNS_MODE, params.dnsMode)
            .putBoolean(KEY_RECONNECT_ON_NETWORK_CHANGE, params.reconnectOnNetworkChange)
            .putString(KEY_OBFS_TYPE, params.obfsType)
            .putString(KEY_OBFS_HOST, params.obfsHost)
            .putInt(KEY_OBFS_PORT, params.obfsPort)
            .commit()
        return path
    }

    /**
     * Deliberately leaves [VpnStartParams.configJson] empty: the widget and the tile
     * call this on the main thread. Use [hasUsableConfig] to test for a config and
     * let the service read the file on its own IO thread.
     */
    fun startParamsFromPrefs(context: Context): VpnStartParams {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        return VpnStartParams(
            configPath = VpnConfigStore.configFile(context).absolutePath,
            engineType = prefs.getString(KEY_ENGINE_TYPE, null) ?: DEFAULT_ENGINE_TYPE,
            splitMode = prefs.getString(KEY_SPLIT_MODE, null) ?: DEFAULT_SPLIT_MODE,
            splitPackages = prefs.getStringSet(KEY_SPLIT_PACKAGES, emptySet()) ?: emptySet(),
            mtu = prefs.getInt(KEY_MTU, LumenVpnService.DEFAULT_MTU),
            localSocksPort = prefs.getInt(KEY_LOCAL_SOCKS_PORT, DEFAULT_LOCAL_SOCKS_PORT),
            dnsMode = prefs.getString(KEY_DNS_MODE, null) ?: DEFAULT_DNS_MODE,
            reconnectOnNetworkChange = prefs.getBoolean(KEY_RECONNECT_ON_NETWORK_CHANGE, true),
            obfsType = prefs.getString(KEY_OBFS_TYPE, null).orEmpty(),
            obfsHost = prefs.getString(KEY_OBFS_HOST, null).orEmpty(),
            obfsPort = prefs.getInt(KEY_OBFS_PORT, 0)
        )
    }

    fun buildStartIntent(context: Context, params: VpnStartParams): Intent =
        Intent(context, LumenVpnService::class.java).apply {
            action = LumenVpnService.ACTION_START_VPN
            // Never the config itself: see VpnConfigStore.
            putExtra(LumenVpnService.EXTRA_CONFIG_PATH, configPathOf(context, params))
            putExtra(LumenVpnService.EXTRA_ENGINE_TYPE, params.engineType)
            putExtra(LumenVpnService.EXTRA_SPLIT_MODE, params.splitMode)
            putStringArrayListExtra(
                LumenVpnService.EXTRA_SPLIT_PACKAGES,
                ArrayList(params.splitPackages)
            )
            putExtra(LumenVpnService.EXTRA_MTU, params.mtu.coerceIn(1280, 9000))
            putExtra(
                LumenVpnService.EXTRA_LOCAL_SOCKS_PORT,
                params.localSocksPort.coerceIn(1024, 65535)
            )
            putExtra(LumenVpnService.EXTRA_DNS_MODE, params.dnsMode)
            putExtra(
                LumenVpnService.EXTRA_RECONNECT_ON_NETWORK_CHANGE,
                params.reconnectOnNetworkChange
            )
            // Plain http/socks proxies need no relay: only obfs2/obfs3 bridges are
            // terminated in-app, and an incomplete triple would start a dead relay.
            if (params.obfsType.isNotBlank() && params.obfsHost.isNotBlank() &&
                params.obfsPort in 1..65535
            ) {
                putExtra(LumenVpnService.EXTRA_OBFS_TYPE, params.obfsType)
                putExtra(LumenVpnService.EXTRA_OBFS_HOST, params.obfsHost)
                putExtra(LumenVpnService.EXTRA_OBFS_PORT, params.obfsPort)
            }
        }

    /**
     * Upper bound, in bytes, on everything [buildStartIntent] puts into the Binder
     * transaction. Kotlin strings parcel as UTF-16, so a config carried as an extra
     * cost two bytes per character and a 300 member AUTO pool went past the ~1 MB
     * limit. Nothing here may grow with the pool, which the unit test pins down.
     */
    fun startIntentPayloadBytes(context: Context, params: VpnStartParams): Int {
        val strings = listOf(
            LumenVpnService.ACTION_START_VPN,
            configPathOf(context, params),
            params.engineType,
            params.splitMode,
            params.dnsMode,
            params.obfsType,
            params.obfsHost
        ) + params.splitPackages
        return strings.sumOf { it.length * 2 + EXTRA_OVERHEAD_BYTES } +
            4 * (Int.SIZE_BYTES + EXTRA_OVERHEAD_BYTES)
    }

    private fun configPathOf(context: Context, params: VpnStartParams): String =
        params.configPath.ifBlank { VpnConfigStore.configFile(context).absolutePath }

    fun buildStopIntent(context: Context): Intent =
        Intent(context, LumenVpnService::class.java).apply {
            action = LumenVpnService.ACTION_STOP_VPN
        }
}
