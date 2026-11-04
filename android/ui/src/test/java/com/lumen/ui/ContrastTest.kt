package com.lumen.ui

import androidx.compose.ui.graphics.Color
import com.lumen.ui.theme.contrastRatio
import com.lumen.ui.theme.readableForeground
import org.junit.Assert.*
import org.junit.Test

class ContrastTest {
    @Test fun customColorsAlwaysReceiveReadableText() {
        for (red in 0..255 step 17) for (green in 0..255 step 17) for (blue in 0..255 step 17) {
            val background = Color(red, green, blue)
            val foreground = readableForeground(background, background)
            assertTrue(contrastRatio(background, foreground) >= 4.5f)
        }
    }

    @Test fun readableThemeColorsAreKeptAndAlphaIsConsidered() {
        assertEquals(Color.Black, readableForeground(Color.White, Color.Black))
        assertEquals(Color.White, readableForeground(Color.Black, Color.White))
        val translucent = Color.Black.copy(alpha = .05f)
        assertEquals(Color.Black, readableForeground(Color.White, translucent))
    }
}
