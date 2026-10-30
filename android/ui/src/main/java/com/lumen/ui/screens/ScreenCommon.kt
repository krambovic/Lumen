package com.lumen.ui.screens

import androidx.compose.ui.graphics.luminance

import com.lumen.ui.components.expressiveButtonSemantics
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.foundation.BorderStroke
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.theme.ConnectionDanger
import com.lumen.ui.theme.ConnectionSuccess
import com.lumen.ui.theme.ConnectionWarning

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.ui.graphics.graphicsLayer
import com.lumen.ui.theme.LocalLumenColors

import androidx.compose.ui.draw.shadow

/** All editable fields share filled expressive surfaces, typography and focus feedback. */
@Composable
fun ExpressiveTextField(
    value: String, onValueChange: (String) -> Unit, modifier: Modifier = Modifier,
    label: (@Composable () -> Unit)? = null, placeholder: (@Composable () -> Unit)? = null,
    leadingIcon: (@Composable () -> Unit)? = null, trailingIcon: (@Composable () -> Unit)? = null,
    singleLine: Boolean = false, minLines: Int = 1, maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    enabled: Boolean = true, isError: Boolean = false, readOnly: Boolean = false,
    shape: androidx.compose.ui.graphics.Shape = RoundedCornerShape(14.dp),
    keyboardOptions: androidx.compose.foundation.text.KeyboardOptions = androidx.compose.foundation.text.KeyboardOptions.Default,
    keyboardActions: androidx.compose.foundation.text.KeyboardActions = androidx.compose.foundation.text.KeyboardActions.Default,
    visualTransformation: androidx.compose.ui.text.input.VisualTransformation = androidx.compose.ui.text.input.VisualTransformation.None,
    textStyle: androidx.compose.ui.text.TextStyle = MaterialTheme.typography.bodyLarge,
    colors: androidx.compose.material3.TextFieldColors = expressiveFieldColors()
) {
    androidx.compose.material3.TextField(value, onValueChange, modifier,
        enabled = enabled, readOnly = readOnly, textStyle = textStyle,
        label = label, placeholder = placeholder, leadingIcon = leadingIcon, trailingIcon = trailingIcon,
        singleLine = singleLine, minLines = minLines, maxLines = maxLines, isError = isError,
        keyboardOptions = keyboardOptions, keyboardActions = keyboardActions,
        visualTransformation = visualTransformation, shape = shape, colors = colors)
}

@Composable
fun expressiveFieldColors() = androidx.compose.material3.TextFieldDefaults.colors(
    focusedContainerColor = MaterialTheme.colorScheme.primaryContainer,
    unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
    disabledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
    errorContainerColor = MaterialTheme.colorScheme.errorContainer,
    focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent,
    disabledIndicatorColor = Color.Transparent, errorIndicatorColor = Color.Transparent
)

@Composable
internal fun PreferenceSwitchRow(
    title: String, description: String, icon: androidx.compose.ui.graphics.vector.ImageVector,
    checked: Boolean, enabled: Boolean = true, onChange: (Boolean) -> Unit
) {
    val selectedColor by androidx.compose.animation.animateColorAsState(
        if (checked) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        animationSpec = androidx.compose.animation.core.spring(), label = "preference_color")
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(selectedColor)
        .border(1.dp, if (checked) Color.Transparent else MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(18.dp))
        .then(Modifier.graphicsLayer { alpha = if (enabled) 1f else .45f })
        .then(Modifier.togglePreference(checked, enabled, onChange))
        .padding(horizontal = 12.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(40.dp).clip(com.lumen.ui.components.expressiveShape(
            if (checked) com.lumen.ui.components.Motif.Clover else com.lumen.ui.components.Motif.Pebble))
            .background(if (checked) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHighest),
            contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = if (checked) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(21.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f).padding(end = 8.dp)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
            if (description.isNotBlank()) Text(description, style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 3.dp))
        }
        LumenSwitch(checked, null, enabled = enabled)
    }
}

