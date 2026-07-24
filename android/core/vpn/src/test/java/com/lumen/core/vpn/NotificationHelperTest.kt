package com.lumen.core.vpn

import org.junit.Assert.assertEquals
import org.junit.Test

class NotificationHelperTest {

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
