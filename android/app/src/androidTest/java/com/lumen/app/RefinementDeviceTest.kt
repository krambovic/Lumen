package com.lumen.app

import android.graphics.Bitmap
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.LayoutDirection
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.components.LocalQrEncoder
import com.lumen.ui.screens.*
import com.lumen.ui.theme.*
import java.io.File
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class RefinementDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MotionTestActivity>()
    private fun screenshot(name: String) {
        val file = File(compose.activity.getExternalFilesDir(null), "review/$name.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use { compose.onAllNodes(isRoot()).onLast().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test fun supportOffersBothMethodsAndDisplaysTheWalletQr() {
        val s = stringsForLanguage("en")
        compose.setContent { LumenTheme(themeMode = ThemeMode.LIGHT) {
            CompositionLocalProvider(LocalStrings provides s,
                LocalQrEncoder provides { encodeQrBitmap(it, 512).asImageBitmap() }) {
                SettingsScreen(SettingsUiState(), {}, {}, {}, {}, {})
            }
        } }
        compose.onNodeWithContentDescription(s.community, substring = true).performScrollTo().performClick()
        compose.onNodeWithContentDescription(s.supportProject).performClick()
        compose.onNodeWithContentDescription("Donation Alerts").assertIsDisplayed()
        compose.onNodeWithContentDescription("${s.cryptoSupport} · USDT").performClick()
        compose.onNodeWithText("TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s").assertIsDisplayed()
        compose.waitUntil(5000) { compose.onAllNodesWithContentDescription(s.walletAddress).fetchSemanticsNodes().isNotEmpty() }
        screenshot("community-crypto")
    }

    @Test fun manualPingDoesNotReplaceTheProtocolOrDuplicateLatency() {
        val node = NodeUiModel("sample", "Server", "vless", "example.invalid", 443,
            pingMs = 100, countryCode = "NL", displayProtocol = "VLESS/REALITY")
        compose.setContent { LumenTheme(themeMode = ThemeMode.LIGHT) {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                ServerListScreen(listOf(node), emptyList(), refreshingIds = emptySet(), isPinging = false,
                    testingNodeId = null, serverTestResults = mapOf(node.id to "100 ms"), connectionState = ConnectionState.Disconnected,
                    onToggleConnection = {}, onSelectNode = {}, onEditNode = {}, onDeleteNode = {}, onAddNode = {},
                    onPingAll = {}, onPingNode = {}, onImportClipboard = {}, onRefreshSubscription = {},
                    onDeleteSubscription = {}, persistPreferences = false)
            }
        } }
        compose.onNodeWithText("VLESS/REALITY", useUnmergedTree = true).assertIsDisplayed()
        compose.onAllNodesWithText("100 ms", useUnmergedTree = true).assertCountEquals(1)
    }

    @Test fun persianSettingsStayInsideViewport() {
        val s = stringsForLanguage("fa")
        compose.setContent { LumenTheme(language = "fa", themeMode = ThemeMode.DARK) {
            CompositionLocalProvider(LocalStrings provides s, LocalLayoutDirection provides LayoutDirection.Rtl) {
                SettingsScreen(SettingsUiState(language = "fa"), {}, {}, {}, {}, {})
            }
        } }
        val root = compose.onRoot().fetchSemanticsNode().boundsInRoot
        listOf(s.routing, "DNS", s.ping).forEach { title ->
            val node = compose.onNodeWithContentDescription(title, substring = true).fetchSemanticsNode().boundsInRoot
            assertTrue("$title left clipped", node.left >= root.left)
            assertTrue("$title right clipped", node.right <= root.right)
        }
        screenshot("persian-settings")
        compose.onNodeWithContentDescription(s.settingsEditTiles).performClick()
        compose.onNodeWithContentDescription(s.settingsResizeTile + ": " + s.routing, useUnmergedTree = true).performClick()
        compose.waitForIdle()
        val full = compose.onNodeWithContentDescription(s.routing).fetchSemanticsNode().boundsInRoot
        assertTrue("Full tile must span both columns", full.width > root.width * .85f)
        screenshot("persian-full-tile")
    }

    @Test fun compactServersAreThreeAcrossWithoutProtocolClutter() {
        val nodes = (1..6).map { NodeUiModel("$it", "Server $it", "vless", "example.invalid", 443,
            countryCode = "NL", pingMs = it * 100, isSelected = it == 1) }
        compose.setContent { LumenTheme(themeMode = ThemeMode.LIGHT) {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage("en")) {
                ServerListScreen(nodes, emptyList(), refreshingIds = emptySet(), isPinging = false, testingNodeId = null,
                    serverTestResults = emptyMap(), connectionState = ConnectionState.Disconnected,
                    onToggleConnection = {}, onSelectNode = {}, onEditNode = {}, onDeleteNode = {}, onAddNode = {},
                    onPingAll = {}, onPingNode = {}, onImportClipboard = {}, onRefreshSubscription = {},
                    onDeleteSubscription = {}, persistPreferences = false, compactGrid = true)
            }
        } }
        val cards = (1..3).map { compose.onNodeWithText("Server $it", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot }
        assertEquals(cards[0].top, cards[1].top, 1f)
        assertEquals(cards[1].top, cards[2].top, 1f)
        compose.onAllNodesWithText("VLESS").assertCountEquals(0)
        screenshot("compact-servers")
    }

    @Test fun everyPaletteHasReadableExpressivePairs() {
        var preset by mutableStateOf(ThemePreset.DARK)
        var colors: ExpressivePalette? = null
        var background = 0
        compose.setContent { LumenTheme(themePreset = preset, themeMode = ThemeMode.DARK) {
            val palette = expressivePalette()
            val bg = MaterialTheme.colorScheme.background
            SideEffect { colors = palette; background = bg.toArgb() }
            Surface(Modifier.fillMaxSize()) { Text(preset.name) }
        } }
        val stages = mutableSetOf<androidx.compose.ui.graphics.Color>()
        ThemePreset.entries.filter { it != ThemePreset.LIGHT }.forEach { next ->
            compose.runOnUiThread { preset = next }
            compose.waitForIdle()
            val p = colors!!
            stages += p.stage
            val native = compose.activity.resources.newTheme().apply {
                applyStyle(splashThemeFor(SettingsUiState(themePreset = next), false), true)
            }
            val splashBackground = android.util.TypedValue()
            native.resolveAttribute(androidx.core.splashscreen.R.attr.windowSplashScreenBackground, splashBackground, true)
            assertEquals("$next splash must match its app palette", background, splashBackground.data)
            listOf(p.stage to p.onStage, p.lime to p.onLime, p.blue to p.onBlue, p.coral to p.onCoral,
                p.lavender to p.onLavender).forEach { (a, b) ->
                val ratio = (maxOf(a.luminance(), b.luminance()) + .05f) / (minOf(a.luminance(), b.luminance()) + .05f)
                assertTrue("$next contrast $ratio", ratio >= 4.5f)
            }
        }
        assertTrue("Presets must own distinct stages", stages.size >= 9)
    }

    @Test fun standardPalettesKeepTheirOriginalArtAndSplashFollowsAppMode() {
        var light by mutableStateOf(true)
        var art: ExpressivePalette? = null
        compose.setContent { LumenTheme(themePreset = if (light) ThemePreset.LIGHT else ThemePreset.DARK,
            themeMode = if (light) ThemeMode.LIGHT else ThemeMode.DARK) {
            val palette = expressivePalette()
            SideEffect { art = palette }
            Surface { Text("Lumen") }
        } }
        compose.waitForIdle()
        assertEquals(androidx.compose.ui.graphics.Color(0xFF2B1744), art!!.stage)
        assertEquals(androidx.compose.ui.graphics.Color(0xFFE1F59A), art!!.lime)
        compose.runOnUiThread { light = false }
        compose.waitForIdle()
        assertEquals(androidx.compose.ui.graphics.Color(0xFF3D2057), art!!.stage)
        val context = compose.activity
        listOf(true to 0xFFFAF7F2.toInt(), false to 0xFF09080D.toInt()).forEach { (mode, expected) ->
            val theme = context.resources.newTheme().apply { applyStyle(splashThemeFor(SettingsUiState(), mode), true) }
            val value = android.util.TypedValue()
            assertTrue(theme.resolveAttribute(androidx.core.splashscreen.R.attr.windowSplashScreenBackground, value, true))
            assertEquals(expected, value.data)
        }
    }
}