private fun Modifier.togglePreference(checked: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit): Modifier =
    this.then(Modifier.toggleable(value = checked, enabled = enabled,
        role = androidx.compose.ui.semantics.Role.Switch, onValueChange = onChange))

/** Material Design 3 Menu Selector with rounded corners & shadow elevation. */
@Composable
fun LumenDropdown(
    label: String,
    options: List<String>,
    selected: String,
    onSelected: (String) -> Unit,
    modifier: Modifier = Modifier,
    optionLabel: (String) -> String = { it }
) {
    var expanded by remember { mutableStateOf(false) }

    Column(modifier) {
        if (label.isNotEmpty()) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(4.dp))
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .background(MaterialTheme.colorScheme.primary.copy(alpha = .09f))
                .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = .18f), RoundedCornerShape(14.dp))
                .clickable { expanded = true }
                .padding(horizontal = 14.dp, vertical = 14.dp)
        ) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(optionLabel(selected), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                val arrowRotation by animateFloatAsState(if (expanded) 180f else 0f, label = "arrow")
                Icon(
                    imageVector = Icons.Filled.KeyboardArrowDown,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(20.dp).graphicsLayer { rotationZ = arrowRotation }
                )
            }
            LumenMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false },
                modifier = Modifier.widthIn(min = 220.dp)
            ) {
                    options.forEach { option ->
                        val isOptionSelected = option == selected
                        DropdownMenuItem(
                            text = {
                                Text(
                                    text = optionLabel(option),
                                    color = if (isOptionSelected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                                    fontWeight = if (isOptionSelected) FontWeight.Bold else FontWeight.Medium,
                                    style = MaterialTheme.typography.bodyLarge
                                )
                            },
                            trailingIcon = if (isOptionSelected) {
                                { Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp)) }
                            } else null,
                            onClick = {
                                onSelected(option)
                                expanded = false
                            }
                        )
                    }
            }
        }
    }
}

/**
 * Single Material Design 3 menu container used by every dropdown in the app:
 * 20dp rounded corners, surface colour, hairline accent outline and 6dp of
 * vertical padding around the items.
 */
@Composable
fun LumenMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(24.dp)
    // The popup surface draws its own container, so the rounding is set through the
    // theme shape; an extra clip/background here produced a second, square outline.
    MaterialTheme(shapes = MaterialTheme.shapes.copy(extraSmall = shape)) {
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = onDismissRequest,
            modifier = modifier.widthIn(min = 220.dp)
        ) {
            Column(Modifier.padding(vertical = 6.dp)) { content() }
        }
    }
}

/** High contrast, vibrant theme switch component. */
@Composable
fun LumenSwitch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true
) {
    androidx.compose.material3.Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier,
        enabled = enabled,
        thumbContent = {
            Icon(
                imageVector = if (checked) Icons.Filled.Check else Icons.Filled.Close,
                contentDescription = null,
                modifier = Modifier.size(androidx.compose.material3.SwitchDefaults.IconSize)
            )
        },
        colors = androidx.compose.material3.SwitchDefaults.colors(
            checkedThumbColor = MaterialTheme.colorScheme.onPrimary,
            checkedTrackColor = MaterialTheme.colorScheme.primary,
            checkedBorderColor = Color.Transparent,
            checkedIconColor = MaterialTheme.colorScheme.primary,
            uncheckedThumbColor = MaterialTheme.colorScheme.outline,
            uncheckedTrackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
            uncheckedBorderColor = MaterialTheme.colorScheme.outlineVariant,
            uncheckedIconColor = MaterialTheme.colorScheme.surfaceContainerHighest
        )
    )
}

/** Material Design 3 Card container */
@Composable
fun LumenCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(20.dp)
    Box(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(16.dp)
    ) {
        content()
    }
}

/** Small caps caption above a group of cards, as in the 2.x redesign. */
@Composable
fun SectionHeader(title: String, modifier: Modifier = Modifier) {
    Row(modifier.padding(start = 2.dp, top = 24.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(5.dp).height(20.dp).clip(RoundedCornerShape(4.dp))
            .background(MaterialTheme.colorScheme.primary))
        Spacer(Modifier.width(10.dp))
        Text(title, style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface)
    }
}

