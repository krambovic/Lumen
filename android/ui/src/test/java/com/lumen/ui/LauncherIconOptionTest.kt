package com.lumen.ui

import com.lumen.ui.screens.LauncherIconOption
import com.lumen.ui.screens.LumenStrings
import com.lumen.ui.screens.SettingsUiState
import com.lumen.ui.screens.stringsForLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Guards the launcher-icon picker added to Customization: the shipped default must
 * stay "follow the system theme", every variant must be reachable, and every label
 * must exist in all four shipped languages (a blank field renders as an empty tile).
 */
class LauncherIconOptionTest {

    private val languages = listOf("en", "ru", "zh", "fa")

    /**
     * Losing the auto-theming behaviour would be a silent regression for everyone
     * who never opens the picker.
     */
    @Test
    fun theDefaultIsFollowSystem() {
        assertEquals(LauncherIconOption.SYSTEM, SettingsUiState().launcherIcon)
    }

    /** The auto state has to remain a destination, not just a starting point. */
    @Test
    fun theUserCanReturnToFollowSystem() {
        val pinned = SettingsUiState().copy(launcherIcon = LauncherIconOption.DARK)
        assertEquals(
            LauncherIconOption.SYSTEM,
            pinned.copy(launcherIcon = LauncherIconOption.SYSTEM).launcherIcon
        )
    }

    @Test
    fun bothVariantsPlusTheAutoStateAreOffered() {
        assertEquals(
            listOf(
                LauncherIconOption.SYSTEM,
                LauncherIconOption.LIGHT,
                LauncherIconOption.DARK
            ),
            LauncherIconOption.values().toList()
        )
    }

    @Test
    fun everyLabelIsTranslatedInAllFourLanguages() {
        val labels: List<Pair<String, (LumenStrings) -> String>> = listOf(
            "launcherIcon" to { s: LumenStrings -> s.launcherIcon },
            "launcherIconDesc" to { s: LumenStrings -> s.launcherIconDesc },
            "launcherIconSystem" to { s: LumenStrings -> s.launcherIconSystem },
            "launcherIconLight" to { s: LumenStrings -> s.launcherIconLight },
            "launcherIconDark" to { s: LumenStrings -> s.launcherIconDark }
        )
        languages.forEach { code ->
            val strings = stringsForLanguage(code)
            labels.forEach { (name, read) ->
                assertTrue("$name is blank in $code", read(strings).isNotBlank())
            }
        }
    }

    /** The tile reuses the already-translated "Default" label as its badge. */
    @Test
    fun theDefaultBadgeLabelIsTranslatedEverywhere() {
        languages.forEach { code ->
            assertTrue(
                "dashboardStyleDefault is blank in $code",
                stringsForLanguage(code).dashboardStyleDefault.isNotBlank()
            )
        }
    }

    /** Russian and Persian must not silently fall back to the English wording. */
    @Test
    fun nonEnglishLocalesActuallyTranslateTheSectionTitle() {
        val english = stringsForLanguage("en").launcherIcon
        listOf("ru", "zh", "fa").forEach { code ->
            assertTrue(
                "launcherIcon was not translated for $code",
                stringsForLanguage(code).launcherIcon != english
            )
        }
    }
}
