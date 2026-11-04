package com.lumen.app

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.screens.*
import com.lumen.ui.theme.*
import java.io.File
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class BatteryPingUiDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MotionTestActivity>()

    private fun screenshot(name: String) {
        val file = File(compose.activity.filesDir, "review/$name.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use { compose.onAllNodes(isRoot()).onLast().captureToImage()
            .asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test fun selectedServerPingAndCancellationWorkInEveryDashboard() {
        var style by mutableStateOf(DashboardStyle.DEFAULT)
        var checking by mutableStateOf(false)
        var taps = 0
        compose.setContent { LumenTheme {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                DashboardScreen(ConnectionState.Connected,
                    listOf(NodeUiModel("test", "Test server", "openvpn", "vpn.example", 1194, isSelected = true)),
                    emptyList(), dashboardStyle = style, isCheckingPing = checking,
                    onCheckPing = { taps++; checking = !checking }, onToggleConnection = {}, onSelectNode = {},
                    onImportClipboard = {}, onImportFile = {}, onImportQr = {}, onAddManualNode = {})
            }
        } }
        DashboardStyle.entries.forEach { next ->
            compose.runOnIdle { style = next }
            compose.onNode(hasAnyAncestor(hasTestTag("dashboard_connected_ping")) and hasClickAction())
                .assertIsDisplayed().performClick()
            compose.onNode(hasAnyAncestor(hasTestTag("dashboard_connected_ping")) and hasClickAction())
                .assertIsDisplayed().performClick()
            compose.runOnIdle { assertFalse(checking) }
        }
        assertEquals(DashboardStyle.entries.size * 2, taps)
        screenshot("dashboard-ping")
    }

    @Test fun expressiveTextHasContrastAcrossPresetsAndCustomAccents() {
        var preset by mutableStateOf(ThemePreset.DARK)
        var accent by mutableStateOf("")
        var pairs = emptyList<Pair<Color, Color>>()
        compose.setContent { LumenTheme(themePreset = preset, accentColor = accent) {
            val art = expressivePalette()
            val scheme = MaterialTheme.colorScheme
            SideEffect { pairs = listOf(art.stage to art.onStage, art.lime to art.onLime,
                art.coral to art.onCoral, art.blue to art.onBlue, art.lavender to art.onLavender,
                scheme.primary to scheme.onPrimary, scheme.secondary to scheme.onSecondary) }
        } }
        for (theme in ThemePreset.entries) for (custom in listOf("", "#F5F2A0", "#101020")) {
            compose.runOnIdle { preset = theme; accent = custom }
            compose.waitForIdle()
            pairs.forEach { (background, foreground) ->
                val a = background.luminance(); val b = foreground.luminance()
                assertTrue("Unreadable pair for $theme/$custom", (maxOf(a, b) + .05f) / (minOf(a, b) + .05f) >= 4.5f)
            }
        }
    }

    @Test fun portNumbersStayVisibleWithLargeFontsOnNarrowScreen() {
        val strings = stringsForLanguage("en")
        compose.setContent { LumenTheme {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalStrings provides strings,
                LocalDensity provides Density(density.density, 2f)) {
                Box(Modifier.width(320.dp)) {
                    SettingsScreen(SettingsUiState(localInboundEnabled = true, localSocksPort = 65535,
                        localHttpPort = 10809), {}, {}, {}, {}, {})
                }
            }
        } }
        compose.onNodeWithContentDescription(strings.settingsAppTile).performScrollTo().performClick()
        compose.onNodeWithText("65535").performScrollTo().assertIsDisplayed()
        val field = compose.onNodeWithText("65535").fetchSemanticsNode().boundsInRoot
        assertTrue("Port field must have room for all five scaled digits", field.width > with(compose.density) { 200.dp.toPx() })
        screenshot("ports-large-font")
        compose.onNodeWithText("10809").performScrollTo().assertIsDisplayed()
    }
}
