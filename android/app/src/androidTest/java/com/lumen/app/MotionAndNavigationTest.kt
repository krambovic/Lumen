package com.lumen.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Home
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.foundation.layout.padding
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.lumen.app.navigation.openMainTab
import com.lumen.app.navigation.backOrMinimize
import com.lumen.ui.components.*
import com.lumen.ui.screens.*
import com.lumen.ui.theme.LumenTheme
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Isolated native components and Navigation; no user database or VPN access. */
class MotionAndNavigationTest {
    @get:Rule val compose = createAndroidComposeRule<MotionTestActivity>()

    @Test fun sliderReservesViewportAndOccupiesHiddenDockSlot() {
        val visible = mutableStateOf(true)
        compose.setContent { LumenTheme {
            Scaffold(bottomBar = { BottomConnectionControls(true, visible.value, ConnectionState.Disconnected,
                listOf(DockItem("home", "Home", Icons.Default.Home)), "home", {}, {}) }) { inset ->
                Box(Modifier.fillMaxSize().padding(inset)) { Box(Modifier.fillMaxSize().testTag("body")) }
            }
        } }
        val initial = compose.onNodeWithContentDescription("Connect").fetchSemanticsNode().boundsInRoot
        val body = compose.onNodeWithTag("body").fetchSemanticsNode().boundsInRoot
        assertTrue("Content must end before the slider", body.bottom <= initial.top + 1f)
        compose.runOnIdle { visible.value = false }
        compose.mainClock.advanceTimeBy(1200)
        compose.waitForIdle()
        val lowered = compose.onNodeWithContentDescription("Connect").fetchSemanticsNode().boundsInRoot
        assertTrue("Slider must occupy the former dock slot", lowered.bottom > initial.bottom + 40f)
        val expandedBody = compose.onNodeWithTag("body").fetchSemanticsNode().boundsInRoot
        assertTrue(expandedBody.bottom <= lowered.top + 1f)
    }

    @Test fun defaultConnectionTileKeepsItsHeightAcrossEveryState() {
        val state = mutableStateOf(ConnectionState.Disconnected)
        compose.setContent { LumenTheme {
            Box(Modifier.testTag("stage")) { ConnectionStage(state.value, "A long server name", {}) }
        } }
        val height = compose.onNodeWithTag("stage").fetchSemanticsNode().boundsInRoot.height
        assertTrue("The basic connection tile must remain compact", height <= with(compose.density) { 220.dp.toPx() })
        listOf(ConnectionState.Connecting, ConnectionState.Connected, ConnectionState.Disconnected, ConnectionState.Error).forEach {
            compose.runOnIdle { state.value = it }
            compose.mainClock.advanceTimeBy(1200)
            compose.waitForIdle()
            assertEquals(height, compose.onNodeWithTag("stage").fetchSemanticsNode().boundsInRoot.height, 1f)
        }
    }

    @Test fun connectionGlyphStaysCenteredThroughoutTheLoadingTransition() {
        val state = mutableStateOf(ConnectionState.Connecting)
        compose.setContent { LumenTheme { ConnectionStage(state.value, "Server", {}) } }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { state.value = ConnectionState.Connected }
        repeat(32) {
            compose.mainClock.advanceTimeBy(16)
            compose.waitForIdle()
            val center = compose.onNodeWithTag(LumenMotionTags.ConnectionIndicator, useUnmergedTree = true)
                .fetchSemanticsNode().boundsInRoot.center
            val glyphs = compose.onAllNodesWithTag(LumenMotionTags.ConnectionGlyph, useUnmergedTree = true)
                .fetchSemanticsNodes()
            assertTrue("The transition must retain at least one glyph", glyphs.isNotEmpty())
            glyphs.forEach { glyph ->
                    assertEquals(center.x, glyph.boundsInRoot.center.x, 1f)
                    assertEquals(center.y, glyph.boundsInRoot.center.y, 1f)
                }
        }
        compose.mainClock.autoAdvance = true
    }

