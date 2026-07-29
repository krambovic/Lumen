package com.lumen.ui.screens

import android.os.Build
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.Wallpaper
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
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

        // Two-per-row grid of compact preview tiles instead of full-width list rows.
        themes.chunked(2).forEach { pair ->
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                pair.forEach { item ->
                    ThemePresetCard(
                        title = item.title,
                        subtitle = item.subtitle,
                        iconBg = item.iconBg,
                        iconTint = item.iconTint,
                        dots = item.dots,
                        isSelected = state.themePreset == item.preset,
                        modifier = Modifier.weight(1f),
                        onClick = {
                            onUpdate(state.copy(themePreset = item.preset, themeMode = if (item.preset == ThemePreset.LIGHT) ThemeMode.LIGHT else ThemeMode.DARK))
                        }
                    )
                }
                if (pair.size == 1) Spacer(Modifier.weight(1f))
            }
        }

        Spacer(Modifier.height(8.dp))

        // Two square toggle tiles instead of switch rows, matching the preset grid.
        // Dynamic colour only exists on Android 12+; below that the tile says so
        // instead of offering a switch that would silently do nothing.
        val dynamicColorSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            ThemeToggleTile(
                title = s.materialYou,
                subtitle = if (dynamicColorSupported) s.materialYouDesc else s.materialYouUnavailable,
                // Wallpaper, because that is literally where the colours come from.
                icon = Icons.Filled.Wallpaper,
                checked = state.useMaterialYou && dynamicColorSupported,
                enabled = dynamicColorSupported,
                // Live preview of the scheme currently in effect: on a Material You
                // device these are the wallpaper-derived roles.
                dots = listOf(
                    MaterialTheme.colorScheme.primary,
                    MaterialTheme.colorScheme.secondary,
                    MaterialTheme.colorScheme.tertiary
                ),
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(useMaterialYou = it)) }
            ThemeToggleTile(
                title = s.amoledBlack,
                subtitle = s.amoledBlackDesc,
                icon = Icons.Filled.DarkMode,
                // Real #000000, not a dark grey: the swatch has to be the thing it promises.
                swatch = Color(0xFF000000),
                checked = state.useAmoledBlack,
                dots = listOf(Color(0xFF000000), Color(0xFF000000), MaterialTheme.colorScheme.primary),
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(useAmoledBlack = it)) }
        }

        Spacer(Modifier.height(20.dp))

        Text(
            text = s.dashboardStyle,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold
        )
        Spacer(Modifier.height(10.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            DashboardStyleCard(
                title = s.dashboardStyleDefault,
                style = DashboardStyle.DEFAULT,
                isSelected = state.dashboardStyle == DashboardStyle.DEFAULT,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(dashboardStyle = DashboardStyle.DEFAULT)) }
            DashboardStyleCard(
                title = s.dashboardStyleSlider,
                style = DashboardStyle.SLIDER,
                isSelected = state.dashboardStyle == DashboardStyle.SLIDER,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(dashboardStyle = DashboardStyle.SLIDER)) }
            DashboardStyleCard(
                title = s.dashboardStyleCentered,
                style = DashboardStyle.CENTERED,
                isSelected = state.dashboardStyle == DashboardStyle.CENTERED,
                modifier = Modifier.weight(1f)
            ) { onUpdate(state.copy(dashboardStyle = DashboardStyle.CENTERED)) }
        }

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
private fun LauncherIconCard(
    title: String,
    option: LauncherIconOption,
    isSelected: Boolean,
    modifier: Modifier = Modifier,
    defaultBadge: String? = null,
    onClick: () -> Unit
) {
    val shape = RoundedCornerShape(18.dp)
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
            .clickable(onClick = onClick)
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
    title: String,
    style: DashboardStyle,
    isSelected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    val shape = RoundedCornerShape(18.dp)
    val accent = MaterialTheme.colorScheme.primary
    val border = if (isSelected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    val muted = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.25f)
    Column(
        modifier = modifier
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(if (isSelected) 2.dp else 1.dp, border, shape)
            .clickable(onClick = onClick)
            .padding(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(84.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.surface)
                .padding(8.dp)
        ) {
            Column(
                modifier = Modifier.fillMaxWidth().fillMaxHeight(),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                when (style) {
                    DashboardStyle.DEFAULT -> {
                        Box(
                            Modifier.fillMaxWidth().height(10.dp)
                                .clip(RoundedCornerShape(4.dp)).background(muted)
                        )
                        Spacer(Modifier.height(6.dp))
                        Box(Modifier.size(26.dp).clip(CircleShape).background(accent))
                        Spacer(Modifier.height(6.dp))
                        Box(
                            Modifier.fillMaxWidth().height(8.dp)
                                .clip(RoundedCornerShape(4.dp)).background(muted)
                        )
                    }
                    DashboardStyle.SLIDER -> {
                        Box(
                            Modifier.fillMaxWidth().height(10.dp)
                                .clip(RoundedCornerShape(4.dp)).background(muted)
                        )
                        Spacer(Modifier.height(4.dp))
                        Box(
                            Modifier.fillMaxWidth().height(18.dp)
                                .clip(RoundedCornerShape(6.dp)).background(muted)
                        )
                        Spacer(Modifier.weight(1f))
                        Box(
                            Modifier.fillMaxWidth().height(16.dp)
                                .clip(RoundedCornerShape(8.dp)).background(accent)
                        )
                    }
                    DashboardStyle.CENTERED -> {
                        Box(
                            Modifier.fillMaxWidth().height(8.dp)
                                .clip(RoundedCornerShape(4.dp)).background(muted)
                        )
                        Spacer(Modifier.weight(1f))
                        Box(Modifier.size(34.dp).clip(CircleShape).background(accent))
                        Spacer(Modifier.weight(1f))
                        Box(
                            Modifier.fillMaxWidth().height(8.dp)
                                .clip(RoundedCornerShape(4.dp)).background(muted)
                        )
                    }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Bold,
                maxLines = 2,
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
    title: String,
    subtitle: String,
    iconBg: Color,
    iconTint: Color,
    dots: List<Color>,
    isSelected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    val shape = RoundedCornerShape(18.dp)
    val accent = MaterialTheme.colorScheme.primary
    val cardBorder = if (isSelected) accent else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    Column(
        modifier = modifier
            // Fixed height keeps every preset tile the same size regardless of text length.
            .height(158.dp)
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .border(if (isSelected) 2.dp else 1.dp, cardBorder, shape)
            .clickable(onClick = onClick)
            .padding(12.dp)
    ) {
        // Palette strip doubles as the preview: dots row was too small to judge a theme.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(46.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(iconBg)
        ) {
            dots.forEach { color ->
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(color)
                )
            }
        }

        Spacer(Modifier.height(10.dp))

        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(10.dp)
                    .clip(CircleShape)
                    .background(iconTint)
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f)
            )
            if (isSelected) {
                Box(
                    modifier = Modifier
                        .size(20.dp)
                        .clip(CircleShape)
                        .background(accent),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Filled.Check,
                        contentDescription = null,
                        tint = Color.White,
                        modifier = Modifier.size(13.dp)
                    )
                }
            }
        }
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis
        )
    }
}
