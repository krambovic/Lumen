package com.lumen.ui.screens

import androidx.compose.material.icons.filled.PowerSettingsNew

import androidx.compose.ui.unit.sp

import android.os.Build
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.Wallpaper
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@Composable
private fun ThemeSettingsSwitchRow(
    title: String, subtitle: String, icon: androidx.compose.ui.graphics.vector.ImageVector,
    checked: Boolean, enabled: Boolean, onCheckedChange: (Boolean) -> Unit
) { PreferenceSwitchRow(title, subtitle, icon, checked, enabled, onCheckedChange) }

private data class ThemeOptionUi(
    val preset: ThemePreset,
    val title: String,
    val subtitle: String,
    val iconBg: Color,
    val iconTint: Color,
    val dots: List<Color>
)

private fun themeOptions(s: LumenStrings): List<ThemeOptionUi> = listOf(
        ThemeOptionUi(
            preset = ThemePreset.LIGHT,
            title = s.lightTheme,
            subtitle = s.lightThemeDesc,
            iconBg = Color(0xFFE2E8F0),
            iconTint = Color(0xFF007AA3),
            dots = listOf(com.lumen.ui.theme.LumenLightBg, com.lumen.ui.theme.LumenLightTonal, com.lumen.ui.theme.LumenLightAccent)
        ),
        ThemeOptionUi(
            preset = ThemePreset.DARK,
            title = s.darkTheme,
            subtitle = s.darkThemeDesc,
            iconBg = Color(0xFF1E293B),
            iconTint = Color(0xFF38BDF8),
            dots = listOf(com.lumen.ui.theme.LumenDarkBg, com.lumen.ui.theme.LumenDarkTonal, com.lumen.ui.theme.LumenDarkAccent)
        ),
        ThemeOptionUi(
            preset = ThemePreset.DRACULA,
            title = "Dracula",
            subtitle = "Dracula Dark Purple Palette",
            iconBg = Color(0xFF343746),
            iconTint = Color(0xFFBD93F9),
            dots = listOf(Color(0xFF282A36), Color(0xFF343746), Color(0xFFBD93F9))
        ),
        ThemeOptionUi(
            preset = ThemePreset.CATPPUCCIN,
            title = "Catppuccin",
            subtitle = "Catppuccin Mocha Pastel Accent",
            iconBg = Color(0xFF1E1E2E),
            iconTint = Color(0xFFCBA6F7),
            dots = listOf(Color(0xFF11111B), Color(0xFF1E1E2E), Color(0xFFCBA6F7))
        ),
        ThemeOptionUi(
            preset = ThemePreset.NORD,
            title = "Nord",
            subtitle = "Arctic Blue Nord Palette",
            iconBg = Color(0xFF3B4252),
            iconTint = Color(0xFF88C0D0),
            dots = listOf(Color(0xFF2E3440), Color(0xFF3B4252), Color(0xFF88C0D0))
        ),
        ThemeOptionUi(
            preset = ThemePreset.GITHUB,
            title = "GitHub Dark",
            subtitle = "GitHub Official Dark Theme",
            iconBg = Color(0xFF161B22),
            iconTint = Color(0xFF58A6FF),
            dots = listOf(Color(0xFF0D1117), Color(0xFF161B22), Color(0xFF58A6FF))
        ),
        ThemeOptionUi(
            preset = ThemePreset.GRUVBOX,
            title = "Gruvbox",
            subtitle = "Retro Warm Dark Gruvbox",
            iconBg = Color(0xFF32302F),
            iconTint = Color(0xFFFABD2F),
            dots = listOf(Color(0xFF1D2021), Color(0xFF32302F), Color(0xFFFABD2F))
        ),
        ThemeOptionUi(
            preset = ThemePreset.TOKYO_NIGHT,
            title = "Tokyo Night",
            subtitle = "Tokyo Neon Night Theme",
            iconBg = Color(0xFF24283B),
            iconTint = Color(0xFF7AA2F7),
            dots = listOf(Color(0xFF16161E), Color(0xFF24283B), Color(0xFF7AA2F7))
        ),
        ThemeOptionUi(
            preset = ThemePreset.MONOKAI,
            title = "Monokai",
            subtitle = "Classic Monokai Dark",
            iconBg = Color(0xFF272822),
            iconTint = Color(0xFFF92672),
            dots = listOf(Color(0xFF1E1F1C), Color(0xFF272822), Color(0xFFF92672))
        ),
        ThemeOptionUi(
            preset = ThemePreset.MATERIAL,
            title = "Material Dark",
            subtitle = "Sleek Material Palette",
            iconBg = Color(0xFF202331),
            iconTint = Color(0xFF80CBC4),
            dots = listOf(Color(0xFF0F111A), Color(0xFF202331), Color(0xFF80CBC4))
        ),
        ThemeOptionUi(
            preset = ThemePreset.SOLARIZED,
            title = "Solarized",
            subtitle = "Solarized Dark Cyan",
            iconBg = Color(0xFF0A3A46),
            iconTint = Color(0xFF268BD2),
            dots = listOf(Color(0xFF002B36), Color(0xFF0A3A46), Color(0xFF268BD2))
        ),
        ThemeOptionUi(
            preset = ThemePreset.ROSE_PINE,
            title = s.rosePineTheme,
            subtitle = s.rosePineThemeDesc,
            iconBg = Color(0xFF1F1D2E),
            iconTint = Color(0xFFC4A7E7),
            dots = listOf(Color(0xFF191724), Color(0xFF26233A), Color(0xFFC4A7E7))
        )
    )

