package com.lumen.core.vpn

import android.content.Context
import android.content.SharedPreferences
import androidx.core.app.NotificationCompat
import com.lumen.core.engine.TrafficStats
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.kotlin.any
import org.mockito.kotlin.eq
import org.mockito.kotlin.mock
import org.mockito.kotlin.whenever

class NotificationHelperTest {

    private fun contextWithPrefs(booleans: Map<String, Boolean>): Context {
        val prefs = mock<SharedPreferences>()
        whenever(prefs.getBoolean(any(), any())).thenAnswer { invocation ->
            booleans[invocation.getArgument<String>(0)] ?: invocation.getArgument<Boolean>(1)
        }
        val context = mock<Context>()
        whenever(context.getSharedPreferences(eq(VpnStartIntentFactory.PREFS_NAME), any()))
            .thenReturn(prefs)
        return context
    }

    private val allOn = NotificationHelper.NotificationSettings()

    @Test
    fun `every notification setting defaults to on so the feature works before the UI lands`() {
        val settings = NotificationHelper.settings(contextWithPrefs(emptyMap()))

        assertEquals(NotificationHelper.NotificationSettings(), settings)
    }

    @Test
    fun `each notification setting is read from lumen_prefs`() {
        assertFalse(
            NotificationHelper.settings(
                contextWithPrefs(mapOf(NotificationHelper.PREF_SHOW_NOTIFICATION to false))
            ).visible
        )
        assertFalse(
            NotificationHelper.settings(
                contextWithPrefs(mapOf(NotificationHelper.PREF_SHOW_SERVER to false))
            ).showServer
        )
        assertFalse(
            NotificationHelper.settings(
                contextWithPrefs(mapOf(NotificationHelper.PREF_SHOW_ON_LOCKSCREEN to false))
            ).onLockscreen
        )
    }

    @Test
    fun `turning off the speed readout either way hides it`() {
        // The dashboard-wide switch and the notification-only switch both count.
        assertFalse(
            NotificationHelper.settings(
                contextWithPrefs(mapOf(NotificationHelper.PREF_SPEED_STATS to false))
            ).showSpeed
        )
        assertFalse(
            NotificationHelper.settings(
                contextWithPrefs(mapOf(NotificationHelper.PREF_SHOW_SPEED to false))
            ).showSpeed
        )
    }

    @Test
    fun `hiding the notification moves it to the quiet channel instead of cancelling it`() {
        // A foreground service notification cannot be cancelled, so the switch has
        // to change where it is posted or it does nothing at all.
        assertEquals(NotificationHelper.CHANNEL_ID, NotificationHelper.channelIdFor(allOn))
        assertEquals(
            NotificationHelper.CHANNEL_ID_QUIET,
            NotificationHelper.channelIdFor(allOn.copy(visible = false))
        )
        assertNotEquals(NotificationHelper.CHANNEL_ID, NotificationHelper.CHANNEL_ID_QUIET)
    }

    @Test
    fun `hiding the notification also defers it so the shade stays clean`() {
        // The quiet channel only drops the status bar icon. Deferring is the second
        // half: on Android 12+ it keeps the entry out of the shade for ten seconds,
        // which for a short session means it never appears at all. Posting the
        // visible one immediately avoids a ten second gap on every connect.
        assertEquals(
            androidx.core.app.NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE,
            NotificationHelper.foregroundBehaviorFor(allOn)
        )
        assertEquals(
            androidx.core.app.NotificationCompat.FOREGROUND_SERVICE_DEFERRED,
            NotificationHelper.foregroundBehaviorFor(allOn.copy(visible = false))
        )
    }

    @Test
    fun `the lockscreen never shows the server name`() {
        // PRIVATE swaps in the public version, which names no server; SECRET hides
        // the notification from the lockscreen entirely.
        assertEquals(
            NotificationCompat.VISIBILITY_PRIVATE,
            NotificationHelper.lockscreenVisibilityFor(allOn)
        )
        assertEquals(
            NotificationCompat.VISIBILITY_SECRET,
            NotificationHelper.lockscreenVisibilityFor(allOn.copy(onLockscreen = false))
        )
    }

