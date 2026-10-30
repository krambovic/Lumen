package com.lumen.ui.components

import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.lumen.ui.theme.LocalLumenColors

data class DockItem(val id: String, val label: String, val icon: ImageVector)

/** Visibility only changes a retained graphics layer, never the page's layout. */
@Composable
fun FloatingNavigationDock(
    items: List<DockItem>, activeId: String, visible: Boolean, onSelect: (String) -> Unit,
    modifier: Modifier = Modifier, visibilityProgress: (() -> Float)? = null,
    trailingAction: (@Composable () -> Unit)? = null
) {
    val progress = if (visibilityProgress == null) animateFloatAsState(if (visible) 1f else 0f,
        spring(dampingRatio = 1f, stiffness = 550f), label = "dock_visibility") else null
    Surface(
        modifier.graphicsLayer {
            val fraction = (visibilityProgress?.invoke() ?: progress!!.value).coerceIn(0f, 1f)
            alpha = fraction
            translationY = (1f - fraction) * (size.height + 16.dp.toPx())
        }.then(if (visible) Modifier else Modifier.clearAndSetSemantics { }),
        shape = RoundedCornerShape(32.dp), color = LocalLumenColors.current.raised.copy(alpha = .88f),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = .35f)),
        shadowElevation = 0.dp
    ) {
        // Label expansion owns width animation. A second animateContentSize/padding
        // animation here would remeasure the same content twice with different springs.
        Row(Modifier.selectableGroup().padding(4.dp), verticalAlignment = Alignment.CenterVertically) {
            items.forEach { item -> key(item.id) {
                DockTab(item, item.id == activeId, visible, compact = trailingAction != null) { onSelect(item.id) }
            } }
            if (trailingAction != null) {
                Box(Modifier.padding(horizontal = 6.dp).width(1.dp).height(26.dp)
                    .background(MaterialTheme.colorScheme.outlineVariant))
                trailingAction()
            }
        }
    }
}

@Composable
private fun DockTab(item: DockItem, active: Boolean, enabled: Boolean, compact: Boolean = false, onClick: () -> Unit) {
    val background = animateColorAsState(if (active) MaterialTheme.colorScheme.primary else Color.Transparent,
        tween(180), label = "dock_tab_background")
    val foreground = animateColorAsState(if (active) MaterialTheme.colorScheme.onPrimary else LocalLumenColors.current.muted,
        tween(180), label = "dock_tab_foreground")
    val scale = animateFloatAsState(if (active) 1.12f else 1f,
        spring(dampingRatio = .85f, stiffness = 600f), label = "dock_tab_icon")
    Row(Modifier.height(46.dp).widthIn(min = 48.dp).clip(RoundedCornerShape(26.dp))
        .drawBehind { drawRect(background.value) }
        .expressiveControlLabel(item.label)
        .selectable(active, enabled = enabled, role = Role.Tab, onClick = onClick)
        .padding(horizontal = if (compact) 10.dp else 14.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center) {
        DockIcon(item.icon, item.label, foreground) { scale.value }
        AnimatedVisibility(active,
            enter = expandHorizontally(spring(dampingRatio = 1f, stiffness = 550f)) + fadeIn(tween(140)),
            exit = shrinkHorizontally(spring(dampingRatio = 1f, stiffness = 550f)) + fadeOut(tween(100))) {
            DockLabel(item.label, foreground)
        }
    }
}

@Composable
private fun DockIcon(icon: ImageVector, label: String, color: State<Color>, scale: () -> Float) {
    Icon(icon, label, Modifier.size(20.dp).graphicsLayer { scaleX = scale(); scaleY = scale() }, tint = color.value)
}

@Composable
private fun DockLabel(label: String, color: State<Color>) {
    Text(label, Modifier.padding(start = 7.dp), color = color.value,
        style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, maxLines = 1)
}

/** Hysteresis prevents tiny reversals and end-of-fling noise from restarting the dock animation. */
class DockScrollVisibility(private val thresholdPx: Float, private val keepHiddenAtBottom: Boolean = false) {
    private var distance = 0f
    private var travel = 0f
    var visible: Boolean = true
        private set

    fun reset() { distance = 0f; travel = 0f; visible = true }

    fun onScroll(consumedY: Float, availableY: Float, direct: Boolean = true): Boolean {
        if (kotlin.math.abs(consumedY) < .5f && kotlin.math.abs(availableY) > .5f) {
            if (availableY > 0f) travel = 0f
            distance = 0f
            // A downward end-of-list delta is not an upward intent. Revealing here
            // grows the footer and creates a new scroll range, reversing the slider.
            // Pages that never scrolled, and the top boundary, keep navigation visible.
            visible = !keepHiddenAtBottom || availableY > 0f || travel < thresholdPx * 2f
        } else if (consumedY != 0f) {
            travel = (travel - consumedY).coerceAtLeast(0f)
            if (distance * consumedY < 0f) distance = 0f
            distance += consumedY
            when {
                travel < thresholdPx * 2f -> visible = true // Protect the top controls.
                distance <= -thresholdPx * 1.5f -> { visible = false; distance = 0f }
                distance >= thresholdPx * (if (direct) .8f else 2.5f) -> { visible = true; distance = 0f }
            }
        }
        return visible
    }

    fun onFling(velocityY: Float): Boolean {
        if (velocityY > 600f) { visible = true; distance = 0f }
        else if (velocityY < -1400f && travel >= thresholdPx * 2f) { visible = false; distance = 0f }
        return visible
    }
}