internal fun themeDisplayName(preset: ThemePreset, strings: LumenStrings): String =
    themeOptions(strings).first { it.preset == preset }.title

@Composable
internal fun ThemePalettePicker(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    val themes = themeOptions(s)
    Column {
        SectionHeader(s.theme)
        androidx.compose.foundation.lazy.LazyRow(
            state = androidx.compose.foundation.lazy.rememberLazyListState(
                initialFirstVisibleItemIndex = themes.indexOfFirst { it.preset == state.themePreset }.coerceAtLeast(0)),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(themes, key = { it.preset.name }) { item ->
                ThemePresetCard(item.title, item.subtitle, item.iconBg, item.iconTint, item.dots,
                    state.themePreset == item.preset, Modifier.width(166.dp).heightIn(min = 184.dp)) {
                    onUpdate(state.copy(themePreset = item.preset, accentColor = "",
                        themeMode = if (item.preset == ThemePreset.LIGHT) ThemeMode.LIGHT else ThemeMode.DARK))
                }
            }
        }
    }
}

@Composable
internal fun DashboardLayoutPicker(state: SettingsUiState, onUpdate: (SettingsUiState) -> Unit) {
    val s = LocalStrings.current
    Column {
        Text(
            text = s.dashboardStyle,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold
        )
        Spacer(Modifier.height(10.dp))
        DashboardStyle.entries.chunked(2).forEach { row ->
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.Bottom
        ) {
            row.forEach { style ->
                DashboardStyleCard(title = when (style) {
                    DashboardStyle.DEFAULT -> s.dashboardStyleDefault
                    DashboardStyle.SLIDER -> s.dashboardStyleSlider
                    DashboardStyle.CENTERED -> s.dashboardStyleCentered
                    DashboardStyle.DOCK -> s.dashboardStyleDock
                }, style = style, isSelected = state.dashboardStyle == style, modifier = Modifier.weight(1f)) {
                    onUpdate(state.copy(dashboardStyle = style))
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        }

    }
}

@Composable
fun ThemeSettingsScreen(
    state: SettingsUiState,
    onUpdate: (SettingsUiState) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current


    Column(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 100.dp)
    ) {
        SettingsDetailHeader(
            title = s.appearance,
            description = s.settingsAppearanceSummary,
            onBack = onBack
        )

        Spacer(Modifier.height(8.dp))

        val dynamicColorSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        SectionHeader(s.appearance)
        SettingsCard {
            ThemeSettingsSwitchRow(s.materialYou,
                if (dynamicColorSupported) s.materialYouDesc else s.materialYouUnavailable,
                Icons.Filled.Wallpaper, state.useMaterialYou && dynamicColorSupported,
                dynamicColorSupported) { onUpdate(state.copy(useMaterialYou = it)) }
            SettingsDivider()
            ThemeSettingsSwitchRow(s.amoledBlack, s.amoledBlackDesc, Icons.Filled.DarkMode,
                state.useAmoledBlack, true) { onUpdate(state.copy(useAmoledBlack = it)) }
        }
        ThemePalettePicker(state, onUpdate)


        Spacer(Modifier.height(20.dp))
        AccentColorSetting(state.accentColor, state.language) { onUpdate(state.copy(accentColor = it)) }
        Spacer(Modifier.height(20.dp))

        DashboardLayoutPicker(state, onUpdate)
        Spacer(Modifier.height(20.dp))
        ServerLayoutPicker(state, onUpdate)

        Spacer(Modifier.height(20.dp))

        Text(
            text = s.launcherIcon,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold
        )
        Spacer(Modifier.height(4.dp))
        // The cost is real and launcher-specific, so it is stated up front rather
        // than discovered after the icon vanishes off the home screen.
        Text(
            text = s.launcherIconDesc,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(Modifier.height(10.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            LauncherIconCard(
                title = s.launcherIconSystem,
                option = LauncherIconOption.SYSTEM,
                // Labelled as the default so the auto-theming behaviour the app has
                // always shipped stays recognisable and reachable again.
                defaultBadge = s.dashboardStyleDefault,
                isSelected = state.launcherIcon == LauncherIconOption.SYSTEM,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(launcherIcon = LauncherIconOption.SYSTEM)) }
            LauncherIconCard(
                title = s.launcherIconLight,
                option = LauncherIconOption.LIGHT,
                isSelected = state.launcherIcon == LauncherIconOption.LIGHT,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(launcherIcon = LauncherIconOption.LIGHT)) }
            LauncherIconCard(
                title = s.launcherIconDark,
                option = LauncherIconOption.DARK,
                isSelected = state.launcherIcon == LauncherIconOption.DARK,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(launcherIcon = LauncherIconOption.DARK)) }
        }

        Spacer(Modifier.height(16.dp))
    }
}

