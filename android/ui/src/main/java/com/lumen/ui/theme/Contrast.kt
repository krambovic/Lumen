package com.lumen.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.luminance
import androidx.compose.material3.ColorScheme

internal fun contrastRatio(background: Color, foreground: Color): Float {
    val bg = background.luminance()
    val fg = foreground.compositeOver(background).luminance()
    return (maxOf(bg, fg) + .05f) / (minOf(bg, fg) + .05f)
}

/** Keep the theme's ink when readable; custom/system palettes still get a valid pair. */
fun readableForeground(background: Color, preferred: Color): Color =
    if (contrastRatio(background, preferred) >= 4.5f) preferred
    else if (contrastRatio(background, Color.Black) >= contrastRatio(background, Color.White)) Color.Black
    else Color.White

internal fun ColorScheme.withReadableContent(): ColorScheme = copy(
    onPrimary = readableForeground(primary, onPrimary),
    onSecondary = readableForeground(secondary, onSecondary),
    onTertiary = readableForeground(tertiary, onTertiary),
    onPrimaryContainer = readableForeground(primaryContainer, onPrimaryContainer),
    onSecondaryContainer = readableForeground(secondaryContainer, onSecondaryContainer),
    onTertiaryContainer = readableForeground(tertiaryContainer, onTertiaryContainer),
    onBackground = readableForeground(background, onBackground),
    onSurface = readableForeground(surface, onSurface),
    onSurfaceVariant = readableForeground(surfaceVariant, onSurfaceVariant),
    onError = readableForeground(error, onError),
    onErrorContainer = readableForeground(errorContainer, onErrorContainer)
)
