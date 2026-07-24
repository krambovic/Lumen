package com.lumen.app.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.widget.RemoteViews
import com.lumen.app.R
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
            views.setOnClickPendingIntent(R.id.widget_button, pendingIntent)

            if (isRunning) {
                views.setInt(R.id.widget_container, "setBackgroundResource", R.drawable.widget_bg_connected)
                views.setTextViewText(R.id.widget_status, "Connected • Active")
                views.setTextColor(R.id.widget_status, android.graphics.Color.parseColor("#00E5FF"))
            } else {
                views.setInt(R.id.widget_container, "setBackgroundResource", R.drawable.widget_bg_disconnected)
                views.setTextViewText(R.id.widget_status, "Disconnected")
                views.setTextColor(R.id.widget_status, android.graphics.Color.parseColor("#94A3B8"))
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