// ---------------------------------------------------------------------------
// Launcher icon preview
//
// The mark below is the SAME six-point polygon that
// android/tools/gen_launcher_icons.py traces, normalised to its own bounding
// box, and the two palettes are the ones that generator emits. Drawing it here
// rather than loading @mipmap/ic_launcher keeps the preview honest even for the
// variant the system is not currently showing: on a device in dark mode the
// launcher would hand back the night artwork for every one of the three tiles.
// ---------------------------------------------------------------------------

/** GLYPH from the generator, mapped into a 0..1 box: (x - 345) / 671.5, (y - 211.5) / 812. */
private val LUMEN_MARK: List<Pair<Float, Float>> = listOf(
    0.21668f to 0.00000f,
    0.21668f to 0.81219f,
    1.00000f to 0.81219f,
    0.81683f to 1.00000f,
    0.00000f to 1.00000f,
    0.00000f to 0.18411f
)

/** 671.5 / 812 — width over height of the traced mark. */
private const val MARK_ASPECT = 0.82697f

/** GLYPH_HEIGHT_DP / MASK_DP: the mark fills 50dp of the 72dp a launcher shows. */
private const val MARK_HEIGHT_FRACTION = 50f / 72f

private val ICON_LIGHT_PLATE = listOf(Color(0xFFFFFFFF), Color(0xFFFFFFFF))
private val ICON_LIGHT_MARK = Color(0xFF000000)
private val ICON_DARK_PLATE = listOf(Color(0xFF212121), Color(0xFF0B0B0B))
private val ICON_DARK_FACET = listOf(Color(0xFF2A2A2A), Color(0xFF111111))
private val ICON_DARK_MARK = Color(0xFFFFFFFF)

