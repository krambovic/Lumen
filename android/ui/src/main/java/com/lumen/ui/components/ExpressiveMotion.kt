package com.lumen.ui.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.toPath
import androidx.graphics.shapes.Morph
import androidx.compose.animation.core.spring
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.runtime.State
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.withFrameNanos
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.isActive
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

fun expressivePageEnter(forward: Boolean = true): EnterTransition =
    slideInHorizontally(spring(dampingRatio = .9f, stiffness = 420f)) {
        if (forward) it / 20 else -it / 20
    } + scaleIn(spring(dampingRatio = .88f, stiffness = 420f), initialScale = .955f,
        transformOrigin = TransformOrigin(.5f, .42f)) + fadeIn(spring(stiffness = 750f))

fun expressivePageExit(forward: Boolean = true): ExitTransition =
    slideOutHorizontally(spring(dampingRatio = 1f, stiffness = 700f)) {
        if (forward) -it / 40 else it / 40
    } + scaleOut(spring(dampingRatio = 1f, stiffness = 650f), targetScale = .985f,
        transformOrigin = TransformOrigin(.5f, .42f)) + fadeOut(spring(stiffness = 1500f))

/** Read in a graphics layer/draw block so rotation does not recompose the tile. */
@Composable
fun rememberDecorativeRotation(durationMillis: Int = 32000): State<Float> =
    rememberForegroundAngle { 360_000f / durationMillis.coerceAtLeast(1) }

/** Continuous phase survives speed changes and pauses completely while the UI is not resumed. */
@Composable
internal fun rememberForegroundAngle(
    enabled: Boolean = true,
    degreesPerSecond: () -> Float
): State<Float> {
    val angle = remember { mutableFloatStateOf(0f) }
    val speed = rememberUpdatedState(degreesPerSecond)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(lifecycle, enabled) {
        if (!enabled) return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            var previous = withFrameNanos { it }
            while (isActive) {
                withFrameNanos { now ->
                    // Discard a suspended/stalled frame's long gap instead of jumping on resume.
                    val seconds = ((now - previous) / 1_000_000_000f).coerceIn(0f, .05f)
                    previous = now
                    angle.floatValue = (angle.floatValue + seconds * speed.value()).mod(360f)
                }
            }
        }
    }
    return angle
}

/** Selection geometry uses the same interruptible spring throughout the app. */
@Composable
fun Modifier.animatedSelectionShape(selected: Boolean, idle: Dp = 12.dp, active: Dp = 28.dp): Modifier {
    val outline = selectionShape(selected, idle, active)
    return graphicsLayer { shape = outline; clip = true }
}

@Composable
fun selectionShape(selected: Boolean, idle: Dp = 12.dp, active: Dp = 28.dp): androidx.compose.ui.graphics.Shape {
    val corner = animateDpAsState(if (selected) active else idle,
        spring(dampingRatio = .75f, stiffness = 380f), label = "selection_shape")
    return androidx.compose.runtime.remember(corner) {
        object : androidx.compose.ui.graphics.Shape {
            override fun createOutline(size: androidx.compose.ui.geometry.Size,
                layoutDirection: androidx.compose.ui.unit.LayoutDirection,
                density: androidx.compose.ui.unit.Density): androidx.compose.ui.graphics.Outline =
                RoundedCornerShape(corner.value).createOutline(size, layoutDirection, density)
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun connectionMorphShape(state: ConnectionState): androidx.compose.ui.graphics.Shape {
    val progress = animateFloatAsState(when (state) {
        ConnectionState.Connecting -> .45f
        ConnectionState.Connected -> 1f
        else -> 0f
    }, spring(dampingRatio = .72f, stiffness = 260f), label = "connection_morph")
    val morph = androidx.compose.runtime.remember { Morph(MaterialShapes.Clover4Leaf, MaterialShapes.Flower) }
    return androidx.compose.runtime.remember(morph, progress) {
        object : androidx.compose.ui.graphics.Shape {
            override fun createOutline(size: androidx.compose.ui.geometry.Size,
                layoutDirection: androidx.compose.ui.unit.LayoutDirection,
                density: androidx.compose.ui.unit.Density): androidx.compose.ui.graphics.Outline {
                val path = morph.toPath(progress.value.coerceIn(0f, 1f))
                val bounds = path.getBounds()
                if (bounds.width > 0f && bounds.height > 0f) {
                    path.translate(androidx.compose.ui.geometry.Offset(-bounds.left, -bounds.top))
                    path.transform(androidx.compose.ui.graphics.Matrix().apply {
                        scale(size.width / bounds.width, size.height / bounds.height)
                    })
                }
                return androidx.compose.ui.graphics.Outline.Generic(path)
            }
        }
    }
}
