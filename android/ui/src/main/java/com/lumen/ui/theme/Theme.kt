package com.lumen.ui.theme

import android.os.Build
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import com.lumen.ui.screens.ThemePreset

private val LightColorScheme = lightColorScheme(
    primary = Color(0xFF007AA3),
    onPrimary = Color.White,
    secondary = Color(0xFF0284C7),
    background = Color(0xFFF8FAFC),
    onBackground = Color(0xFF0F172A),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF0F172A),
    surfaceVariant = Color(0xFFF1F5F9),
    onSurfaceVariant = Color(0xFF64748B),
    outline = Color(0xFFCBD5E1),
    error = Color(0xFFDC2626)
)

private val DarkBlueColorScheme = darkColorScheme(
    primary = Color(0xFF007AA3),
    onPrimary = Color.White,
    secondary = Color(0xFF38BDF8),
    background = Color(0xFF0D0F17),
    onBackground = Color.White,
    surface = Color(0xFF161926),
    onSurface = Color.White,
    surfaceVariant = Color(0xFF1E2235),
    onSurfaceVariant = Color(0xFF94A3B8),
    outline = Color(0xFF2B3147),
    error = ConnectionDanger
)

private val DraculaColorScheme = darkColorScheme(
    primary = Color(0xFFBD93F9),
    onPrimary = Color.Black,
    secondary = Color(0xFFFF79C6),
    background = Color(0xFF282A36),
    onBackground = Color(0xFFF8F8F2),
    surface = Color(0xFF343746),
    onSurface = Color(0xFFF8F8F2),
    surfaceVariant = Color(0xFF44475A),
    onSurfaceVariant = Color(0xFFD7D2E8),
    outline = Color(0xFF44475A),
    error = ConnectionDanger
)

private val CatppuccinColorScheme = darkColorScheme(
    primary = Color(0xFFCBA6F7),
    onPrimary = Color.Black,
    secondary = Color(0xFF89B4FA),
    background = Color(0xFF11111B),
    onBackground = Color(0xFFCDD6F4),
    surface = Color(0xFF1E1E2E),
    onSurface = Color(0xFFCDD6F4),
    surfaceVariant = Color(0xFF313244),
    onSurfaceVariant = Color(0xFFA6ADC8),
    outline = Color(0xFF313244),
    error = ConnectionDanger
)

private val NordColorScheme = darkColorScheme(
    primary = Color(0xFF88C0D0),
    onPrimary = Color.Black,
    secondary = Color(0xFF81A1C1),
    background = Color(0xFF2E3440),
    onBackground = Color(0xFFECEFF4),
    surface = Color(0xFF3B4252),
    onSurface = Color(0xFFECEFF4),
    surfaceVariant = Color(0xFF434C5E),
    onSurfaceVariant = Color(0xFFD8DEE9),
    outline = Color(0xFF4C566A),
    error = ConnectionDanger
)

private val GithubColorScheme = darkColorScheme(
    primary = Color(0xFF58A6FF),
    onPrimary = Color.Black,
    secondary = Color(0xFF3FB950),
    background = Color(0xFF0D1117),
    onBackground = Color(0xFFE6EDF3),
    surface = Color(0xFF161B22),
    onSurface = Color(0xFFE6EDF3),
    surfaceVariant = Color(0xFF21262D),
    onSurfaceVariant = Color(0xFF8B949E),
    outline = Color(0xFF30363D),
    error = ConnectionDanger
)

private val GruvboxColorScheme = darkColorScheme(
    primary = Color(0xFFFABD2F),
    onPrimary = Color.Black,
    secondary = Color(0xFFFE8019),
    background = Color(0xFF1D2021),
    onBackground = Color(0xFFEBDBB2),
    surface = Color(0xFF32302F),
    onSurface = Color(0xFFEBDBB2),
    surfaceVariant = Color(0xFF3C3836),
    onSurfaceVariant = Color(0xFFD5C4A1),
    outline = Color(0xFF504945),
    error = ConnectionDanger
)

