package com.lumen.ui.theme



import android.os.Build

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi

import androidx.compose.material3.darkColorScheme

import androidx.compose.material3.dynamicDarkColorScheme

import androidx.compose.material3.dynamicLightColorScheme

import androidx.compose.material3.lightColorScheme

import androidx.compose.foundation.shape.RoundedCornerShape

import androidx.compose.material3.Shapes

import androidx.compose.runtime.Composable

import androidx.compose.runtime.CompositionLocalProvider

import androidx.compose.runtime.SideEffect

import androidx.compose.runtime.staticCompositionLocalOf

import androidx.compose.ui.unit.dp

import androidx.compose.ui.graphics.Color

import androidx.compose.ui.graphics.compositeOver

import androidx.compose.ui.graphics.luminance

import androidx.compose.ui.platform.LocalContext

import com.lumen.ui.screens.ThemePreset



// Light and dark follow the 2.x redesign: neutral greys, one blue accent for actions,

// green reserved for the connected state (see LumenColors).

private val LightColorScheme = lightColorScheme(

    primary = Color(0xFF6937D4),

    onPrimary = Color.White,

    primaryContainer = Color(0xFFE7D8FF),

    onPrimaryContainer = Color(0xFF492676),

    secondary = Color(0xFF6750A4),

    background = Color(0xFFFAF7F2),

    onBackground = Color(0xFF271C32),

    surface = Color(0xFFF0EBF1),

    onSurface = Color(0xFF271C32),

    surfaceVariant = Color(0xFFF0EBF1),

    onSurfaceVariant = Color(0xFF686075),

    surfaceContainer = Color(0xFFEAE1F0),

    surfaceContainerHigh = Color(0xFFDDD7EA),

    outline = Color(0xFFCBC5D7),

    error = Color(0xFFC42B3B)

)



private val DarkBlueColorScheme = darkColorScheme(

    primary = Color(0xFFD5B8FF),

    onPrimary = Color(0xFF08182C),

    primaryContainer = Color(0xFF382047),

    onPrimaryContainer = Color(0xFFF0DAFF),

    secondary = Color(0xFFD5B8FF),

    background = Color(0xFF09080D),

    onBackground = Color(0xFFE5E0F0),

    surface = Color(0xFF17121F),

    onSurface = Color(0xFFE5E0F0),

    surfaceVariant = Color(0xFF17121F),

    onSurfaceVariant = Color(0xFFBCB5CD),

    surfaceContainer = Color(0xFF251A32),

    surfaceContainerHigh = Color(0xFF432859),

    outline = Color(0xFF494551),

    error = Color(0xFFFF6B78)

)



/** Status and expressive design colours that Material's scheme has no slot for. */

data class LumenColors(

    val connected: Color,

    val onConnected: Color,

    val connectedContainer: Color,

    val warning: Color,

    val mint: Color = Color(0xFFD5EDDF),

    val onMint: Color = Color(0xFF224E38),

    val statBlue: Color = Color(0xFFD8E8F9),

    val hero: Color = Color(0xFF6937D4),

    val onHero: Color = Color(0xFFECE9FF),

    val raised: Color = Color(0xFFEAE1F0),

    val tonal: Color = Color(0xFFE7D8FF),

    val onTonal: Color = Color(0xFF492676),

    val line: Color = Color(0xFFCBC5D7),

    val surface: Color = Color(0xFFF0EBF1),

    val ink: Color = Color(0xFF271C32),

    val muted: Color = Color(0xFF686075),

    val accent: Color = Color(0xFF6937D4)

)



private val LightLumenColors = LumenColors(

    connected = Color(0xFF0E7A52),

    onConnected = Color(0xFFF4F6F9),

    connectedContainer = Color(0xFFDCF2E8),

    warning = Color(0xFF9A5800),

    mint = LumenLightMint,

    onMint = LumenLightOnMint,

    statBlue = LumenLightBlueStat,

    hero = LumenLightHero,

    onHero = LumenOnHero,

    raised = LumenLightRaised,

    tonal = LumenLightTonal,

    onTonal = LumenLightOnTonal,

    line = LumenLightLine,

    surface = LumenLightSurface,

    ink = LumenLightInk,

    muted = LumenLightMuted,

    accent = LumenLightAccent

)



