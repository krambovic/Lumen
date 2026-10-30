package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.*
import com.lumen.ui.theme.expressivePalette

@Composable
internal fun WelcomeIntro(language: String) {
    val art = expressivePalette()
    val rotation = rememberDecorativeRotation(48000)
    Box(Modifier.fillMaxWidth().heightIn(min = 196.dp)
        .clip(RoundedCornerShape(20.dp, 48.dp, 20.dp, 48.dp)).background(art.blue).padding(24.dp)) {
        ExpressiveMotif(Motif.Sun, art.onBlue.copy(alpha = .12f),
            Modifier.size(144.dp).align(Alignment.TopEnd).graphicsLayer { rotationZ = rotation.value })
        Icon(Icons.Default.Public, null, Modifier.size(32.dp).align(Alignment.TopStart), tint = art.onBlue)
        Text(WelcomeText.WELCOME.inLanguage(language), Modifier.align(Alignment.BottomStart).padding(top = 64.dp),
            style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold, color = art.onBlue)
    }
}

@Composable
internal fun WelcomeSummaryGrid(settings: SettingsUiState, region: SetupRegion, routing: SetupRouting, count: Int) {
    val s = LocalStrings.current
    val art = expressivePalette()
    fun t(text: WelcomeText) = text.inLanguage(settings.language)
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SummaryCell(s.language, when (settings.language) { "ru" -> "Русский"; "zh" -> "简体中文"; "fa" -> "فارسی"; else -> "English" },
                Icons.Default.Language, art.blue, art.onBlue, Modifier.weight(1f))
            SummaryCell(s.theme, if (settings.themeMode == ThemeMode.SYSTEM) s.systemDefault else themeDisplayName(settings.themePreset, s), Icons.Default.Palette,
                art.lavender, art.onLavender, Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SummaryCell(t(WelcomeText.REGION), t(when (region) { SetupRegion.RUSSIA -> WelcomeText.RUSSIA; SetupRegion.CHINA -> WelcomeText.CHINA; SetupRegion.IRAN -> WelcomeText.IRAN }),
                Icons.Default.Public, art.coral, art.onCoral, Modifier.weight(1f))
            SummaryCell(s.routing, t(when (routing) { SetupRouting.GLOBAL -> WelcomeText.GLOBAL; SetupRouting.BLOCKED -> WelcomeText.BLOCKED; SetupRouting.EXCEPT_LOCAL -> WelcomeText.EXCEPT_LOCAL }),
                Icons.Default.Route, art.blue, art.onBlue, Modifier.weight(1f))
        }
        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(32.dp, 12.dp, 32.dp, 12.dp)).background(art.lime).padding(20.dp),
            horizontalArrangement = Arrangement.spacedBy(20.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(count.toString(), style = MaterialTheme.typography.displayMedium, color = art.onLime)
            Text(t(WelcomeText.SERVERS), style = MaterialTheme.typography.titleLarge, color = art.onLime)
        }
    }
}

@Composable
private fun SummaryCell(title: String, value: String, icon: ImageVector, color: Color, ink: Color, modifier: Modifier) {
    Column(modifier.heightIn(min = 142.dp).clip(RoundedCornerShape(24.dp, 12.dp, 24.dp, 12.dp))
        .background(color).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Icon(icon, null, Modifier.size(24.dp), tint = ink)
        Text(title, style = MaterialTheme.typography.labelMedium, color = ink.copy(alpha = .7f))
        Text(value, style = MaterialTheme.typography.titleMedium, color = ink)
    }
}
