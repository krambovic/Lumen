package com.lumen.core.vpn

import android.content.Context
import android.content.Intent
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
        val isRunning = LumenVpnService.isRunning.value
        val intent = Intent(this, LumenVpnService::class.java).apply {
            action = if (isRunning) LumenVpnService.ACTION_STOP_VPN else LumenVpnService.ACTION_START_VPN
            if (!isRunning) {
                val prefs = getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
                val configJson = prefs.getString("active_config_json", null)
                    ?: prefs.getString("config_json", null) ?: "{}"
                val engineType = prefs.getString("engine_type", null) ?: "SINGBOX"
                val splitMode = prefs.getString("split_mode", null) ?: "DISABLED"
                val splitPackagesSet = prefs.getStringSet("split_packages", emptySet()) ?: emptySet()
                val mtu = prefs.getInt("mtu", 1500)

                putExtra(LumenVpnService.EXTRA_CONFIG_JSON, configJson)
                putExtra(LumenVpnService.EXTRA_ENGINE_TYPE, engineType)
                putExtra(LumenVpnService.EXTRA_SPLIT_MODE, splitMode)
                putExtra(LumenVpnService.EXTRA_SPLIT_PACKAGES, ArrayList(splitPackagesSet))
                putExtra(LumenVpnService.EXTRA_MTU, mtu)
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }

        updateTileState(!isRunning)
    }

    private fun updateTileState(overrideRunningState: Boolean? = null) {
        val tile = qsTile ?: return
        val isRunning = overrideRunningState ?: LumenVpnService.isRunning.value

        tile.state = if (isRunning) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "Lumen VPN"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            tile.subtitle = if (isRunning) "Connected" else "Disconnected"
        }
        tile.updateTile()
        sendBroadcast(Intent("com.lumen.app.widget.ACTION_UPDATE_STATE"))
    }
}