private fun DrawScope.drawLumenIcon(
    plate: List<Color>,
    markColor: Color,
    diagonalFacet: List<Color>? = null
) {
    drawRect(Brush.verticalGradient(plate, startY = 0f, endY = size.height))
    if (diagonalFacet != null) {
        val facet = Path().apply {
            moveTo(size.width * 870f / 1024f, 0f)
            lineTo(size.width, 0f)
            lineTo(size.width, size.height)
            lineTo(size.width * 348f / 1024f, size.height)
            close()
        }
        drawPath(
            path = facet,
            brush = Brush.linearGradient(
                colors = diagonalFacet,
                start = Offset(size.width * 870f / 1024f, 0f),
                end = Offset(size.width * 348f / 1024f, size.height)
            )
        )
    }
    val markHeight = size.height * MARK_HEIGHT_FRACTION
    val markWidth = markHeight * MARK_ASPECT
    val left = (size.width - markWidth) / 2f
    val top = (size.height - markHeight) / 2f
    val path = Path()
    LUMEN_MARK.forEachIndexed { index, point ->
        val x = left + point.first * markWidth
        val y = top + point.second * markHeight
        if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.close()
    drawPath(path, markColor)
}

@Composable
private fun LauncherIconPreview(option: LauncherIconOption, modifier: Modifier = Modifier) {
    Canvas(modifier) {
        when (option) {
            // Both halves at once: "follow system" is neither one nor the other.
            LauncherIconOption.SYSTEM -> {
                clipRect(right = size.width / 2f) {
                    drawLumenIcon(ICON_LIGHT_PLATE, ICON_LIGHT_MARK)
                }
                clipRect(left = size.width / 2f) {
                    drawLumenIcon(ICON_DARK_PLATE, ICON_DARK_MARK, ICON_DARK_FACET)
                }
            }
            LauncherIconOption.LIGHT -> drawLumenIcon(ICON_LIGHT_PLATE, ICON_LIGHT_MARK)
            LauncherIconOption.DARK ->
                drawLumenIcon(ICON_DARK_PLATE, ICON_DARK_MARK, ICON_DARK_FACET)
        }
    }
}

// Same tile shape as DashboardStyleCard, with the actual artwork as the preview.
@Composable
private fun selectionCardShape(selected: Boolean): RoundedCornerShape {
    val corner = androidx.compose.animation.core.animateDpAsState(
        targetValue = if (selected) 26.dp else 18.dp,
        animationSpec = androidx.compose.animation.core.spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMedium
        ),
        label = "selection_card_shape"
    )
    return RoundedCornerShape(corner.value)
}

@Composable
private fun LauncherIconCard(
    title: String,
    option: LauncherIconOption,
    isSelected: Boolean,
    modifier: Modifier = Modifier,
    defaultBadge: String? = null,
    onClick: () -> Unit
) {
    val shape = selectionCardShape(isSelected)
    val accent = MaterialTheme.colorScheme.primary
    val border = if (isSelected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    Column(
        modifier = modifier
            // Fixed height so the "Default" badge on one tile cannot make that
            // tile taller than the other two.
            .height(150.dp)
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(if (isSelected) 2.dp else 1.dp, border, shape)
            .selectable(selected = isSelected, role = Role.RadioButton, onClick = onClick)
            .padding(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            contentAlignment = Alignment.Center
        ) {
            LauncherIconPreview(
                option = option,
                modifier = Modifier
                    .size(56.dp)
                    // Rounded square rather than a circle: it reads as an app icon
                    // without pretending to be one launcher's mask in particular.
                    .clip(RoundedCornerShape(28))
                    .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(28))
            )
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f)
            )
            if (isSelected) {
                Spacer(Modifier.width(4.dp))
                Box(
                    modifier = Modifier.size(16.dp).clip(CircleShape).background(accent),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Filled.Check,
                        contentDescription = null,
                        tint = Color.White,
                        modifier = Modifier.size(11.dp)
                    )
                }
            }
        }
        if (defaultBadge != null) {
            Text(
                text = defaultBadge,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.fillMaxWidth()
            )
        }
    }
}