private val DarkLumenColors = LumenColors(

    connected = Color(0xFF3DD68C),

    onConnected = Color(0xFF0E1015),

    connectedContainer = Color(0xFF12321F),

    warning = Color(0xFFF2B544),

    mint = LumenDarkMint,

    onMint = LumenDarkOnMint,

    statBlue = LumenDarkBlueStat,

    hero = LumenDarkHero,

    onHero = LumenOnHero,

    raised = LumenDarkRaised,

    tonal = LumenDarkTonal,

    onTonal = LumenDarkOnTonal,

    line = LumenDarkLine,

    surface = LumenDarkSurface,

    ink = LumenDarkInk,

    muted = LumenDarkMuted,

    accent = LumenDarkAccent

)



val LocalLumenColors = staticCompositionLocalOf { DarkLumenColors }



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



// Rosé Pine (main variant) published palette: base/surface/overlay/highlight-high,

// text, iris and foam. Matches the Rose Pine theme shipped by the desktop client.

private val RosePineColorScheme = darkColorScheme(

    primary = Color(0xFFC4A7E7),

    onPrimary = Color(0xFF191724),

    secondary = Color(0xFF9CCFD8),

    background = Color(0xFF191724),

    onBackground = Color(0xFFE0DEF4),

    surface = Color(0xFF1F1D2E),

    onSurface = Color(0xFFE0DEF4),

    surfaceVariant = Color(0xFF26233A),

    onSurfaceVariant = Color(0xFF908CAA),

    outline = Color(0xFF524F67),

    error = ConnectionDanger

)



private val LumenShapes = Shapes(

    extraSmall = RoundedCornerShape(8.dp),

    small = RoundedCornerShape(12.dp),

    medium = RoundedCornerShape(14.dp),

    large = RoundedCornerShape(20.dp),

    extraLarge = RoundedCornerShape(28.dp)

)