/**
 * Latency colour thresholds, kept in sync with the ping settings so a user can
 * decide what counts as a good or an average server.
 */
object PingThresholds {
    var goodMs by androidx.compose.runtime.mutableStateOf(150)
    var fairMs by androidx.compose.runtime.mutableStateOf(300)
}

/** Latency colours; LumenTheme points them at the active palette. */
object PingColors {
    var good by androidx.compose.runtime.mutableStateOf(ConnectionSuccess)
    var fair by androidx.compose.runtime.mutableStateOf(ConnectionWarning)
    var bad by androidx.compose.runtime.mutableStateOf(ConnectionDanger)
}

fun pingColor(ping: Int): Color = when {
    ping <= 0 -> PingColors.bad
    ping <= PingThresholds.goodMs -> PingColors.good
    ping <= PingThresholds.fairMs -> PingColors.fair
    else -> PingColors.bad
}

/** Keep semantic green/amber/red readable on both the active dark card and light rows. */
fun pingColor(ping: Int, background: Color): Color {
    val original = pingColor(ping)
    val target = if (background.luminance() < .4f) Color.White else Color.Black
    for (step in 0..10) {
        val color = androidx.compose.ui.graphics.lerp(original, target, step / 10f)
        val ratio = (maxOf(color.luminance(), background.luminance()) + .05f) /
            (minOf(color.luminance(), background.luminance()) + .05f)
        if (ratio >= 4.5f) return color
    }
    return target
}

/** Badge colour per protocol family; VLESS/VMess/Trojan intentionally share one. */
fun protocolColor(protocol: String): Color = when (protocol.trim().lowercase()) {
    "vless", "vmess", "trojan" -> Color(0xFF00C8E8)
    "ss", "shadowsocks", "ssr" -> Color(0xFF8B7BFF)
    "hysteria", "hysteria2", "hy2" -> Color(0xFFFF8A3D)
    "tuic" -> Color(0xFFFFC53D)
    "wireguard", "wg" -> Color(0xFF34D399)
    "awg", "amneziawg" -> Color(0xFF16A97A)
    "openvpn", "ovpn" -> Color(0xFFFF6B81)
    "socks", "socks5", "http", "https" -> Color(0xFF9AA6B8)
    "auto", "urltest", "url-test" -> Color(0xFFC08CFF)
    else -> Color(0xFF9AA6B8)
}

/** Header for settings destinations, matching the settings hub hierarchy. */
@Composable
fun SettingsDetailHeader(
    title: String, description: String, onBack: (() -> Unit)?,
    applyStatusBarPadding: Boolean = false, actions: (@Composable () -> Unit)? = null
) {
    val art = com.lumen.ui.theme.expressivePalette()
    Column(Modifier.fillMaxWidth()
        .then(if (applyStatusBarPadding) Modifier.statusBarsPadding() else Modifier)
        .padding(top = 8.dp, bottom = 12.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween) {
            if (onBack != null) Row(
                Modifier.height(48.dp).clip(RoundedCornerShape(24.dp))
                    .background(MaterialTheme.colorScheme.surfaceContainer)
                    .clickable(role = androidx.compose.ui.semantics.Role.Button, onClick = onBack)
                    .padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, null, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text(LocalStrings.current.back, style = MaterialTheme.typography.labelLarge)
            }
            com.lumen.ui.components.ExpressiveMotif(com.lumen.ui.components.Motif.Sun, art.stage,
                Modifier.size(28.dp))
        }
        Text(title, style = MaterialTheme.typography.headlineLarge,
            color = MaterialTheme.colorScheme.onSurface, modifier = Modifier.padding(top = 18.dp))
        if (description.isNotBlank()) Text(description, style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 8.dp, bottom = 4.dp))
        if (actions != null) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) { actions() }
    }
}

