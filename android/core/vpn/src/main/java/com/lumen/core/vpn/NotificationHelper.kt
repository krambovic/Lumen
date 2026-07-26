package com.lumen.core.vpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Base64
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.lumen.core.engine.TrafficStats
import java.util.Locale

object NotificationHelper {
    /**
     * Importance, sound and lockscreen visibility freeze the moment a channel is
     * created, so none of the settings below ever reached a device that had already
     * run an older build. Both ids are new and the original channel is deleted.
     */
    const val CHANNEL_ID = "lumen_vpn_status_v2"

    /**
     * "Show VPN notification = off". A foreground service notification cannot be
     * cancelled while the service runs, so the quiet channel is the closest Android
     * gets to hiding it: no status bar icon, collapsed at the bottom of the shade.
     */
    const val CHANNEL_ID_QUIET = "lumen_vpn_status_quiet_v2"
    private const val LEGACY_CHANNEL_ID = "lumen_vpn_channel"
    const val CHANNEL_NAME = "VPN status"
    const val CHANNEL_NAME_QUIET = "VPN status (minimised)"
    const val NOTIFICATION_ID = 1001

    // Read straight from "lumen_prefs" with a safe default, so every one of them
    // works before the matching switch exists in the settings screen.
    const val PREF_SHOW_NOTIFICATION = "show_notification"
    const val PREF_SHOW_SPEED = "show_notification_speed"
    const val PREF_SPEED_STATS = "enable_speed_stats"
    const val PREF_SHOW_SERVER = "show_notification_server"
    const val PREF_SHOW_ON_LOCKSCREEN = "show_notification_on_lockscreen"
    private const val PREF_SESSION_STARTED_AT = "session_started_at"
    private const val PREF_SERVER_NAME = "selected_node_name"
    private const val PREF_SERVER_NAME_B64 = "selected_node_name_b64"

    private const val REQUEST_STOP = 1
    private const val REQUEST_OPEN_APP = 4
    private const val MAX_SERVER_NAME_CHARS = 48

    /**
     * Overrides the status bar glyph. 0 means "resolve the app's own icon", which is
     * what actually happens today: nothing ever assigned this, so every notification
     * went out with a framework dialog icon.
     */
    @Volatile
    var smallIconRes: Int = 0

    /** Flat single-colour vectors, in the order they are preferred. */
    private val ICON_CANDIDATES = arrayOf(
        "ic_stat_lumen",
        "ic_qs_lumen",
        "ic_launcher_monochrome"
    )

    @Volatile
    private var resolvedIconRes: Int = 0

    enum class Stage { CONNECTING, CONNECTED, DISCONNECTED }

    data class NotificationSettings(
        val visible: Boolean = true,
        val showSpeed: Boolean = true,
        val showServer: Boolean = true,
        val onLockscreen: Boolean = true
    )

    @Volatile
    private var channelsCreated = false

    // Rebuilding the builder every second churns PendingIntents and actions for a
    // notification whose only moving part is one line of text.
    @Volatile
    private var cachedBuilder: NotificationCompat.Builder? = null

    @Volatile
    private var cachedKey: String? = null

    // What the currently posted notification renders as. Re-posting an identical
    // notification once a second is pure churn, and it is also what made a changed
    // setting look like it had taken effect only "sometimes".
    @Volatile
    private var lastPostedKey: String? = null

    /**
     * :core:vpn owns no resources, so the glyph is looked up in the app package by
     * name. A miss degrades to the framework icon instead of failing the build, and
     * the app can still assign [smallIconRes] to skip the lookup entirely.
     */
    private fun smallIcon(context: Context): Int {
        smallIconRes.takeIf { it != 0 }?.let { return it }
        resolvedIconRes.takeIf { it != 0 }?.let { return it }
        val resolved = ICON_CANDIDATES.firstNotNullOfOrNull { name ->
            runCatching {
                context.resources.getIdentifier(name, "drawable", context.packageName)
            }.getOrNull()?.takeIf { it != 0 }
        } ?: return android.R.drawable.ic_dialog_info
        resolvedIconRes = resolved
        return resolved
    }

