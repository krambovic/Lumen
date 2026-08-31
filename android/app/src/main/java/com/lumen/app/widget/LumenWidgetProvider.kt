package com.lumen.app.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.widget.RemoteViews
import net.kramb.lumen.R
import com.lumen.core.vpn.LumenVpnService
import com.lumen.core.vpn.VpnStartIntentFactory

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

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        // The very first tile has just been placed: paint it immediately instead
        // of waiting for the first periodic update.
        refreshAll(context)
    }

    override fun onAppWidgetOptionsChanged(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        newOptions: android.os.Bundle
    ) {
        super.onAppWidgetOptionsChanged(context, appWidgetManager, appWidgetId, newOptions)
        updateWidget(context, appWidgetManager, appWidgetId)
    }

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        when (intent.action) {
            ACTION_UPDATE_STATE -> refreshAll(context)
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED -> refreshAll(context)
        }
    }

    companion object {
        const val ACTION_TOGGLE_VPN = "com.lumen.app.widget.ACTION_TOGGLE_VPN"
        const val ACTION_UPDATE_STATE = "com.lumen.app.widget.ACTION_UPDATE_STATE"

        /**
         * Explicit toggle intent aimed at the non-exported control receiver, so no
         * other app can drop the tunnel by broadcasting the action.
         */
        fun toggleIntent(context: Context): Intent =
            Intent(context, LumenWidgetControlReceiver::class.java).apply {
                action = ACTION_TOGGLE_VPN
                setPackage(context.packageName)
            }

        fun refreshAll(context: Context) {
            val appWidgetManager = AppWidgetManager.getInstance(context) ?: return
            val componentName = ComponentName(context, LumenWidgetProvider::class.java)
            for (appWidgetId in appWidgetManager.getAppWidgetIds(componentName)) {
                updateWidget(context, appWidgetManager, appWidgetId)
            }
        }

        /**
         * Accent colour per palette. Keys match ThemePreset entries written to
         * prefs under "theme_preset" by the settings screen.
         */
        /**
         * Server names used to be mirrored into prefs by builds that decoded
         * them as Latin-1, which reaches the widget as mojibake. Re-decode those
         * back to UTF-8 and strip anything still unprintable so RemoteViews never
         * renders garbage.
         */
        private fun repairName(raw: String?): String? {
            val name = raw?.trim()?.takeIf { it.isNotBlank() } ?: return null
            val looksMojibake = name.any { it in '\u0080'..'\u00FF' } &&
                name.none { it in '\u0400'..'\u04FF' }
            val fixed = if (looksMojibake) {
                runCatching { String(name.toByteArray(Charsets.ISO_8859_1), Charsets.UTF_8) }
                    .getOrDefault(name)
            } else {
                name
            }
            val cleaned = fixed
                .replace("\uFFFD", "")
                .filter { it == ' ' || !Character.isISOControl(it) }
                .trim()
            return cleaned.takeIf { it.isNotBlank() }
        }

        /** Public alias so the 1x1 widget can reuse the same palette. */
        fun accentColorFor(preset: String): Int = accentFor(preset)

        private fun accentFor(preset: String): Int = when (preset) {
            "LIGHT" -> Color.parseColor("#0A84FF")
            "DARK" -> Color.parseColor("#4C8DFF")
            "DRACULA" -> Color.parseColor("#BD93F9")
            "CATPPUCCIN" -> Color.parseColor("#CBA6F7")
            "NORD" -> Color.parseColor("#88C0D0")
            "GITHUB" -> Color.parseColor("#58A6FF")
            "GRUVBOX" -> Color.parseColor("#FE8019")
            "TOKYO_NIGHT" -> Color.parseColor("#7AA2F7")
            "MONOKAI" -> Color.parseColor("#A6E22E")
            "MATERIAL" -> Color.parseColor("#6750A4")
            "SOLARIZED" -> Color.parseColor("#2AA198")
            else -> Color.parseColor("#4C8DFF")
        }

        fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
        ) {
            val isRunning = LumenVpnService.isRunning.value
            val views = RemoteViews(context.packageName, R.layout.widget_lumen_toggle)
            val prefs = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)

            val toggleIntent = toggleIntent(context)
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

            // "theme_preset" and "use_amoled_black" are the keys the app really
            // writes; the widget used to read a key that never existed.
            val preset = prefs.getString("theme_preset", "DARK") ?: "DARK"
            val amoled = prefs.getBoolean("use_amoled_black", false)
            val accent = accentFor(preset)
            val connectedColor = Color.parseColor("#00E676")

            views.setInt(
                R.id.widget_container,
                "setBackgroundResource",
                if (amoled) R.drawable.widget_tile_bg_amoled else R.drawable.widget_tile_bg
            )

            if (isRunning) {
                views.setInt(R.id.widget_button_bg, "setColorFilter", connectedColor)
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_pause)
                views.setTextViewText(R.id.widget_state_text, context.getString(R.string.connected))
                views.setTextColor(R.id.widget_state_text, connectedColor)
            } else {
                views.setInt(R.id.widget_button_bg, "setColorFilter", accent)
                views.setImageViewResource(R.id.widget_button_icon, R.drawable.ic_power)
                views.setTextViewText(R.id.widget_state_text, context.getString(R.string.disconnected))
                views.setTextColor(R.id.widget_state_text, if (amoled) Color.WHITE else Color.parseColor("#E6E6EC"))
            }

            // Prefer the Base64/UTF-8 mirror: plain prefs strings can arrive
            // mangled in the widget process on some ROMs.
            val nameFromB64 = prefs.getString("selected_node_name_b64", null)?.let { encoded ->
                runCatching {
                    String(android.util.Base64.decode(encoded, android.util.Base64.NO_WRAP), Charsets.UTF_8)
                }.getOrNull()
            }?.takeIf { it.isNotBlank() }
            val serverName = nameFromB64
                ?: repairName(prefs.getString("selected_node_name", null))
                ?: context.getString(R.string.app_name)
            views.setTextViewText(R.id.widget_server_text, serverName)
            views.setTextColor(
                R.id.widget_server_text,
                if (amoled) Color.parseColor("#8E8E96") else Color.parseColor("#A0A0AA")
            )

            appWidgetManager.updateAppWidget(appWidgetId, views)
        }

        fun sendUpdateBroadcast(context: Context) {
            val intent = Intent(context, LumenWidgetProvider::class.java).apply {
                action = ACTION_UPDATE_STATE
            }
            runCatching { context.sendBroadcast(intent) }
        }
    }
}