@Composable
fun LumenScreenHeader(
    title: String, onBack: (() -> Unit)? = null, subtitle: String? = null,
    subtitleColor: Color? = null, actions: (@Composable () -> Unit)? = null,
    stackSubtitleOnCompact: Boolean = true, applyStatusBarPadding: Boolean = true,
    modifier: Modifier = Modifier
) {
    Column(modifier.fillMaxWidth()
        .then(if (applyStatusBarPadding) Modifier.statusBarsPadding() else Modifier)
        .padding(vertical = 10.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            if (onBack != null) {
                IconButton(onClick = onBack, modifier = Modifier.size(48.dp)
                    .clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.surfaceContainer)) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, LocalStrings.current.back)
                }
                Spacer(Modifier.width(12.dp))
            }
            Text(title, style = if (onBack == null) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineMedium,
                modifier = Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (actions != null) actions()
        }
        if (!subtitle.isNullOrBlank()) Text(subtitle, style = MaterialTheme.typography.bodyMedium,
            color = subtitleColor ?: MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 8.dp, start = if (onBack != null) 60.dp else 0.dp))
    }
}

/**
 * True while the device itself plays touch feedback.
 *
 * Read instead of trusting `View.performHapticFeedback`: the platform routes the tick
 * through the window session, which returns true as soon as a vibration effect exists for
 * the constant - the vibrator service drops it afterwards when the user's touch feedback
 * setting is off. So the return value cannot be used to decide whether to fall back.
 */
private fun systemHapticFeedbackEnabled(context: android.content.Context): Boolean = runCatching {
    val resolver = context.contentResolver
    val enabled = android.provider.Settings.System.getInt(
        resolver,
        android.provider.Settings.System.HAPTIC_FEEDBACK_ENABLED,
        1
    ) != 0
    // Android 12 added a separate per-usage strength; 0 mutes touch feedback while
    // HAPTIC_FEEDBACK_ENABLED stays at 1. The key is not public API, hence the literal.
    val intensity = if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
        android.provider.Settings.System.getInt(resolver, "haptic_feedback_intensity", 2)
    } else {
        2
    }
    enabled && intensity != 0
}.getOrDefault(true)

/**
 * Plain vibration used when system touch feedback is muted. It is played with the default
 * (unknown) usage on purpose: a TOUCH usage would be suppressed by the very setting this
 * fallback exists to work around, leaving the in-app switch with nothing to do.
 */
private fun vibrateTick(vibrator: android.os.Vibrator, longPress: Boolean) {
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
        vibrator.vibrate(
            android.os.VibrationEffect.createPredefined(
                if (longPress) android.os.VibrationEffect.EFFECT_HEAVY_CLICK
                else android.os.VibrationEffect.EFFECT_TICK
            )
        )
    } else {
        vibrator.vibrate(
            android.os.VibrationEffect.createOneShot(
                if (longPress) 30L else 14L,
                android.os.VibrationEffect.DEFAULT_AMPLITUDE
            )
        )
    }
}

/**
 * Haptic tick, muted while the in-app vibration setting is off.
 *
 * System haptics are preferred while the device plays them: they give the short, tuned
 * tick the hardware is designed for. When the device's own "touch feedback" switch is off
 * they are dropped silently, so the plain vibrator is driven directly instead - the in-app
 * switch is the user asking for feedback, and it has to work on its own.
 */
