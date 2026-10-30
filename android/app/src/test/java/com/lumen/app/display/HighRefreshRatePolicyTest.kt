package com.lumen.app.display

import org.junit.Assert.*
import org.junit.Test

class HighRefreshRatePolicyTest {
    @Test fun picks144HzWithoutChangingResolution() {
        val modes = listOf(
            DisplayModeOption(1, 1080, 2400, 60f), DisplayModeOption(2, 1080, 2400, 120f),
            DisplayModeOption(3, 1080, 2400, 144f), DisplayModeOption(4, 720, 1600, 240f)
        )
        assertEquals(3, highestRefreshMode(1080, 2400, modes)?.id)
    }

    @Test fun supports60HzAndNonIntegerRefreshRatesWithoutRounding() {
        val modes = listOf(DisplayModeOption(1, 1080, 2400, 59.94f), DisplayModeOption(2, 1080, 2400, 119.88f))
        assertEquals(119.88f, highestRefreshMode(1080, 2400, modes)!!.refreshRate, 0f)
        assertEquals(59.94f, highestRefreshMode(1080, 2400, modes.take(1))!!.refreshRate, 0f)
    }

    @Test fun neverChoosesInvalidRatesOrAnotherDisplayResolution() {
        val modes = listOf(DisplayModeOption(1, 1080, 2400, Float.NaN), DisplayModeOption(2, 1080, 2400, -1f),
            DisplayModeOption(3, 720, 1600, 144f))
        assertNull(highestRefreshMode(1080, 2400, modes))
    }
}