// Miniature dashboard mock-up so the layout choice is understandable at a glance.
@Composable
private fun DashboardStyleCard(
    title: String, style: DashboardStyle, isSelected: Boolean,
    modifier: Modifier = Modifier, onClick: () -> Unit
) {
    val art = com.lumen.ui.theme.expressivePalette()
    val accent = MaterialTheme.colorScheme.primary
    val shape = selectionCardShape(isSelected)
    Column(modifier.height(if (isSelected) 178.dp else 164.dp).clip(shape)
        .background(if (isSelected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow)
        .border(if (isSelected) 2.dp else 1.dp,
            if (isSelected) accent else MaterialTheme.colorScheme.outlineVariant, shape)
        .selectable(selected = isSelected, role = Role.RadioButton, onClick = onClick).padding(10.dp)) {
        Box(Modifier.fillMaxWidth().height(88.dp).clip(RoundedCornerShape(14.dp)).background(art.stage).padding(8.dp)) {
            when (style) {
                DashboardStyle.DEFAULT -> Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.SpaceBetween) {
                    Box(Modifier.width(22.dp).height(4.dp).clip(CircleShape).background(art.onStage.copy(alpha = .4f)))
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween) {
                        Box(Modifier.width(26.dp).height(9.dp).clip(CircleShape).background(art.onStage))
                        com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Clover, art.lime, Modifier.size(21.dp))
                    }
                    Box(Modifier.fillMaxWidth().height(12.dp).clip(CircleShape).background(art.lime))
                }
                DashboardStyle.SLIDER -> Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.SpaceBetween) {
                    Box(Modifier.width(22.dp).height(4.dp).clip(CircleShape).background(art.onStage.copy(alpha = .4f)))
                    Box(Modifier.fillMaxWidth(.7f).height(12.dp).clip(CircleShape).background(art.onStage))
                    Row(Modifier.fillMaxWidth().height(22.dp).clip(CircleShape).background(art.lime).padding(horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically) {
                        com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Clover, art.onLime, Modifier.size(16.dp))
                        androidx.compose.foundation.Canvas(Modifier.weight(1f).height(20.dp).padding(start = 4.dp)) {
                            val wave = androidx.compose.ui.graphics.Path()
                            var x = 0f
                            while (x <= size.width) {
                                val y = size.height / 2 + kotlin.math.sin(x / 6f) * 3f
                                if (x == 0f) wave.moveTo(x, y) else wave.lineTo(x, y)
                                x += 1f
                            }
                            drawPath(wave, art.onLime, style = androidx.compose.ui.graphics.drawscope.Stroke(width = 2f))
                        }
                    }
                }
                DashboardStyle.CENTERED -> Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.SpaceBetween) {
                    Box(Modifier.width(22.dp).height(4.dp).clip(CircleShape).background(art.onStage.copy(alpha = .4f)))
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Flower, art.lime, Modifier.size(36.dp))
                        Box(Modifier.weight(1f).height(9.dp).clip(CircleShape).background(art.onStage))
                    }
                    Box(Modifier.fillMaxWidth().height(12.dp).clip(RoundedCornerShape(4.dp)).background(art.onStage.copy(alpha = .22f)))
                }
                DashboardStyle.DOCK -> Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.SpaceBetween) {
                    Box(Modifier.fillMaxWidth(.45f).height(5.dp).clip(CircleShape).background(art.onStage.copy(alpha = .4f)))
                    Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        Box(Modifier.weight(1f).height(23.dp).clip(RoundedCornerShape(5.dp)).background(art.onStage.copy(alpha = .22f)))
                        Box(Modifier.weight(1f).height(23.dp).clip(RoundedCornerShape(5.dp)).background(art.onStage.copy(alpha = .22f)))
                    }
                    Row(Modifier.fillMaxWidth().height(23.dp).clip(CircleShape).background(art.onStage.copy(alpha = .18f)).padding(3.dp),
                        horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                        repeat(4) { Box(Modifier.size(5.dp).clip(CircleShape).background(art.onStage.copy(alpha = .6f))) }
                        Spacer(Modifier.weight(1f))
                        Box(Modifier.size(16.dp).clip(com.lumen.ui.components.expressiveShape(com.lumen.ui.components.Motif.Sun))
                            .background(art.lime), contentAlignment = Alignment.Center) {
                            Icon(Icons.Filled.PowerSettingsNew, null, Modifier.size(9.dp), tint = art.onLime)
                        }
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        Text(title, style = MaterialTheme.typography.bodySmall, fontWeight = FontWeight.Bold,
            maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
            if (isSelected) Icon(Icons.Filled.Check, null, tint = accent, modifier = Modifier.size(18.dp))
        }
    }
}

