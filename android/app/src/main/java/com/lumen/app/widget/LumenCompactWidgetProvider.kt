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
import com.lumen.core.vpn.LumenVpnService
import net.kramb.lumen.R

/**
 * 1x1 home screen widget: a single power button with no labels. Toggling is
 * delegated to [LumenWidgetProvider] so both widgets share the same logic.
 */
class LumenCompactWidgetProvider : AppWidgetProvider() {

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (appWidgetId in appWidgetIds) {
            updateWidget(context, appWidgetManager, appWidgetId)
        }
    }

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        refreshAll(context)
    }

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        when (intent.action) {
            LumenWidgetProvider.ACTION_UPDATE_STATE,
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED -> refreshAll(context)
        }
    }

    companion object {

        fun refreshAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(
                ComponentName(context, LumenCompactWidgetProvider::class.java)
            )
            for (id in ids) {
                updateWidget(context, manager, id)
            }
        }

        private fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
        ) {
            val views = RemoteViews(context.packageName, R.layout.widget_lumen_compact)
            val prefs = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
            val isRunning = LumenVpnService.isRunning.value

            val toggleIntent = Intent(context, LumenWidgetProvider::class.java).apply {
                action = LumenWidgetProvider.ACTION_TOGGLE_VPN
            }
            val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            } else {
                PendingIntent.FLAG_UPDATE_CURRENT
            }
            val pendingIntent = PendingIntent.getBroadcast(context, 1, toggleIntent, flags)
            views.setOnClickPendingIntent(R.id.widget_container, pendingIntent)
            views.setOnClickPendingIntent(R.id.widget_button_bg, pendingIntent)
            views.setOnClickPendingIntent(R.id.widget_button_icon, pendingIntent)

            val preset = prefs.getString("theme_preset", "DARK") ?: "DARK"
            val amoled = prefs.getBoolean("use_amoled_black", false)
            views.setInt(
                R.id.widget_container,
                "setBackgroundResource",
                if (amoled) R.drawable.widget_tile_bg_amoled else R.drawable.widget_tile_bg
            )

            if (isRunning) {
                views.setInt(R.id.widget_button_bg, "setColorFilter", Color.parseColor("#00E676"))
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_pause)
            } else {
                views.setInt(
                    R.id.widget_button_bg,
                    "setColorFilter",
                    LumenWidgetProvider.accentColorFor(preset)
                )
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_power)
            }

            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }
}