    @Test fun pickerKeepsItsHeaderStillAtTheEndAndPingsWithoutSelecting() {
        val nodes = (0 until 50).map {
            NodeUiModel("node-$it", "Node ${it.toString().padStart(2, '0')}", "vless", "example.invalid", 443, pingMs = it + 1)
        }
        var requested: List<NodeUiModel> = emptyList()
        var selected: NodeUiModel? = null
        val labels = stringsForLanguage("en")
        compose.setContent { LumenTheme {
            DashboardScreen(ConnectionState.Disconnected, nodes, emptyList(),
                onToggleConnection = {}, onSelectNode = { selected = it }, onImportClipboard = {},
                onImportFile = {}, onImportQr = {}, onAddManualNode = {}, onPingNodes = { requested = it })
        } }
        compose.onNodeWithContentDescription(labels.changeServer).performClick()
        val headerY = compose.onNodeWithTag(LumenMotionTags.PickerHeader).fetchSemanticsNode().boundsInRoot.top
        val list = compose.onNodeWithTag(LumenMotionTags.PickerList)
        list.performScrollToIndex(49)
        repeat(4) {
            list.performTouchInput { swipeUp(durationMillis = 250) }
            compose.waitForIdle()
            assertEquals(headerY, compose.onNodeWithTag(LumenMotionTags.PickerHeader).fetchSemanticsNode().boundsInRoot.top, 1f)
        }
        compose.onNodeWithText(labels.ping).performClick()
        compose.runOnIdle { assertEquals(nodes.map { it.id }, requested.map { it.id }) }
        list.performScrollToIndex(0)
        compose.onNodeWithContentDescription("${labels.ping}: ${nodes[0].name}").performClick()
        compose.runOnIdle {
            assertNull("Pinging a node must not select it", selected)
            assertEquals("A single-node probe must replace the batch request", listOf(nodes[0].id), requested.map { it.id })
        }
    }

    @Test fun rootBackMinimizesWithoutRemovingTheDashboardEntry() {
        lateinit var navigation: NavHostController
        var minimized = 0
        compose.setContent {
            val controller = rememberNavController()
            SideEffect { navigation = controller }
            NavHost(controller, "dashboard") {
                composable("dashboard") {}
                composable("servers") {}
            }
        }
        compose.runOnIdle {
            repeat(2) { navigation.backOrMinimize { minimized++ } }
            assertEquals(2, minimized)
            assertEquals("dashboard", navigation.currentDestination?.route)
            navigation.openMainTab("servers")
        }
        compose.waitForIdle()
        compose.runOnIdle {
            navigation.backOrMinimize { minimized++ }
            assertEquals(2, minimized)
            assertEquals("dashboard", navigation.currentDestination?.route)
        }
    }

    @Test fun backReturnsToServersAndSettingsNeverRestoresLogs() {
        lateinit var navigation: NavHostController
        compose.setContent {
            val controller = rememberNavController()
            SideEffect { navigation = controller }
            NavHost(controller, "dashboard") {
                listOf("dashboard", "servers", "settings", "logs").forEach { route -> composable(route) {} }
            }
        }
        compose.runOnIdle { navigation.openMainTab("servers"); navigation.openMainTab("settings") }
        compose.waitForIdle()
        androidx.test.espresso.Espresso.pressBack()
        compose.runOnIdle { assertEquals("servers", navigation.currentDestination?.route) }
        compose.runOnIdle { navigation.openMainTab("settings"); navigation.navigate("logs") }
        compose.waitForIdle()
        compose.runOnIdle { navigation.openMainTab("settings"); assertEquals("settings", navigation.currentDestination?.route) }
        compose.runOnIdle { navigation.navigate("logs"); navigation.openMainTab("servers"); navigation.openMainTab("settings") }
        compose.waitForIdle()
        compose.runOnIdle { assertEquals("settings", navigation.currentDestination?.route) }
    }
}