// Compact square tile with an accent glow when on; replaces the old switch rows.
@Composable
private fun ThemeToggleTile(
    title: String,
    subtitle: String,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    checked: Boolean,
    modifier: Modifier = Modifier,
    // Forces the icon disc fill; used by AMOLED so the tile shows real #000000.
    swatch: Color? = null,
    // Optional three-colour preview, matching the preset cards.
    dots: List<Color> = emptyList(),
    enabled: Boolean = true,
    onChange: (Boolean) -> Unit
) {
    val shape = RoundedCornerShape(18.dp)
    val accent = MaterialTheme.colorScheme.primary
    val border = if (checked) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    val discFill = swatch ?: if (checked) accent else MaterialTheme.colorScheme.surface
    val discTint = when {
        swatch != null -> accent
        checked -> MaterialTheme.colorScheme.onPrimary
        else -> accent
    }
    Column(
        modifier = modifier
            .height(158.dp)
            .alpha(if (enabled) 1f else 0.5f)
            .clip(shape)
            // Flat fill (no gradient): selection is conveyed by the border and check mark,
            // which keeps AMOLED and Material You surfaces perfectly uniform.
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(if (checked) 2.dp else 1.dp, border, shape)
            .clickable(enabled = enabled) { onChange(!checked) }
            .padding(14.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .clip(CircleShape)
                    .background(discFill)
                    .border(1.dp, MaterialTheme.colorScheme.outline, CircleShape),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = discTint,
                    modifier = Modifier.size(20.dp)
                )
            }
            Spacer(Modifier.weight(1f))
            Box(
                modifier = Modifier
                    .size(22.dp)
                    .clip(CircleShape)
                    .background(if (checked) accent else Color.Transparent)
                    .border(1.dp, if (checked) accent else MaterialTheme.colorScheme.outline, CircleShape),
                contentAlignment = Alignment.Center
            ) {
                if (checked) {
                    Icon(
                        imageVector = Icons.Filled.Check,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onPrimary,
                        modifier = Modifier.size(14.dp)
                    )
                }
            }
        }
        if (dots.isNotEmpty()) {
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                dots.forEach { color ->
                    Box(
                        modifier = Modifier
                            .size(12.dp)
                            .clip(CircleShape)
                            .background(color)
                            .border(1.dp, MaterialTheme.colorScheme.outline, CircleShape)
                    )
                }
            }
        }
        Spacer(Modifier.weight(1f))
        Text(
            text = title,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
private fun ThemePresetCard(
    title: String, subtitle: String, iconBg: Color, iconTint: Color, dots: List<Color>,
    isSelected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit
) {
    val shape = selectionCardShape(isSelected)
    val accent = MaterialTheme.colorScheme.primary
    Column(modifier.clip(shape)
        .background(if (isSelected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface)
        .selectable(selected = isSelected, role = Role.RadioButton, onClick = onClick).padding(10.dp)) {
        Box(Modifier.fillMaxWidth().height(100.dp).clip(RoundedCornerShape(20.dp)).background(dots.first())) {
            Text("Aa", fontFamily = com.lumen.ui.theme.Inter, fontSize = 42.sp, fontWeight = FontWeight.ExtraBold,
                letterSpacing = (-2).sp, color = dots.last(), modifier = Modifier.padding(start = 12.dp, top = 8.dp))
            com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Flower, dots.last(),
                Modifier.align(Alignment.CenterEnd).offset(x = 12.dp).size(66.dp))
            Row(Modifier.align(Alignment.BottomStart).padding(10.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Box(Modifier.width(35.dp).height(12.dp).clip(RoundedCornerShape(50)).background(dots.last()))
                Box(Modifier.width(24.dp).height(12.dp).clip(RoundedCornerShape(50)).background(dots.getOrElse(1) { iconBg }))
            }
        }
        Spacer(Modifier.height(10.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f),
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (isSelected) Icon(Icons.Filled.Check, LocalStrings.current.selected,
                tint = accent, modifier = Modifier.size(20.dp))
        }
        Text(subtitle, style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}
