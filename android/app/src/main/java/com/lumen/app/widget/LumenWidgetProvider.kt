package com.lumen.app.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.widget.RemoteViews
import net.kramb.lumen.R
import com.lumen.core.vpn.LumenVpnService

class LumenWidgetProvider : AppWidgetProvider() {

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (appWidgetId in appWidgetIds) {
            updateWidget(context, appWidgetManager, appWidgetId)
        }
    }

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        if (intent.action == ACTION_TOGGLE_VPN || intent.action == ACTION_UPDATE_STATE) {
            if (intent.action == ACTION_TOGGLE_VPN) {
                toggleVpn(context)
            }
            val appWidgetManager = AppWidgetManager.getInstance(context)
            val componentName = ComponentName(context, LumenWidgetProvider::class.java)
            val appWidgetIds = appWidgetManager.getAppWidgetIds(componentName)
            for (appWidgetId in appWidgetIds) {
                updateWidget(context, appWidgetManager, appWidgetId)
            }
        }
    }

    private fun toggleVpn(context: Context) {
        val isRunning = LumenVpnService.isRunning.value
        val intent = Intent(context, LumenVpnService::class.java).apply {
            action = if (isRunning) LumenVpnService.ACTION_STOP_VPN else LumenVpnService.ACTION_START_VPN
            if (!isRunning) {
                val prefs = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
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
            context.startForegroundService(intent)
        } else {
            context.startService(intent)
        }
    }

    companion object {
        const val ACTION_TOGGLE_VPN = "com.lumen.app.widget.ACTION_TOGGLE_VPN"
        const val ACTION_UPDATE_STATE = "com.lumen.app.widget.ACTION_UPDATE_STATE"

        fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
        ) {
            val isRunning = LumenVpnService.isRunning.value
            val views = RemoteViews(context.packageName, R.layout.widget_lumen_toggle)
            val prefs = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)

            val toggleIntent = Intent(context, LumenWidgetProvider::class.java).apply {
                action = ACTION_TOGGLE_VPN
            }
            val pendingIntentFlags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            } else {
                PendingIntent.FLAG_UPDATE_CURRENT
            }
            val pendingIntent = PendingIntent.getBroadcast(
                context, 0, toggleIntent, pendingIntentFlags
            )

            views.setOnClickPendingIntent(R.id.widget_container, pendingIntent)
            views.setOnClickPendingIntent(R.id.widget_button_bg, pendingIntent)
            views.setOnClickPendingIntent(R.id.widget_button_icon, pendingIntent)

            val themePalette = prefs.getString("theme_palette", "DEFAULT") ?: "DEFAULT"
            val palettePrimaryColor = when (themePalette) {
                "ROSE_PINE" -> Color.parseColor("#EB6F92")
                "TOKYO_NIGHT" -> Color.parseColor("#7AA2F7")
                "NORD" -> Color.parseColor("#88C0D0")
                "CYBERPUNK" -> Color.parseColor("#FF007F")
                "EMERALD" -> Color.parseColor("#10B981")
                "SUNSET" -> Color.parseColor("#F59E0B")
                "DRACULA" -> Color.parseColor("#BD93F9")
                "MONOKAI" -> Color.parseColor("#A6E22E")
                "MATERIAL_YOU" -> Color.parseColor("#3B82F6")
                else -> Color.parseColor("#0088FF") // Standard Blue / Cyan
            }

            if (isRunning) {
                // Connected: Bright Green circle, Pause icon
                views.setInt(R.id.widget_button_bg, "setColorFilter", Color.parseColor("#00E676"))
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_pause)
            } else {
                // Disconnected: Theme Palette primary color, Power icon
                views.setInt(R.id.widget_button_bg, "setColorFilter", palettePrimaryColor)
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_power)
            }

            appWidgetManager.updateAppWidget(appWidgetId, views)
        }

        fun sendUpdateBroadcast(context: Context) {
            val intent = Intent(context, LumenWidgetProvider::class.java).apply {
                action = ACTION_UPDATE_STATE
            }
            context.sendBroadcast(intent)
        }
    }
}