@Composable
fun rememberHapticTick(): (androidx.compose.ui.hapticfeedback.HapticFeedbackType) -> Unit {
    val haptics = androidx.compose.ui.platform.LocalHapticFeedback.current
    val enabled = LocalHapticsEnabled.current
    val context = androidx.compose.ui.platform.LocalContext.current
    val vibrator = remember(context) {
        runCatching {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
                (context.getSystemService(android.content.Context.VIBRATOR_MANAGER_SERVICE)
                    as? android.os.VibratorManager)?.defaultVibrator
            } else {
                @Suppress("DEPRECATION")
                context.getSystemService(android.content.Context.VIBRATOR_SERVICE) as? android.os.Vibrator
            }
        }.getOrNull()?.takeIf { runCatching { it.hasVibrator() }.getOrDefault(false) }
    }
    val view = androidx.compose.ui.platform.LocalView.current
    return remember(enabled, haptics, vibrator, view, context) {
        { type: androidx.compose.ui.hapticfeedback.HapticFeedbackType ->
            if (enabled) {
                val longPress = type == androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress
                // 1. System haptics, but only while the device actually plays them.
                var played = if (systemHapticFeedbackEnabled(context)) {
                    runCatching {
                        view.performHapticFeedback(
                            if (longPress) android.view.HapticFeedbackConstants.LONG_PRESS
                            else android.view.HapticFeedbackConstants.VIRTUAL_KEY
                        )
                    }.getOrDefault(false)
                } else {
                    false
                }
                // 2. Plain vibrator so the in-app switch keeps working with system
                //    touch feedback disabled.
                if (!played) {
                    vibrator?.let { vib ->
                        played = runCatching { vibrateTick(vib, longPress) }.isSuccess
                    }
                }
                // 3. Last resort: whatever Compose can still produce.
                if (!played) runCatching { haptics.performHapticFeedback(type) }
            }
        }
    }
}

/**
 * Height of a filter chip. Fixed so a long subscription name can never turn the
 * chip into a tall rectangle next to the 38dp filter icons.
 */
private val FilterChipHeight = 34.dp

/** Compact selectable chip used by the server filters. */
@Composable
fun LumenFilterChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    badge: String? = null,
    leading: String? = null
) {
    // Expressive selection changes the silhouette as well as its color.
    val corner by androidx.compose.animation.core.animateDpAsState(
        targetValue = if (selected) FilterChipHeight / 2 else 11.dp,
        animationSpec = androidx.compose.animation.core.spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow
        ),
        label = "filter_chip_shape"
    )
    val shape = RoundedCornerShape(corner)
    val tick = rememberHapticTick()
    val bg by androidx.compose.animation.animateColorAsState(
        targetValue = if (selected) MaterialTheme.colorScheme.primary
            else MaterialTheme.colorScheme.surfaceVariant,
        label = "chip_bg"
    )
    val borderColor by androidx.compose.animation.animateColorAsState(
        targetValue = if (selected) MaterialTheme.colorScheme.primary
            else MaterialTheme.colorScheme.outlineVariant,
        label = "chip_border"
    )
    val fg = if (selected) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
    Row(
        modifier = modifier.expressiveButtonSemantics(label + (badge?.let { " · $it" } ?: ""), onClick)
            .height(FilterChipHeight)
            .clip(shape)
            .background(bg)
            .border(1.dp, borderColor, shape)
            .clickable {
                tick(androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove)
                onClick()
            }
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (!leading.isNullOrBlank()) {
            Text(leading, style = MaterialTheme.typography.bodySmall, maxLines = 1)
            Spacer(Modifier.width(6.dp))
        }
        // Weight without fill: a short label still hugs its text, while a long one
        // gives the badge the room it needs instead of squeezing it to zero width,
        // which used to wrap the count onto three lines and blow up the chip.
        Text(
            text = label,
            style = MaterialTheme.typography.labelLarge,
            color = fg,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            // Capped width instead of weight: chips also live in horizontally scrolling
            // rails, where a weighted child gets no width at all.
            modifier = Modifier.widthIn(max = 200.dp)
        )
        if (!badge.isNullOrBlank()) {
            Spacer(Modifier.width(6.dp))
            Text(
                text = badge,
                style = MaterialTheme.typography.labelSmall,
                color = fg.copy(alpha = 0.75f),
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                softWrap = false
            )
        }
    }
}

/** Walks the context wrappers Compose hands out until the hosting activity is found. */
private fun android.content.Context.findActivity(): android.app.Activity? {
    var current: android.content.Context = this
    while (current is android.content.ContextWrapper) {
        if (current is android.app.Activity) return current
        current = current.baseContext
    }
    return null
}