/**
 * Tunnel control for both home screen widgets. Declared android:exported="false":
 * the widget PendingIntents are explicit and carry our own identity, so they still
 * reach it, while no third party app can start or stop the VPN by broadcast.
 */
class LumenWidgetControlReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != LumenWidgetProvider.ACTION_TOGGLE_VPN) return
        toggleVpn(context)
        LumenWidgetProvider.refreshAll(context)
        LumenCompactWidgetProvider.refreshAll(context)
    }

    private fun toggleVpn(context: Context) {
        val isRunning = LumenVpnService.isRunning.value || LumenVpnService.isStarting.value
        if (isRunning) {
            startVpnService(context, VpnStartIntentFactory.buildStopIntent(context))
            return
        }
        val params = VpnStartIntentFactory.startParamsFromPrefs(context)
        if (!VpnStartIntentFactory.hasUsableConfig(context) ||
            (!params.proxyOnly && android.net.VpnService.prepare(context) != null)
        ) {
            openApp(context)
            return
        }
        if (!startVpnService(context, VpnStartIntentFactory.buildStartIntent(context, params))) {
            openApp(context)
        }
    }

    private fun startVpnService(context: Context, intent: Intent): Boolean = runCatching {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent)
        } else {
            context.startService(intent)
        }
    }.isSuccess

    private fun openApp(context: Context) {
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ?: return
        runCatching { context.startActivity(launch) }
    }
}
