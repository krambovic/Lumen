package com.lumen.app

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Home
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.lumen.ui.components.*
import com.lumen.ui.screens.*
import com.lumen.ui.theme.LumenTheme
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class DockConnectionDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MotionTestActivity>()

    @Test fun dockStyleHasExactlyOneConnectionActionAndAnInformationalHome() {
        val state = mutableStateOf(ConnectionState.Disconnected)
        var taps = 0
        compose.mainClock.autoAdvance = false
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                Scaffold(bottomBar = {
                    BottomConnectionControls(false, true, state.value,
                        listOf("Home", "Servers", "Network", "Settings").map { DockItem(it, it, Icons.Default.Home) },
                        "Home", {}, { taps++ }, dockConnection = true)
                }) { padding ->
                    DashboardScreen(state.value, listOf(NodeUiModel("sample", "Server", "vless", "example.invalid", 443,
                        isSelected = true)), emptyList(), dashboardStyle = DashboardStyle.DOCK,
                        onToggleConnection = { taps++ }, onSelectNode = {}, onImportClipboard = {}, onImportFile = {},
                        onImportQr = {}, onAddManualNode = {}, modifier = Modifier.padding(padding))
                }
            }
        } }
        compose.mainClock.advanceTimeBy(1000)
        compose.onAllNodesWithContentDescription("Connect").assertCountEquals(1)
        compose.onNodeWithTag("dock_connection").performClick()
        assertEquals(1, taps)
        listOf(ConnectionState.Connecting to "Cancel", ConnectionState.Connected to "Disconnect",
            ConnectionState.Disconnected to "Connect").forEach { (next, action) ->
            compose.runOnUiThread { state.value = next }
            compose.mainClock.advanceTimeBy(1200)
            compose.onAllNodesWithContentDescription(action).assertCountEquals(1)
            compose.onNodeWithTag("dock_connection").assertIsDisplayed()
        }
    }

    @Test fun onboardingCanSelectTheFourthStyle() {
        var settings by mutableStateOf(SettingsUiState(language = "en"))
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                OnboardingScreen(OnboardingUiState(visible = true), settings, 0, false,
                    onUpdate = { settings = it }, onLanguage = {}, onImportText = {}, onImportClipboard = {},
                    onImportFile = {}, onImportQr = {}, onGenerateWarp = {}, onCancelWarp = {},
                    onCancelPreparation = {}, onFinish = {}, onOpenLink = {}, onMinimize = {},
                    initialStep = WelcomeStep.LOOK)
            }
        } }
        compose.onNodeWithText("Dock control").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(DashboardStyle.DOCK, settings.dashboardStyle) }
    }

    @Test fun appearanceSettingsCanSelectTheFourthStyle() {
        var settings by mutableStateOf(SettingsUiState(language = "en"))
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                ThemeSettingsScreen(settings, { settings = it }, {})
            }
        } }
        compose.onNodeWithText("Dock control").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(DashboardStyle.DOCK, settings.dashboardStyle) }
    }
}
