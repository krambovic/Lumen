package com.lumen.core.vpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import com.lumen.core.engine.TrafficStats
import java.util.Locale

object NotificationHelper {
    const val CHANNEL_ID = "lumen_vpn_channel"
    const val CHANNEL_NAME = "Lumen VPN Service"
    const val NOTIFICATION_ID = 1001

    @Volatile
    private var channelCreated = false

    fun createNotificationChannel(context: Context) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !channelCreated) {
            channelCreated = true
            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Notification channel for active Lumen VPN connection"
                setShowBadge(false)
            }
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(channel)
        }
    }

    fun buildNotification(
        context: Context,
        stats: TrafficStats = TrafficStats(),
        isConnected: Boolean = true
    ): Notification {
        createNotificationChannel(context)

        val title = if (isConnected) "Lumen VPN Connected" else "Lumen VPN Disconnected"
        val showSpeed = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
            .getBoolean("show_notification_speed", true)
        val contentText = if (isConnected) {
            if (showSpeed) {
                val upSpeed = formatSpeed(stats.uploadSpeed)
                val downSpeed = formatSpeed(stats.downloadSpeed)
                val totalUp = formatBytes(stats.totalUploaded)
                val totalDown = formatBytes(stats.totalDownloaded)
                "↑ $upSpeed  ↓ $downSpeed | Total: ↑ $totalUp  ↓ $totalDown"
            } else {
                "Lumen is protecting your data"
            }
        } else {
            "Tap to connect"
        }

        // Action intent for Disconnect / Connect
        val actionIntent = Intent(context, LumenVpnService::class.java).apply {
            action = if (isConnected) LumenVpnService.ACTION_STOP_VPN else LumenVpnService.ACTION_START_VPN
        }
        val actionPendingIntentFlags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
        }
        val actionPendingIntent = PendingIntent.getService(
            context,
            if (isConnected) 1 else 2,
            actionIntent,
            actionPendingIntentFlags
        )

        val actionTitle = if (isConnected) "Disconnect" else "Connect"

        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(contentText)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setOngoing(isConnected)
            .setOnlyAlertOnce(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .addAction(
                android.R.drawable.ic_menu_close_clear_cancel,
                actionTitle,
                actionPendingIntent
            )

        return builder.build()
    }

    fun updateNotification(
        context: Context,
        stats: TrafficStats,
        isConnected: Boolean = true
    ) {
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val showNotification = context.getSharedPreferences("lumen_prefs", Context.MODE_PRIVATE)
            .getBoolean("show_notification", true)
        if (!showNotification && isConnected) {
            manager.cancel(NOTIFICATION_ID)
        } else {
            manager.notify(NOTIFICATION_ID, buildNotification(context, stats, isConnected))
        }
    }

    fun formatSpeed(bytesPerSec: Long): String {
        return "${formatBytes(bytesPerSec)}/s"
    }

    fun formatBytes(bytes: Long): String {
        if (bytes <= 0) return "0 B"
        val units = arrayOf("B", "KB", "MB", "GB", "TB")
        var value = bytes.toDouble()
        var index = 0
        while (value >= 1024.0 && index < units.size - 1) {
            value /= 1024.0
            index++
        }
        if (index < units.size - 1 && String.format(Locale.US, "%.1f", value) == "1024.0") {
            value = 1.0
            index++
        }
        return if (index == 0) {
            "${bytes} B"
        } else {
            String.format(Locale.US, "%.1f %s", value, units[index])
        }
    }
}
