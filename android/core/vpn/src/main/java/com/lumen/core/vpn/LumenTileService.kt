package com.lumen.core.vpn

import android.app.PendingIntent
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class LumenTileService : TileService() {

    private val serviceScope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private var listeningJob: Job? = null

    override fun onStartListening() {
        super.onStartListening()
        updateTileState()
        listeningJob?.cancel()
        listeningJob = serviceScope.launch {
            LumenVpnService.isRunning.collect { isRunning ->
                updateTileState(isRunning)
            }
        }
    }

    override fun onStopListening() {
        super.onStopListening()
        listeningJob?.cancel()
        listeningJob = null
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceScope.cancel()
    }

    override fun onClick() {
        super.onClick()
        val isRunning = LumenVpnService.isRunning.value || LumenVpnService.isStarting.value
        if (isRunning) {
            if (startVpnService(VpnStartIntentFactory.buildStopIntent(this))) {
                updateTileState(false)
            }
            return
        }
        // VPN consent and a generated config only exist in the app; starting
        // without either leaves the tile ACTIVE while the service fails silently.
        val params = VpnStartIntentFactory.startParamsFromPrefs(this)
        // hasUsableConfig stats the stored file; onClick runs on the main thread and
        // params never carries the config itself. See VpnConfigStore.
        if (!VpnStartIntentFactory.hasUsableConfig(this) ||
            VpnService.prepare(this) != null
        ) {
            openApp()
            return
        }
        if (startVpnService(VpnStartIntentFactory.buildStartIntent(this, params))) {
            updateTileState(true)
        } else {
            openApp()
        }
    }

    private fun startVpnService(intent: Intent): Boolean = runCatching {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }.isSuccess

    private fun openApp() {
        val launch = packageManager.getLaunchIntentForPackage(packageName)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            val pending = PendingIntent.getActivity(
                this,
                0,
                launch,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            runCatching { startActivityAndCollapse(pending) }
        } else {
            @Suppress("DEPRECATION")
            runCatching { startActivityAndCollapse(launch) }
        }
    }

    private fun updateTileState(overrideRunningState: Boolean? = null) {
        val tile = qsTile ?: return
        val isRunning = overrideRunningState
            ?: (LumenVpnService.isRunning.value || LumenVpnService.isStarting.value)

        tile.state = if (isRunning) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "Lumen VPN"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            tile.subtitle = when {
                LumenVpnService.isRunning.value -> "Connected"
                LumenVpnService.isStarting.value -> "Connecting"
                else -> "Disconnected"
            }
        }
        tile.updateTile()
        // Must be explicit, otherwise Android 8+ drops it before it reaches the
        // manifest declared widget receivers.
        runCatching {
            sendBroadcast(
                Intent(LumenVpnService.WIDGET_UPDATE_ACTION).setPackage(packageName)
            )
        }
    }
}