/**
 * Camera permission gate for the QR scanner.
 *
 * The scanner library shows its own decade-old "Barcode Scanner / the camera encountered a
 * problem" dialog when it cannot open the camera, which names a library the user never
 * installed and blames the device instead of the denied permission. Asking here means the
 * scanner is only ever launched with the permission in hand, and a refusal is explained in
 * the app's own words.
 */
@Composable
fun rememberQrScanRequest(onScan: () -> Unit): () -> Unit {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scan = androidx.compose.runtime.rememberUpdatedState(onScan)
    var deniedPermanently by remember { mutableStateOf<Boolean?>(null) }
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) {
            scan.value()
        } else {
            // No rationale after a denial means the user chose "don't ask again": the
            // only way back is the app settings page.
            deniedPermanently = context.findActivity()?.let {
                !androidx.core.app.ActivityCompat.shouldShowRequestPermissionRationale(
                    it,
                    android.Manifest.permission.CAMERA
                )
            } ?: true
        }
    }
    // A Dialog draws in its own window, so emitting it here costs the caller no layout.
    deniedPermanently?.let { permanent ->
        CameraPermissionDialog(
            permanentlyDenied = permanent,
            onDismiss = { deniedPermanently = null },
            onOpenSettings = {
                deniedPermanently = null
                runCatching {
                    context.startActivity(
                        android.content.Intent(
                            android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                            android.net.Uri.fromParts("package", context.packageName, null)
                        ).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    )
                }
            }
        )
    }
    return {
        val granted = androidx.core.content.ContextCompat.checkSelfPermission(
            context,
            android.Manifest.permission.CAMERA
        ) == android.content.pm.PackageManager.PERMISSION_GRANTED
        if (granted) scan.value() else launcher.launch(android.Manifest.permission.CAMERA)
    }
}

/** The app's own explanation for a refused camera permission. */
@Composable
fun CameraPermissionDialog(
    permanentlyDenied: Boolean,
    onDismiss: () -> Unit,
    onOpenSettings: () -> Unit
) {
    val s = LocalStrings.current
    com.lumen.ui.components.LumenDialog(
        title = s.cameraPermissionTitle,
        message = if (permanentlyDenied) s.cameraPermissionBlocked else s.cameraPermissionMessage,
        onDismissRequest = onDismiss,
        confirmText = if (permanentlyDenied) s.openAppSettings else "OK",
        onConfirm = if (permanentlyDenied) onOpenSettings else onDismiss,
        dismissText = if (permanentlyDenied) s.cancel else null,
        onDismiss = onDismiss
    )
}

/**
 * Small UI-scoped preference holder backed by the app's SharedPreferences.
 * Screens use it to remember their last used filters, groups and tabs across
 * process restarts without plumbing every value through the ViewModel.
 */
@Composable
fun rememberUiPreference(
    key: String,
    default: String
): androidx.compose.runtime.MutableState<String> {
    val context = androidx.compose.ui.platform.LocalContext.current
    val prefs = remember(context) {
        context.getSharedPreferences("lumen_prefs", android.content.Context.MODE_PRIVATE)
    }
    val backing = remember(key) { mutableStateOf(prefs.getString(key, default) ?: default) }
    // An import can retarget the selected group from the view model while this screen
    // is already composed, so the value has to follow external writes too - a plain
    // remember would keep showing the group the user was on before the import.
    androidx.compose.runtime.DisposableEffect(prefs, key) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { store, changed ->
            if (changed == key) backing.value = store.getString(key, default) ?: default
        }
        prefs.registerOnSharedPreferenceChangeListener(listener)
        onDispose { prefs.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    return remember(key) {
        object : androidx.compose.runtime.MutableState<String> {
            override var value: String
                get() = backing.value
                set(newValue) {
                    if (backing.value != newValue) {
                        backing.value = newValue
                        prefs.edit().putString(key, newValue).apply()
                    }
                }

            override fun component1(): String = value
            override fun component2(): (String) -> Unit = { value = it }
        }
    }
}
