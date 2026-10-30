package com.lumen.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import com.lumen.ui.screens.ThemePreset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.compositeOver

/** Art direction tokens. Text and decorative accents always travel as contrast pairs. */
data class ExpressivePalette(
    val stage: Color, val onStage: Color,
    val lime: Color, val onLime: Color,
    val coral: Color, val onCoral: Color,
    val lavender: Color, val onLavender: Color,
    val blue: Color, val onBlue: Color
)

private val LightExpressive = ExpressivePalette(
    stage = Color(0xFF2B1744), onStage = Color(0xFFF2DEFF),
    lime = Color(0xFFE1F59A), onLime = Color(0xFF29350D),
    coral = Color(0xFFFFC5B5), onCoral = Color(0xFF5C281E),
    lavender = Color(0xFFE3CEFF), onLavender = Color(0xFF402365),
    blue = Color(0xFFCEDFFF), onBlue = Color(0xFF20385C)
)
private val DarkExpressive = ExpressivePalette(
    stage = Color(0xFF3D2057), onStage = Color(0xFFF3E4FF),
    lime = Color(0xFFC7ED64), onLime = Color(0xFF29350D),
    coral = Color(0xFF512724), onCoral = Color(0xFFFFDBCF),
    lavender = Color(0xFF321B49), onLavender = Color(0xFFF1DBFF),
    blue = Color(0xFF182C47), onBlue = Color(0xFFD8E6FF)
)

internal val LocalExpressivePreset = staticCompositionLocalOf { ThemePreset.DARK }

@Composable
fun expressivePalette(): ExpressivePalette {
    val scheme = MaterialTheme.colorScheme
    val brand = if (scheme.background.luminance() > .5f) LightExpressive else DarkExpressive
    if (LocalExpressivePreset.current in setOf(ThemePreset.LIGHT, ThemePreset.DARK)) {
        return if (scheme.primary == LumenLightAccent || scheme.primary == LumenDarkAccent) brand
        else brand.copy(stage = scheme.primary, onStage = scheme.onPrimary,
            lavender = scheme.primaryContainer, onLavender = scheme.onPrimaryContainer,
            blue = scheme.secondaryContainer, onBlue = scheme.onSecondaryContainer,
            coral = scheme.tertiaryContainer, onCoral = scheme.onTertiaryContainer)
    }
    return if (scheme.primary == LumenLightAccent || scheme.primary == LumenDarkAccent) brand
    else brand.copy(stage = scheme.primary.copy(alpha = .28f).compositeOver(scheme.background), onStage = scheme.onSurface,
        lime = scheme.secondary, onLime = scheme.onSecondary,
        lavender = scheme.primaryContainer, onLavender = scheme.onPrimaryContainer,
        blue = scheme.secondaryContainer, onBlue = scheme.onSecondaryContainer,
        coral = scheme.tertiaryContainer, onCoral = scheme.onTertiaryContainer)
}