@Composable

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
fun LumenTheme(

    themePreset: ThemePreset = ThemePreset.DARK,

    useAmoledBlack: Boolean = false,

    useMaterialYou: Boolean = false,
    themeMode: com.lumen.ui.screens.ThemeMode? = null,
    accentColor: String = "",
    language: String = "",

    content: @Composable () -> Unit

) {

    val context = LocalContext.current

    val light = when (themeMode) {
        com.lumen.ui.screens.ThemeMode.SYSTEM -> !androidx.compose.foundation.isSystemInDarkTheme()
        com.lumen.ui.screens.ThemeMode.LIGHT -> true
        com.lumen.ui.screens.ThemeMode.DARK -> false
        null -> themePreset == ThemePreset.LIGHT
    }

    // Every theme shares the redesign's neutral greys; a preset (or Material You)

    // only supplies the accent colour.

    val base = if (light) LightColorScheme else DarkBlueColorScheme

    val accentSource = when {

        useMaterialYou && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {

            if (light) dynamicLightColorScheme(context) else dynamicDarkColorScheme(context)

        }

        themePreset == ThemePreset.DRACULA -> DraculaColorScheme

        themePreset == ThemePreset.CATPPUCCIN -> CatppuccinColorScheme

        themePreset == ThemePreset.NORD -> NordColorScheme

        themePreset == ThemePreset.GITHUB -> GithubColorScheme

        themePreset == ThemePreset.GRUVBOX -> GruvboxColorScheme

        themePreset == ThemePreset.TOKYO_NIGHT -> TokyoNightColorScheme

        themePreset == ThemePreset.MONOKAI -> MonokaiColorScheme

        themePreset == ThemePreset.MATERIAL -> MaterialColorScheme

        themePreset == ThemePreset.SOLARIZED -> SolarizedColorScheme

        themePreset == ThemePreset.ROSE_PINE -> RosePineColorScheme

        else -> base

    }

    val customAccent = accentColor.takeIf { it.matches(Regex("#[0-9a-fA-F]{6}")) }
        ?.drop(1)?.toLongOrNull(16)?.let { Color(0xFF000000 or it) }
    val accent = customAccent ?: accentSource.primary

    val onAccent = if (accent.luminance() > .179f) Color(0xFF0E1015) else Color.White

    // Presets own the whole scheme, not just one accent on Lumen's purple base.
    // Keep their native surfaces in dark mode and derive light surfaces from the same hues.
    val paletteBase = if (accentSource === base || useMaterialYou) accentSource else if (!light) accentSource else base.copy(
        background = accentSource.primary.copy(alpha = .035f).compositeOver(base.background),
        surface = accentSource.primary.copy(alpha = .045f).compositeOver(base.surface),
        primary = accentSource.primary.copy(alpha = .72f).compositeOver(Color.Black),
        secondary = accentSource.secondary.copy(alpha = .65f).compositeOver(Color.Black),
        tertiary = accentSource.error.copy(alpha = .65f).compositeOver(Color.Black)
    )
    val paletteAccent = customAccent ?: paletteBase.primary
    val accented = if (accentSource === base && customAccent == null) base else paletteBase.copy(
        primary = paletteAccent,
        onPrimary = if (paletteAccent.luminance() > .179f) Color(0xFF0E1015) else Color.White,
        onSecondary = if (paletteBase.secondary.luminance() > .179f) Color(0xFF0E1015) else Color.White,
        primaryContainer = paletteAccent.copy(alpha = if (light) .16f else .24f).compositeOver(paletteBase.surface),
        onPrimaryContainer = paletteBase.onSurface
    )

    val tuned = accented.copy(

        outline = accented.onSurface.copy(alpha = 0.30f).compositeOver(accented.surface),

        outlineVariant = accented.primary.copy(alpha = .16f).compositeOver(accented.background),
        surfaceDim = accented.background,
        surfaceBright = accented.primary.copy(alpha = .20f).compositeOver(accented.background),
        surfaceContainerLowest = accented.background,
        surfaceContainerLow = accented.primary.copy(alpha = if (light) .04f else .045f).compositeOver(accented.background),
        surfaceContainer = accented.primary.copy(alpha = if (light) .08f else .08f).compositeOver(accented.background),
        surfaceContainerHigh = accented.primary.copy(alpha = if (light) .12f else .13f).compositeOver(accented.background),
        surfaceContainerHighest = accented.primary.copy(alpha = if (light) .16f else .19f).compositeOver(accented.background),
        secondaryContainer = accented.secondary.copy(alpha = .22f).compositeOver(accented.background),
        onSecondaryContainer = accented.onSurface,
        tertiaryContainer = accented.tertiary.copy(alpha = .22f).compositeOver(accented.background),
        onTertiaryContainer = accented.onSurface

    )



    // AMOLED keeps the same structure and only drops every surface to (near) black.

    val finalScheme = if (useAmoledBlack && !light) {

        tuned.copy(

            background = Color.Black,

            surface = Color(0xFF0C0D10),

            surfaceVariant = Color(0xFF0C0D10),

            surfaceDim = Color.Black,

            surfaceBright = Color(0xFF1A1C21),

            surfaceContainerLowest = Color.Black,

            surfaceContainerLow = Color(0xFF0C0D10),

            surfaceContainer = Color(0xFF15171B),

            surfaceContainerHigh = Color(0xFF1A1C21),

            surfaceContainerHighest = Color(0xFF1F2127),

            primaryContainer = accent.copy(alpha = 0.18f).compositeOver(Color(0xFF0C0D10)),

            outline = Color(0xFF3A3D45),

            outlineVariant = Color(0xFF22252B),

            scrim = Color.Black

        )

    } else {

        tuned

    }



    val presetColors = if (light) LightLumenColors else DarkLumenColors
    val lumenColors = presetColors.copy(
        accent = finalScheme.primary,
        tonal = finalScheme.primaryContainer,
        onTonal = finalScheme.onPrimaryContainer,
        raised = finalScheme.surfaceContainer,
        surface = finalScheme.surface,
        ink = finalScheme.onSurface,
        muted = finalScheme.onSurfaceVariant,
        line = finalScheme.outlineVariant,
        hero = if (accentSource === base && customAccent == null) presetColors.hero else finalScheme.primary,
        onHero = if (accentSource === base && customAccent == null) presetColors.onHero else finalScheme.onPrimary
    )

    SideEffect {

        com.lumen.ui.screens.PingColors.good = lumenColors.connected

        com.lumen.ui.screens.PingColors.fair = lumenColors.warning

        com.lumen.ui.screens.PingColors.bad = finalScheme.error

    }

    CompositionLocalProvider(LocalLumenColors provides lumenColors, LocalExpressivePreset provides themePreset) {

        MaterialExpressiveTheme(

            colorScheme = finalScheme,

            typography = if (language == "fa") PersianTypography else Typography,

            shapes = LumenShapes,

            content = content

        )

    }

}