    @Test
    fun `the title names the server only while connected and only when allowed`() {
        assertEquals(
            "Connected to Amsterdam",
            NotificationHelper.title(allOn, "Amsterdam", NotificationHelper.Stage.CONNECTED)
        )
        assertEquals(
            "Lumen VPN connected",
            NotificationHelper.title(
                allOn.copy(showServer = false),
                "Amsterdam",
                NotificationHelper.Stage.CONNECTED
            )
        )
        // No server selected yet must not render "Connected to ".
        assertEquals(
            "Lumen VPN connected",
            NotificationHelper.title(allOn, "", NotificationHelper.Stage.CONNECTED)
        )
        // The stage wins over the server name in both non-connected states.
        assertEquals(
            "Lumen VPN connecting",
            NotificationHelper.title(allOn, "Amsterdam", NotificationHelper.Stage.CONNECTING)
        )
        assertEquals(
            "Lumen VPN disconnected",
            NotificationHelper.title(allOn, "Amsterdam", NotificationHelper.Stage.DISCONNECTED)
        )
    }

    @Test
    fun `the speed line disappears when either speed setting is off`() {
        val stats = TrafficStats(
            uploadSpeed = 2048,
            downloadSpeed = 1024,
            totalUploaded = 4096,
            totalDownloaded = 8192
        )

        val shown = NotificationHelper.contentText(allOn, stats, NotificationHelper.Stage.CONNECTED)
        assertTrue(shown.contains("2.0 KB/s"))
        assertTrue(shown.contains("1.0 KB/s"))
        assertTrue(shown.contains("8.0 KB"))

        assertEquals(
            "Lumen is protecting your data",
            NotificationHelper.contentText(
                allOn.copy(showSpeed = false),
                stats,
                NotificationHelper.Stage.CONNECTED
            )
        )
        // Hidden notification: the quiet channel still needs a body, but not one
        // that keeps changing once a second.
        assertEquals(
            "Lumen is protecting your data",
            NotificationHelper.contentText(
                allOn.copy(visible = false),
                stats,
                NotificationHelper.Stage.CONNECTED
            )
        )
    }

    @Test
    fun `the connecting and disconnected bodies ignore stale traffic counters`() {
        val stats = TrafficStats(uploadSpeed = 999, downloadSpeed = 999)

        assertEquals(
            "Starting the selected server",
            NotificationHelper.contentText(allOn, stats, NotificationHelper.Stage.CONNECTING)
        )
        assertEquals(
            "Tap to open Lumen",
            NotificationHelper.contentText(allOn, stats, NotificationHelper.Stage.DISCONNECTED)
        )
    }

    @Test
    fun `formatBytes zero and negative values return 0 B`() {
        assertEquals("0 B", NotificationHelper.formatBytes(0))
        assertEquals("0 B", NotificationHelper.formatBytes(-500))
    }

    @Test
    fun `formatBytes small values formatted in bytes`() {
        assertEquals("1 B", NotificationHelper.formatBytes(1))
        assertEquals("500 B", NotificationHelper.formatBytes(500))
        assertEquals("1023 B", NotificationHelper.formatBytes(1023))
    }

    @Test
    fun `formatBytes kilobytes formatted properly`() {
        assertEquals("1.0 KB", NotificationHelper.formatBytes(1024))
        assertEquals("1.5 KB", NotificationHelper.formatBytes(1536))
    }

    @Test
    fun `formatBytes boundary rounding edge cases roll over correctly`() {
        // 1048500 B is ~1023.92 KB, formatted as 1023.9 KB
        assertEquals("1023.9 KB", NotificationHelper.formatBytes(1048500))

        // Values very close to 1024 KB (e.g., 1048570 B) where 1048570 / 1024 = 1023.994
        // rounding with %.1f gives 1024.0 KB which should roll over to 1.0 MB
        assertEquals("1.0 MB", NotificationHelper.formatBytes(1048570))
        assertEquals("1.0 MB", NotificationHelper.formatBytes(1048576))
    }

    @Test
    fun `formatSpeed appends per second suffix`() {
        assertEquals("1.0 KB/s", NotificationHelper.formatSpeed(1024))
        assertEquals("0 B/s", NotificationHelper.formatSpeed(0))
    }
}
