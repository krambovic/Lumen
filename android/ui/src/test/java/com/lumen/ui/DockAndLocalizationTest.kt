package com.lumen.ui

import com.lumen.ui.components.DockSize
import com.lumen.ui.screens.*
import org.junit.Assert.*
import org.junit.Test

class DockAndLocalizationTest {
    @Test fun normalDockUsesPublishedReleaseDimensions() {
        assertEquals(54, DockSize.DEFAULT.height)
        assertEquals(20, DockSize.DEFAULT.iconSize)
        assertEquals("Обычный", InterfaceText.DEFAULT.inLanguage("ru"))
    }
    @Test fun continuousDockPreferencesMigrateToSupportedSizes() {
        assertEquals(DockSize.SMALL, DockSize.fromPercent(50))
        assertEquals(DockSize.DEFAULT, DockSize.fromPercent(100))
        assertEquals(DockSize.LARGE, DockSize.fromPercent(200))
        assertEquals(DockSize.SMALL, DockSize.fromPercent(Int.MIN_VALUE))
        assertEquals(DockSize.LARGE, DockSize.fromPercent(Int.MAX_VALUE))
        DockSize.entries.forEach { assertEquals(it, DockSize.fromPercent(it.percent)) }
    }

    @Test fun regionalLanguageTagsUseTheSelectedTranslation() {
        for ((tag, language) in listOf("ru-RU" to "ru", "fa_IR" to "fa", "zh-CN" to "zh", "en-US" to "en")) {
            assertEquals(language, stringsForLanguage(tag).localeCode)
            assertEquals(InterfaceText.DOCK_SIZE.inLanguage(language), InterfaceText.DOCK_SIZE.inLanguage(tag))
            assertEquals(LumenProfileText.CREDENTIALS.inLanguage(language), LumenProfileText.CREDENTIALS.inLanguage(tag))
            assertEquals(NetworkInfoText.TITLE.inLanguage(language), NetworkInfoText.TITLE.inLanguage(tag))
            assertEquals(WelcomeText.REOPEN.inLanguage(language), WelcomeText.REOPEN.inLanguage(tag))
        }
    }

    @Test fun addedInterfaceLabelsHaveAllFourTranslations() {
        InterfaceText.entries.forEach { label ->
            val english = label.inLanguage("en")
            for (language in listOf("ru", "fa", "zh")) {
                val translated = label.inLanguage(language)
                assertTrue("$label / $language", translated.isNotBlank())
                assertNotEquals("$label / $language", english, translated)
            }
        }
    }

    @Test fun newlyAddedMessagesDoNotFallBackToEnglish() {
        val english = stringsForLanguage("en")
        for (language in listOf("ru", "fa", "zh")) {
            val localized = stringsForLanguage(language)
            assertNotEquals(english.ovpnKeyPasswordRequired, localized.ovpnKeyPasswordRequired)
            assertNotEquals(english.showPassword, localized.showPassword)
            assertNotEquals(english.pingUnreachable, localized.pingUnreachable)
            assertNotEquals(english.subscriptionUpdated, localized.subscriptionUpdated)
        }
    }

    @Test fun importPresentationLocalizesMessagesAndPreservesGroupNames() {
        for (language in listOf("ru", "fa", "zh")) {
            val strings = stringsForLanguage(language)
            assertNotEquals("Import complete", translatedImportText("Import complete", strings))
            val result = translatedImportText("12 server(s) added to My group", strings)
            assertTrue(result.contains("12"))
            assertTrue(result.contains("My group"))
            assertFalse(result.contains("server(s)"))
            assertNotEquals("File exceeds the 8 MiB import limit",
                translatedImportText("File exceeds the 8 MiB import limit", strings))
            assertEquals("Provider message", translatedImportText("Provider message", strings))
        }
    }
}
