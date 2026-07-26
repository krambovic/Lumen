package com.lumen.ui

import com.lumen.ui.screens.formatGeoBytes
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Locale

class GeoResourcesFormatTest {

    private val original: Locale = Locale.getDefault()

    @After
    fun restoreLocale() {
        Locale.setDefault(original)
    }

    @Test
    fun sizesUseAsciiDigitsAndDotRegardlessOfLocale() {
        Locale.setDefault(Locale.GERMANY)
        assertEquals("12.5 MB", formatGeoBytes(13_107_200L))
        Locale.setDefault(Locale.forLanguageTag("fa-IR"))
        assertEquals("12.5 MB", formatGeoBytes(13_107_200L))
    }

    @Test
    fun smallSizesStayInBytes() {
        assertEquals("512 B", formatGeoBytes(512L))
    }
}
