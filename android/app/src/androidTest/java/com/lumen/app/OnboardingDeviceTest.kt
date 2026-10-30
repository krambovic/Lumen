package com.lumen.app

import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.lumen.ui.screens.*
import com.lumen.ui.theme.LumenTheme
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Real native layout/actions with isolated state; never touches the user's preferences or network. */
class OnboardingDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MotionTestActivity>()

    @Test fun fullSetupVisitsEveryPageAndAppliesSelections() {
        var settings by mutableStateOf(SettingsUiState(language = "en"))
        var welcome by mutableStateOf(OnboardingUiState(visible = true))
        var count by mutableIntStateOf(0)
        var finished = false
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                OnboardingScreen(welcome, settings, count, false,
                    onUpdate = { settings = it }, onLanguage = { settings = settings.copy(language = it) },
                    onImportText = { count++ }, onImportClipboard = { count++ }, onImportFile = { count++ }, onImportQr = { count++ },
                    onGenerateWarp = { welcome = welcome.copy(warpCount = 5); count += 5 }, onCancelWarp = {},
                    onFinish = { finished = true }, onCancelPreparation = {}, onOpenLink = {}, onMinimize = {})
            }
        } }
        compose.onNodeWithContentDescription("Get started").performClick()
        compose.onNodeWithText("Make it yours").assertIsDisplayed()
        compose.onNodeWithText(stringsForLanguage("en").dashboardStyleSlider).performScrollTo().performClick()
        compose.runOnIdle { assertEquals(DashboardStyle.SLIDER, settings.dashboardStyle) }
        compose.onNodeWithContentDescription("Continue").performClick()
        compose.onNodeWithText("Blocked sites only").performScrollTo().performClick()
        compose.runOnIdle { assertEquals("direct", settings.routingFinalOutbound) }
        compose.onNodeWithContentDescription("Continue").performClick()
        compose.onNodeWithText("Not yet").performClick()
        compose.onNodeWithContentDescription("Get WARP").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(5, count) }
        compose.onNodeWithContentDescription("Continue").performClick()
        compose.onNodeWithText("For every day").assertIsDisplayed()
        compose.onNodeWithContentDescription("Continue").performClick()
        compose.onNodeWithText("You're all set").assertIsDisplayed()
        compose.onNodeWithContentDescription("Open Lumen").performClick()
        compose.runOnIdle { assertTrue(finished) }
    }

    @Test fun completedSetupGetsOnlyTheUpdateWelcome() {
        var finished = false
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                OnboardingScreen(OnboardingUiState(visible = true, setupRequired = false, previousVersion = "1.4.0"),
                    SettingsUiState(language = "en"), 18, false, {}, {}, {}, {}, {}, {}, {}, {},
                    onFinish = { finished = true }, onCancelPreparation = {}, onOpenLink = {}, onMinimize = {})
            }
        } }
        compose.onNodeWithText("Welcome back").assertIsDisplayed()
        compose.onNodeWithContentDescription("Open Lumen").performClick()
        compose.runOnIdle { assertTrue(finished) }
    }

    @Test fun customAccentCanBePickedVisuallyWithoutEnteringAColorCode() {
        var settings by mutableStateOf(SettingsUiState(language = "en"))
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                OnboardingScreen(OnboardingUiState(visible = true), settings, 18, false,
                    onUpdate = { settings = it }, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, initialStep = WelcomeStep.LOOK)
            }
        } }
        compose.onNodeWithTag("accent_custom").performScrollTo().performClick()
        compose.onNodeWithTag("accent_saturation_value").performTouchInput { click(percentOffset(.7f, .3f)) }
        compose.onNodeWithTag("accent_hue").performTouchInput { click(percentOffset(.4f, .5f)) }
        compose.onNodeWithTag("accent_apply").performClick()
        compose.runOnIdle { assertTrue(settings.accentColor.matches(Regex("#[0-9A-F]{6}"))) }
        compose.onNodeWithText("Use palette color").assertDoesNotExist()
        val chosen = settings.accentColor
        compose.onNodeWithTag("accent_custom").performClick()
        compose.onNodeWithTag("accent_saturation_value").performTouchInput { click(percentOffset(.1f, .8f)) }
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertEquals("Cancel must retain the applied accent", chosen, settings.accentColor) }
        val preset = compose.onNodeWithContentDescription("Accent color #3174BF")
        preset.performScrollTo().performClick()
        compose.runOnIdle { assertEquals("#3174BF", settings.accentColor) }
        preset.performClick()
        compose.runOnIdle { assertEquals("", settings.accentColor) }
        preset.performClick()
        compose.onNodeWithText(stringsForLanguage("en").darkTheme).performScrollTo().performClick()
        compose.runOnIdle { assertEquals("Selecting a palette restores its accent", "", settings.accentColor) }
    }

    @Test fun preparingRoutesCanBeCancelledAndAnErrorCanBeRetried() {
        var welcome by mutableStateOf(OnboardingUiState(visible = true, preparing = true))
        var cancelled = false
        var retry = false
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                OnboardingScreen(welcome, SettingsUiState(language = "en"), 18, false,
                    {}, {}, {}, {}, {}, {}, {}, {}, onFinish = { retry = true },
                    onCancelPreparation = { cancelled = true; welcome = welcome.copy(preparing = false, preparationError = true) },
                    onOpenLink = {}, onMinimize = {}, initialStep = WelcomeStep.DONE)
            }
        } }
        compose.onNodeWithContentDescription("Open Lumen").assertIsNotEnabled()
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertTrue(cancelled) }
        compose.onNodeWithContentDescription("Open Lumen").assertIsEnabled().performClick()
        compose.runOnIdle { assertTrue(retry) }
    }
}
