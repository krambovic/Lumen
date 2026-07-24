package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

private data class ThemeOptionUi(
    val preset: ThemePreset,
    val title: String,
    val subtitle: String,
    val iconBg: Color,
    val iconTint: Color,
    val dots: List<Color>
)

@Composable
fun ThemeSettingsScreen(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current

    val themes = listOf(
        ThemeOptionUi(
            preset = ThemePreset.LIGHT,
            title = s.lightTheme,
            subtitle = s.lightThemeDesc,
            iconBg = Color(0xFFE2E8F0),
            iconTint = Color(0xFF007AA3),
            dots = listOf(Color(0xFFFFFFFF), Color(0xFFE2E8F0), Color(0xFF007AA3))
        ),
        ThemeOptionUi(
            preset = ThemePreset.DARK,
            title = s.darkTheme,
            subtitle = s.darkThemeDesc,
            iconBg = Color(0xFF1E293B),
            iconTint = Color(0xFF38BDF8),
            dots = listOf(Color(0xFF0D0F17), Color(0xFF161926), Color(0xFF007AA3))
        ),
        ThemeOptionUi(
            preset = ThemePreset.DRACULA,
            title = "Dracula",
            subtitle = "Dracula dark purple palette",
            iconBg = Color(0xFF343746),
            iconTint = Color(0xFFBD93F9),
            dots = listOf(Color(0xFF282A36), Color(0xFF343746), Color(0xFFBD93F9))
        ),
        ThemeOptionUi(
            preset = ThemePreset.CATPPUCCIN,
            title = "Catppuccin",
            subtitle = "Catppuccin Mocha pastel accent",
            iconBg = Color(0xFF1E1E2E),
            iconTint = Color(0xFFCBA6F7),
            dots = listOf(Color(0xFF11111B), Color(0xFF1E1E2E), Color(0xFFCBA6F7))
        ),
        ThemeOptionUi(
            preset = ThemePreset.NORD,
            title = "Nord",
            subtitle = "Arctic blue nord palette",
            iconBg = Color(0xFF3B4252),
            iconTint = Color(0xFF88C0D0),
            dots = listOf(Color(0xFF2E3440), Color(0xFF3B4252), Color(0xFF88C0D0))
        ),
        ThemeOptionUi(
            preset = ThemePreset.GITHUB,
            title = "GitHub Dark",
            subtitle = "GitHub official dark theme",
            iconBg = Color(0xFF161B22),
            iconTint = Color(0xFF58A6FF),
            dots = listOf(Color(0xFF0D1117), Color(0xFF161B22), Color(0xFF58A6FF))
        ),
        ThemeOptionUi(
            preset = ThemePreset.GRUVBOX,
            title = "Gruvbox",
            subtitle = "Retro warm dark gruvbox",
            iconBg = Color(0xFF32302F),
            iconTint = Color(0xFFFABD2F),
            dots = listOf(Color(0xFF1D2021), Color(0xFF32302F), Color(0xFFFABD2F))
        ),
        ThemeOptionUi(
            preset = ThemePreset.TOKYO_NIGHT,
            title = "Tokyo Night",
            subtitle = "Tokyo neon night theme",
            iconBg = Color(0xFF24283B),
            iconTint = Color(0xFF7AA2F7),
            dots = listOf(Color(0xFF16161E), Color(0xFF24283B), Color(0xFF7AA2F7))
        ),
        ThemeOptionUi(
            preset = ThemePreset.MONOKAI,
            title = "Monokai",
            subtitle = "Classic monokai dark",
            iconBg = Color(0xFF272822),
            iconTint = Color(0xFFF92672),
            dots = listOf(Color(0xFF1E1F1C), Color(0xFF272822), Color(0xFFF92672))
        ),
        ThemeOptionUi(
            preset = ThemePreset.MATERIAL,
            title = "Material Dark",
            subtitle = "Sleek material palette",
            iconBg = Color(0xFF202331),
            iconTint = Color(0xFF80CBC4),
            dots = listOf(Color(0xFF0F111A), Color(0xFF202331), Color(0xFF80CBC4))
        ),
        ThemeOptionUi(
            preset = ThemePreset.SOLARIZED,
            title = "Solarized",
            subtitle = "Solarized dark cyan",
            iconBg = Color(0xFF0A3A46),
            iconTint = Color(0xFF268BD2),
            dots = listOf(Color(0xFF002B36), Color(0xFF0A3A46), Color(0xFF268BD2))
        )
    )

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp)
    ) {
        LumenScreenHeader(
            title = s.themeSettings,
            onBack = onBack,
            actions = {
                TextButton(onClick = onBack) {
                    Text(
                        text = s.done,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary
                    )
                }
            }
        )

        Spacer(Modifier.height(8.dp))

        // Render All Theme Preset Cards
        themes.forEach { item ->
            ThemePresetCard(
                title = item.title,
                subtitle = item.subtitle,
                iconBg = item.iconBg,
                iconTint = item.iconTint,
                dots = item.dots,
                isSelected = state.themePreset == item.preset,
                onClick = {
                    onUpdate(state.copy(themePreset = item.preset, themeMode = if (item.preset == ThemePreset.LIGHT) ThemeMode.LIGHT else ThemeMode.DARK))
                }
            )
            Spacer(Modifier.height(12.dp))
        }

        Spacer(Modifier.height(8.dp))

        // Material You & AMOLED Black in a separate bottom panel
        SettingsCard {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Filled.Star,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary
                    )
                }
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        text = s.materialYou,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold
                    )
                    Text(
                        text = s.materialYouDesc,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                Switch(
                    checked = state.useMaterialYou,
                    onCheckedChange = { onUpdate(state.copy(useMaterialYou = it)) }
                )
            }

            SettingsDivider()

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Filled.Info,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary
                    )
                }
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        text = s.amoledBlack,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold
                    )
                    Text(
                        text = s.amoledBlackDesc,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                Switch(
                    checked = state.useAmoledBlack,
                    onCheckedChange = { onUpdate(state.copy(useAmoledBlack = it)) }
                )
            }
        }

        Spacer(Modifier.height(84.dp))
    }
}

@Composable
private fun ThemePresetCard(
    title: String,
    subtitle: String,
    iconBg: Color,
    iconTint: Color,
    dots: List<Color>,
    isSelected: Boolean,
    onClick: () -> Unit
) {
    val shape = RoundedCornerShape(20.dp)
    val cardBorder = if (isSelected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(if (isSelected) 2.dp else 1.dp, cardBorder, shape)
            .clickable(onClick = onClick)
            .padding(16.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .clip(CircleShape)
                    .background(iconBg),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Filled.Star,
                    contentDescription = null,
                    tint = iconTint,
                    modifier = Modifier.size(24.dp)
                )
            }

            Spacer(Modifier.width(14.dp))

            Column(Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            Spacer(Modifier.width(8.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                dots.forEach { color ->
                    Box(
                        modifier = Modifier
                            .size(16.dp)
                            .clip(CircleShape)
                            .background(color)
                            .border(1.dp, Color.Gray.copy(alpha = 0.3f), CircleShape)
                    )
                }
                if (isSelected) {
                    Box(
                        modifier = Modifier
                            .size(24.dp)
                            .clip(CircleShape)
                            .background(MaterialTheme.colorScheme.primary),
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Check,
                            contentDescription = null,
                            tint = Color.White,
                            modifier = Modifier.size(16.dp)
                        )
                    }
                }
            }
        }
    }
}
