package com.lumen.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** The slider owns real viewport space; the dock floats on other dashboards. */
@Composable
fun BottomConnectionControls(
    showSlider: Boolean, dockVisible: Boolean, connectionState: ConnectionState,
    items: List<DockItem>, activeId: String, onSelect: (String) -> Unit, onToggleConnection: () -> Unit,
    dockConnection: Boolean = false
) {
    // One clock keeps the slot, gap and dock layer in phase through reversals.
    val progress = animateFloatAsState(if (dockVisible) 1f else 0f,
        spring(dampingRatio = 1f, stiffness = 550f), label = "bottom_controls")
    val dockSpace = (if (dockConnection) 56.dp else 54.dp) * progress.value
    val gap = 12.dp * progress.value
    val navigationInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val reserved = if (showSlider) 76.dp + gap + dockSpace + navigationInset + 8.dp else 0.dp
    Box(Modifier.fillMaxWidth().height(reserved), contentAlignment = Alignment.BottomCenter) {
        Column(Modifier.fillMaxWidth().wrapContentHeight(unbounded = true, align = Alignment.Bottom)
            .navigationBarsPadding().padding(bottom = 8.dp, start = 16.dp, end = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally) {
            if (showSlider) ConnectionSliderBar(connectionState, onToggleConnection,
                Modifier.fillMaxWidth().padding(bottom = gap))
            Box(Modifier.fillMaxWidth().height(dockSpace), contentAlignment = Alignment.TopCenter) {
                FloatingNavigationDock(items, activeId, dockVisible, onSelect,
                    Modifier.wrapContentHeight(unbounded = true), visibilityProgress = { progress.value },
                    trailingAction = if (dockConnection) { { DockConnectionButton(connectionState, onToggleConnection) } } else null)
            }
        }
    }
}