    fun settings(context: Context): NotificationSettings {
        val prefs = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        )
        return NotificationSettings(
            visible = prefs.getBoolean(PREF_SHOW_NOTIFICATION, true),
            showSpeed = prefs.getBoolean(PREF_SPEED_STATS, true) &&
                prefs.getBoolean(PREF_SHOW_SPEED, true),
            showServer = prefs.getBoolean(PREF_SHOW_SERVER, true),
            onLockscreen = prefs.getBoolean(PREF_SHOW_ON_LOCKSCREEN, true)
        )
    }

    fun createNotificationChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O || channelsCreated) return
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager
            ?: return
        channelsCreated = true
        runCatching { manager.deleteNotificationChannel(LEGACY_CHANNEL_ID) }
        runCatching {
            manager.createNotificationChannel(
                channel(CHANNEL_ID, CHANNEL_NAME, NotificationManager.IMPORTANCE_LOW)
            )
            manager.createNotificationChannel(
                channel(CHANNEL_ID_QUIET, CHANNEL_NAME_QUIET, NotificationManager.IMPORTANCE_MIN)
            )
        }
    }

    private fun channel(id: String, name: String, importance: Int): NotificationChannel =
        NotificationChannel(id, name, importance).apply {
            description = "Status of the active Lumen VPN connection"
            setShowBadge(false)
            setSound(null, null)
            enableVibration(false)
            enableLights(false)
            // Permissive on purpose: the platform takes the more private of the
            // channel and the notification, so the per-notification visibility is
            // what actually decides, and that one follows a live preference.
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        }

    /**
     * A foreground service still runs when POST_NOTIFICATIONS is denied on Android
     * 13+, it just shows nothing. Callers surface that instead of leaving the user
     * with a tunnel they can neither see nor stop from the shade.
     */
    fun notificationsEnabled(context: Context): Boolean = runCatching {
        if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) {
            return@runCatching false
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return@runCatching true
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager
            ?: return@runCatching true
        val channel = manager.getNotificationChannel(channelIdFor(settings(context)))
            ?: return@runCatching true
        channel.importance != NotificationManager.IMPORTANCE_NONE
    }.getOrDefault(true)

    /**
     * Both builders are handed straight to startForeground(), so whatever they
     * return is what ends up on screen and becomes the posted state.
     */
    fun buildNotification(
        context: Context,
        stats: TrafficStats = TrafficStats(),
        isConnected: Boolean = true
    ): Notification {
        val rendered = render(
            context,
            if (isConnected) Stage.CONNECTED else Stage.DISCONNECTED,
            stats
        )
        lastPostedKey = rendered.key
        return rendered.notification
    }

    fun buildConnectingNotification(context: Context): Notification {
        val rendered = render(context, Stage.CONNECTING, TrafficStats())
        lastPostedKey = rendered.key
        return rendered.notification
    }

    fun updateNotification(
        context: Context,
        stats: TrafficStats,
        isConnected: Boolean = true
    ) {
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager
            ?: return
        val rendered = render(
            context,
            if (isConnected) Stage.CONNECTED else Stage.DISCONNECTED,
            stats
        )
        // Nothing changed, so nothing is posted. The caller ticks once a second and
        // every one of those posts used to redraw the shade for no reason.
        if (rendered.key == lastPostedKey) return
        // "Show notification = off" moves to the quiet channel instead of cancelling:
        // cancel() is a no-op for the notification a foreground service is holding,
        // which is why the switch appeared to do nothing at all.
        runCatching { manager.notify(NOTIFICATION_ID, rendered.notification) }
            .onSuccess { lastPostedKey = rendered.key }
    }

    /** Drops the cached builder so the next post picks up new settings or a new server. */
    fun invalidate() {
        cachedBuilder = null
        cachedKey = null
        lastPostedKey = null
    }

    fun cancel(context: Context) {
        invalidate()
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager
            ?: return
        runCatching { manager.cancel(NOTIFICATION_ID) }
    }

    private data class Rendered(val notification: Notification, val key: String)

    private fun render(context: Context, stage: Stage, stats: TrafficStats): Rendered {
        createNotificationChannel(context)
        val settings = settings(context)
        val icon = smallIcon(context)
        val server = if (settings.showServer) serverName(context) else ""
        val sessionStartedAt = sessionStartedAt(context)
        // Everything the builder itself bakes in. A change to any of it has to
        // rebuild, which is how a toggled setting reaches an already open shade.
        val builderKey = "$stage|$settings|$server|$sessionStartedAt|$icon"
        val builder = cachedBuilder?.takeIf { cachedKey == builderKey } ?: newBuilder(
            context,
            stage,
            settings,
            server,
            sessionStartedAt,
            icon
        ).also {
            cachedBuilder = it
            cachedKey = builderKey
        }
        val text = contentText(settings, stats, stage)
        return Rendered(builder.setContentText(text).build(), "$builderKey|$text")
    }

    /** Pure, so the settings matrix is testable without a Context. */
    fun channelIdFor(settings: NotificationSettings): String =
        if (settings.visible) CHANNEL_ID else CHANNEL_ID_QUIET

    /**
     * Pure. Deferring is the only lever the platform gives a foreground service that
     * wants to stay out of the shade; it cannot remove the notification outright.
     */
    fun foregroundBehaviorFor(settings: NotificationSettings): Int =
        if (settings.visible) {
            NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE
        } else {
            NotificationCompat.FOREGROUND_SERVICE_DEFERRED
        }

    /**
     * Pure. The title names a server, so the lockscreen only ever sees the public
     * version, and not even that unless the user asked for it.
     */
    fun lockscreenVisibilityFor(settings: NotificationSettings): Int =
        if (settings.onLockscreen) {
            NotificationCompat.VISIBILITY_PRIVATE
        } else {
            NotificationCompat.VISIBILITY_SECRET
        }

    private fun newBuilder(
        context: Context,
        stage: Stage,
        settings: NotificationSettings,
        server: String,
        sessionStartedAt: Long,
        icon: Int
    ): NotificationCompat.Builder {
        val ongoing = stage != Stage.DISCONNECTED
        val builder = NotificationCompat.Builder(context, channelIdFor(settings))
            .setSmallIcon(icon)
            .setContentTitle(title(settings, server, stage))
            .setOngoing(ongoing)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setLocalOnly(true)
            .setShowWhen(false)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setPriority(
                if (settings.visible) NotificationCompat.PRIORITY_LOW
                else NotificationCompat.PRIORITY_MIN
            )
            .setVisibility(lockscreenVisibilityFor(settings))
            // A foreground service always owns a notification - the platform will not
            // let a VPN hide it - so "show notification = off" buys what it can: the
            // quiet IMPORTANCE_MIN channel drops the status bar icon, and deferring
            // keeps the shade entry away for ten seconds (forever for a short session).
            // When the switch is on we post immediately, otherwise the notification
            // looks broken for the first ten seconds of every connection.
            .setForegroundServiceBehavior(foregroundBehaviorFor(settings))
        if (stage == Stage.CONNECTED && settings.visible && sessionStartedAt > 0L) {
            // The chronometer ticks by itself; re-posting once a second would
            // otherwise be the only way to keep a session duration current.
            builder.setWhen(sessionStartedAt).setShowWhen(true).setUsesChronometer(true)
        }
        if (settings.onLockscreen) {
            builder.setPublicVersion(publicVersion(context, settings, stage, icon))
        }
        val openApp = openAppIntent(context)
        openApp?.let(builder::setContentIntent)
        when (stage) {
            Stage.CONNECTED, Stage.CONNECTING -> stopIntent(context)?.let {
                builder.addAction(
                    android.R.drawable.ic_menu_close_clear_cancel,
                    if (stage == Stage.CONNECTED) "Disconnect" else "Cancel",
                    it
                )
            }
            // Never ACTION_START_VPN from here: the notification carries no config,
            // so the service would start only to stop itself again.
            Stage.DISCONNECTED -> openApp?.let {
                builder.addAction(android.R.drawable.ic_menu_view, "Open Lumen", it)
            }
        }
        return builder
    }

    private fun publicVersion(
        context: Context,
        settings: NotificationSettings,
        stage: Stage,
        icon: Int
    ): Notification =
        NotificationCompat.Builder(context, channelIdFor(settings))
            .setSmallIcon(icon)
            .setContentTitle(
                when (stage) {
                    Stage.CONNECTED -> "Lumen VPN is on"
                    Stage.CONNECTING -> "Lumen VPN is connecting"
                    Stage.DISCONNECTED -> "Lumen VPN is off"
                }
            )
            .setOngoing(stage != Stage.DISCONNECTED)
            .setSilent(true)
            .setShowWhen(false)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .build()

    /** Pure, so the settings matrix is testable without a Context. */
    fun title(settings: NotificationSettings, server: String, stage: Stage): String = when {
        stage == Stage.DISCONNECTED -> "Lumen VPN disconnected"
        stage == Stage.CONNECTING -> "Lumen VPN connecting"
        settings.showServer && server.isNotBlank() -> "Connected to $server"
        else -> "Lumen VPN connected"
    }

    /** Pure, so the settings matrix is testable without a Context. */
    fun contentText(
        settings: NotificationSettings,
        stats: TrafficStats,
        stage: Stage
    ): String = when {
        stage == Stage.CONNECTING -> "Starting the selected server"
        stage == Stage.DISCONNECTED -> "Tap to open Lumen"
        settings.visible && settings.showSpeed ->
            "↑ ${formatSpeed(stats.uploadSpeed)}  ↓ ${formatSpeed(stats.downloadSpeed)}" +
                "  |  ↑ ${formatBytes(stats.totalUploaded)}  ↓ ${formatBytes(stats.totalDownloaded)}"
        else -> "Lumen is protecting your data"
    }

    private fun serverName(context: Context): String {
        val prefs = context.getSharedPreferences(
            VpnStartIntentFactory.PREFS_NAME,
            Context.MODE_PRIVATE
        )
        // The Base64/UTF-8 mirror is what the widget already trusts: a plain prefs
        // string comes back mangled in another process on some ROMs.
        val decoded = prefs.getString(PREF_SERVER_NAME_B64, null)?.let { encoded ->
            runCatching {
                String(Base64.decode(encoded, Base64.NO_WRAP), Charsets.UTF_8)
            }.getOrNull()
        }?.takeIf { it.isNotBlank() }
        return (decoded ?: prefs.getString(PREF_SERVER_NAME, null))
            .orEmpty()
            .trim()
            .take(MAX_SERVER_NAME_CHARS)
    }

    private fun sessionStartedAt(context: Context): Long =
        context.getSharedPreferences(VpnStartIntentFactory.PREFS_NAME, Context.MODE_PRIVATE)
            .getLong(PREF_SESSION_STARTED_AT, 0L)

    private fun stopIntent(context: Context): PendingIntent? = runCatching {
        PendingIntent.getService(
            context,
            REQUEST_STOP,
            Intent(context, LumenVpnService::class.java).setAction(LumenVpnService.ACTION_STOP_VPN),
            pendingIntentFlags()
        )
    }.getOrNull()

    private fun openAppIntent(context: Context): PendingIntent? = runCatching {
        val launch = context.packageManager
            .getLaunchIntentForPackage(context.packageName)
            ?.addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED
            )
            ?: return@runCatching null
        PendingIntent.getActivity(context, REQUEST_OPEN_APP, launch, pendingIntentFlags())
    }.getOrNull()

    /** FLAG_IMMUTABLE is mandatory from S onwards and safe from M onwards. */
    private fun pendingIntentFlags(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
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
