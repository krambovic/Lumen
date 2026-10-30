package com.lumen.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.geometry.Offset
import androidx.graphics.shapes.Morph
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

enum class Motif { Flower, Clover, Sun, Fan, Pebble, Arch }

fun Modifier.expressiveControlLabel(label: String): Modifier = semantics(mergeDescendants = true) {
    contentDescription = label
}

/** One accessible action for a control whose graphics and text span multiple layers. */
fun Modifier.expressiveButtonSemantics(label: String, action: () -> Unit, enabled: Boolean = true): Modifier =
    clearAndSetSemantics {
        contentDescription = label
        role = Role.Button
        if (enabled) onClick(label = label) { action(); true } else disabled()
    }

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun expressiveShape(motif: Motif): Shape = when (motif) {
    Motif.Flower -> MaterialShapes.Flower
    Motif.Clover -> MaterialShapes.Clover4Leaf
    Motif.Sun -> MaterialShapes.Cookie9Sided
    Motif.Fan -> MaterialShapes.Fan
    Motif.Pebble -> MaterialShapes.PuffyDiamond
    Motif.Arch -> MaterialShapes.Arch
}.toShape()

@Composable
fun ExpressiveMotif(motif: Motif, color: Color, modifier: Modifier = Modifier) {
    Box(modifier.clip(expressiveShape(motif)).background(color))
}

/** The pressed motif actually morphs into a circle; progress is read only while drawing. */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MorphingExpressiveMotif(motif: Motif, color: Color, progress: () -> Float, modifier: Modifier = Modifier) {
    val morph = remember(motif) {
        val polygon = when (motif) {
            Motif.Flower -> MaterialShapes.Flower
            Motif.Clover -> MaterialShapes.Clover4Leaf
            Motif.Sun -> MaterialShapes.Cookie9Sided
            Motif.Fan -> MaterialShapes.Fan
            Motif.Pebble -> MaterialShapes.PuffyDiamond
            Motif.Arch -> MaterialShapes.Arch
        }
        Morph(polygon, MaterialShapes.Circle)
    }
    Box(modifier.drawWithCache {
        var cachedProgress = Float.NaN
        var cachedPath: androidx.compose.ui.graphics.Path? = null
        val matrix = Matrix()
        onDrawBehind {
            val current = progress().coerceIn(0f, 1f)
            if (current != cachedProgress) {
                val path = morph.toPath(current)
                val bounds = path.getBounds()
                if (bounds.width > 0f && bounds.height > 0f) {
                    path.translate(Offset(-bounds.left, -bounds.top))
                    matrix.reset()
                    matrix.scale(size.width / bounds.width, size.height / bounds.height)
                    path.transform(matrix)
                }
                cachedPath = path
                cachedProgress = current
            }
            cachedPath?.let { drawPath(it, color) }
        }
    })
}

/** Labeled actions retain platform ripple, a 56dp target and a springy silhouette. */
@Composable
fun ExpressiveAction(
    label: String, onClick: () -> Unit, modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    container: Color = MaterialTheme.colorScheme.primary,
    content: Color = MaterialTheme.colorScheme.onPrimary,
    enabled: Boolean = true
) {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val round = animateFloatAsState(if (pressed) 18f else 32f,
        spring(dampingRatio = .6f, stiffness = 500f), label = "action_shape")
    Row(modifier.expressiveButtonSemantics(label, onClick, enabled).heightIn(min = 56.dp)
        .graphicsLayer { shape = RoundedCornerShape(round.value.dp); clip = true }
        .background(container.copy(alpha = if (enabled) 1f else .38f))
        .clickable(interactionSource = source, indication = androidx.compose.foundation.LocalIndication.current,
            enabled = enabled, role = Role.Button, onClick = onClick)
        .padding(horizontal = 20.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        if (icon != null) {
            Icon(icon, null, tint = content, modifier = Modifier.size(22.dp))
            Spacer(Modifier.width(10.dp))
        }
        Text(label, style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.Bold, color = content)
    }
}
