package com.lumen.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.lumen.ui.R

// Both faces ship as single variable files; each weight pins its own wght axis value.
@OptIn(ExperimentalTextApi::class)
private fun variable(res: Int, weight: FontWeight) = Font(
    res,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight))
)

private val Weights = listOf(
    FontWeight.Normal, FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold, FontWeight.ExtraBold
)

/** Interface text. */
val Inter = FontFamily(Weights.map { variable(R.font.geist_variable, it) })

/** Figures that tick (timer, speed, ping), addresses, keys and log lines. */
val LumenMono = FontFamily(Weights.map { variable(R.font.jetbrains_mono_variable, it) })

private fun TextStyle.lumen() = copy(fontFamily = Inter)

private val Base = Typography()

val Typography = Typography(
    displayLarge = TextStyle(fontWeight = FontWeight.ExtraBold, fontSize = 64.sp, lineHeight = 64.sp, letterSpacing = (-3).sp).lumen(),
    displayMedium = TextStyle(fontWeight = FontWeight.ExtraBold, fontSize = 48.sp, lineHeight = 50.sp, letterSpacing = (-2).sp).lumen(),
    displaySmall = TextStyle(fontWeight = FontWeight.Bold, fontSize = 40.sp, lineHeight = 42.sp, letterSpacing = (-1.5).sp).lumen(),
    headlineLarge = TextStyle(fontWeight = FontWeight.ExtraBold, fontSize = 36.sp, lineHeight = 40.sp, letterSpacing = (-1.2).sp).lumen(),
    headlineMedium = TextStyle(fontWeight = FontWeight.Bold, fontSize = 30.sp, lineHeight = 34.sp, letterSpacing = (-.8).sp).lumen(),
    // Screen titles: 26sp bold, slightly tight.
    headlineSmall = TextStyle(
        fontWeight = FontWeight.Bold,
        fontSize = 26.sp,
        lineHeight = 32.sp,
        letterSpacing = (-0.26).sp
    ).lumen(),
    // Sub-screen titles and the connect tile status.
    titleLarge = TextStyle(fontWeight = FontWeight.Bold, fontSize = 20.sp, lineHeight = 26.sp).lumen(),
    titleMedium = TextStyle(fontWeight = FontWeight.SemiBold, fontSize = 16.sp, lineHeight = 22.sp).lumen(),
    titleSmall = Base.titleSmall.lumen(),
    // Row titles.
    bodyLarge = TextStyle(fontWeight = FontWeight.Medium, fontSize = 15.sp, lineHeight = 20.sp).lumen(),
    bodyMedium = TextStyle(fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 19.sp).lumen(),
    // Row subtitles and hints.
    bodySmall = TextStyle(fontWeight = FontWeight.Normal, fontSize = 12.5.sp, lineHeight = 17.sp).lumen(),
    labelLarge = TextStyle(fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 18.sp).lumen(),
    labelMedium = TextStyle(fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 16.sp).lumen(),
    // Section captions (rendered upper-case by callers).
    labelSmall = TextStyle(
        fontWeight = FontWeight.SemiBold,
        fontSize = 12.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.72.sp
    ).lumen()
)

// Geist supplies Latin; Persian needs a native joining font and untracked glyphs.
private fun TextStyle.persian() = copy(fontFamily = FontFamily.SansSerif, letterSpacing = 0.sp)
val PersianTypography = Typography.copy(
    displayLarge = Typography.displayLarge.persian(), displayMedium = Typography.displayMedium.persian(),
    displaySmall = Typography.displaySmall.persian(), headlineLarge = Typography.headlineLarge.persian(),
    headlineMedium = Typography.headlineMedium.persian(), headlineSmall = Typography.headlineSmall.persian(),
    titleLarge = Typography.titleLarge.persian(), titleMedium = Typography.titleMedium.persian(), titleSmall = Typography.titleSmall.persian(),
    bodyLarge = Typography.bodyLarge.persian(), bodyMedium = Typography.bodyMedium.persian(), bodySmall = Typography.bodySmall.persian(),
    labelLarge = Typography.labelLarge.persian(), labelMedium = Typography.labelMedium.persian(), labelSmall = Typography.labelSmall.persian()
)