private val TokyoNightColorScheme = darkColorScheme(
    primary = Color(0xFF7AA2F7),
    onPrimary = Color.Black,
    secondary = Color(0xFFBB9AF7),
    background = Color(0xFF16161E),
    onBackground = Color(0xFFC0CAF5),
    surface = Color(0xFF24283B),
    onSurface = Color(0xFFC0CAF5),
    surfaceVariant = Color(0xFF292E42),
    onSurfaceVariant = Color(0xFFA9B1D6),
    outline = Color(0xFF414868),
    error = ConnectionDanger
)

private val MonokaiColorScheme = darkColorScheme(
    primary = Color(0xFFF92672),
    onPrimary = Color.White,
    secondary = Color(0xFFA6E22E),
    background = Color(0xFF1E1F1C),
    onBackground = Color(0xFFF8F8F2),
    surface = Color(0xFF272822),
    onSurface = Color(0xFFF8F8F2),
    surfaceVariant = Color(0xFF3E3D32),
    onSurfaceVariant = Color(0xFFCFCFC2),
    outline = Color(0xFF49483E),
    error = ConnectionDanger
)

private val MaterialColorScheme = darkColorScheme(
    primary = Color(0xFF80CBC4),
    onPrimary = Color.Black,
    secondary = Color(0xFF82AAFF),
    background = Color(0xFF0F111A),
    onBackground = Color(0xFFEEFFFF),
    surface = Color(0xFF202331),
    onSurface = Color(0xFFEEFFFF),
    surfaceVariant = Color(0xFF2B3040),
    onSurfaceVariant = Color(0xFFB0BEC5),
    outline = Color(0xFF33394A),
    error = ConnectionDanger
)

private val SolarizedColorScheme = darkColorScheme(
    primary = Color(0xFF268BD2),
    onPrimary = Color.White,
    secondary = Color(0xFF2AA198),
    background = Color(0xFF002B36),
    onBackground = Color(0xFFEEE8D5),
    surface = Color(0xFF0A3A46),
    onSurface = Color(0xFFEEE8D5),
    surfaceVariant = Color(0xFF164B58),
    onSurfaceVariant = Color(0xFF93A1A1),
    outline = Color(0xFF586E75),
    error = ConnectionDanger
)

@Composable
fun LumenTheme(
    themePreset: ThemePreset = ThemePreset.DARK,
    useAmoledBlack: Boolean = false,
    useMaterialYou: Boolean = false,
    content: @Composable () -> Unit
) {
    val context = LocalContext.current
    val baseScheme = when {
        useMaterialYou && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            if (themePreset == ThemePreset.LIGHT) dynamicLightColorScheme(context)
            else dynamicDarkColorScheme(context)
        }
        themePreset == ThemePreset.LIGHT -> LightColorScheme
        themePreset == ThemePreset.DARK -> DarkBlueColorScheme
        themePreset == ThemePreset.DRACULA -> DraculaColorScheme
        themePreset == ThemePreset.CATPPUCCIN -> CatppuccinColorScheme
        themePreset == ThemePreset.NORD -> NordColorScheme
        themePreset == ThemePreset.GITHUB -> GithubColorScheme
        themePreset == ThemePreset.GRUVBOX -> GruvboxColorScheme
        themePreset == ThemePreset.TOKYO_NIGHT -> TokyoNightColorScheme
        themePreset == ThemePreset.MONOKAI -> MonokaiColorScheme
        themePreset == ThemePreset.MATERIAL -> MaterialColorScheme
        themePreset == ThemePreset.SOLARIZED -> SolarizedColorScheme
        else -> DarkBlueColorScheme
    }

    val finalScheme = if (useAmoledBlack && themePreset != ThemePreset.LIGHT) {
        baseScheme.copy(background = Color.Black, surface = Color(0xFF0F0F0F))
    } else {
        baseScheme
    }

    MaterialTheme(
        colorScheme = finalScheme,
        typography = Typography,
        content = content
    )
}
