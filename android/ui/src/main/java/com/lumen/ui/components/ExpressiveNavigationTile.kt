package com.lumen.ui.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.LumenMono

/** A navigation destination, never a quick toggle. The index establishes catalogue rhythm. */
@Composable
fun ExpressiveNavigationTile(
    title: String, description: String, icon: ImageVector, index: String,
    container: Color, content: Color, accent: Color, motif: Motif,
    onClick: () -> Unit, modifier: Modifier = Modifier, prominent: Boolean = false,
    interactive: Boolean = true
) {
    fun contrast(a: Color, b: Color): Float {
        val x = a.luminance(); val y = b.luminance()
        return (maxOf(x, y) + .05f) / (minOf(x, y) + .05f)
    }
    val iconInk = if (contrast(accent, content) >= contrast(accent, container)) content else container
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val radius = animateDpAsState(if (pressed || !interactive) 10.dp else if (prominent) 42.dp else 32.dp,
        spring(dampingRatio = .65f, stiffness = 500f), label = "catalogue_radius")
    val rotation by animateFloatAsState(if (pressed) -28f else 12f,
        spring(dampingRatio = .55f, stiffness = 300f), label = "catalogue_motif")
    val morph = animateFloatAsState(if (pressed) 1f else 0f,
        spring(dampingRatio = .6f, stiffness = 350f), label = "catalogue_morph")
    val ambientRotation = if (interactive) rememberDecorativeRotation(24000 + motif.ordinal * 4000) else null
    Box(modifier.then(if (interactive) Modifier.expressiveButtonSemantics(
        listOf(title, description).filter { it.isNotBlank() }.joinToString(". "), onClick) else Modifier)
        .graphicsLayer {
            shape = RoundedCornerShape(topStart = radius.value, topEnd = radius.value,
                bottomEnd = radius.value, bottomStart = if (prominent) 12.dp else radius.value)
            clip = true
        }
        .background(container)
        .then(if (interactive) Modifier.clickable(interactionSource = source,
            indication = androidx.compose.foundation.LocalIndication.current,
            role = Role.Button, onClick = onClick) else Modifier)) {
        MorphingExpressiveMotif(motif, content.copy(alpha = .1f), { morph.value },
            Modifier.align(Alignment.BottomEnd).offset(x = 30.dp, y = 32.dp).size(if (prominent) 184.dp else 126.dp)
                .graphicsLayer {
                    rotationZ = rotation + (ambientRotation?.value ?: 0f) * if (motif.ordinal % 2 == 0) 1f else -1f
                })
        Column(Modifier.fillMaxSize().padding(if (prominent) 22.dp else 16.dp),
            verticalArrangement = Arrangement.SpaceBetween) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.Top) {
                if (interactive) Text(index, fontFamily = LumenMono, style = MaterialTheme.typography.labelMedium,
                    color = content.copy(alpha = .7f))
                else Spacer(Modifier.size(24.dp))
                Box(Modifier.size(if (prominent) 54.dp else 36.dp)
                    .graphicsLayer { rotationZ = rotation }.clip(expressiveShape(motif)).background(accent),
                    contentAlignment = Alignment.Center) {
                    Icon(icon, null, tint = iconInk, modifier = Modifier.size(if (prominent) 27.dp else 20.dp)
                        .graphicsLayer { rotationZ = -rotation })
                }
            }
            Column(Modifier.padding(end = if (interactive) 0.dp else 32.dp)) {
                Text(title, style = (if (prominent) MaterialTheme.typography.headlineMedium else MaterialTheme.typography.titleLarge)
                    .copy(lineBreak = LineBreak.Heading, hyphens = Hyphens.Auto),
                    fontWeight = FontWeight.Bold, color = content, maxLines = 2,
                    overflow = TextOverflow.Ellipsis)
                if (description.isNotBlank()) Text(description, style = MaterialTheme.typography.bodySmall,
                    color = content.copy(alpha = .8f), maxLines = 2, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 4.dp))
            }
        }
    }
}
