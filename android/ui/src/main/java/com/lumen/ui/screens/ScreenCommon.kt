package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.ui.draw.shadow

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
    val menuShape = RoundedCornerShape(20.dp)

    Column(modifier) {
        if (label.isNotEmpty()) {
            Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(4.dp))
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f), RoundedCornerShape(14.dp))
                .clickable { expanded = true }
                .padding(horizontal = 14.dp, vertical = 12.dp)
        ) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(optionLabel(selected), color = MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                Text("\u25BE", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
            }
            // Rounding comes from the theme shape: clipping the popup content left a
            // second square outline around the menu.
            MaterialTheme(shapes = MaterialTheme.shapes.copy(extraSmall = menuShape)) {
            DropdownMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false },
                modifier = Modifier
                    .widthIn(min = 220.dp)
                    .border(BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)), menuShape)
            ) {
                Column(Modifier.padding(vertical = 6.dp)) {
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
    val shape = RoundedCornerShape(20.dp)
    // The popup surface draws its own container, so the rounding is set through the
    // theme shape; an extra clip/background here produced a second, square outline.
    MaterialTheme(shapes = MaterialTheme.shapes.copy(extraSmall = shape)) {
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = onDismissRequest,
            modifier = modifier
                .widthIn(min = 220.dp)
                .border(
                    BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.35f)),
                    shape
                )
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
        colors = androidx.compose.material3.SwitchDefaults.colors(
            checkedThumbColor = Color.White,
            checkedTrackColor = MaterialTheme.colorScheme.primary,
            checkedBorderColor = Color.Transparent,
            checkedIconColor = MaterialTheme.colorScheme.primary,
            uncheckedThumbColor = MaterialTheme.colorScheme.onSurfaceVariant,
            uncheckedTrackColor = MaterialTheme.colorScheme.surfaceVariant,
            uncheckedBorderColor = MaterialTheme.colorScheme.outline
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
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f), shape)
            .padding(16.dp)
    ) {
        content()
    }
}

@Composable
fun SectionHeader(title: String, modifier: Modifier = Modifier) {
    Text(
        title,
        style = MaterialTheme.typography.titleMedium,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = modifier.padding(top = 16.dp, bottom = 8.dp)
    )
}

/**
 * Latency colour thresholds, kept in sync with the ping settings so a user can
 * decide what counts as a good or an average server.
 */
object PingThresholds {
    var goodMs by androidx.compose.runtime.mutableStateOf(150)
    var fairMs by androidx.compose.runtime.mutableStateOf(300)
}

fun pingColor(ping: Int): Color = when {
    ping <= 0 -> ConnectionDanger
    ping <= PingThresholds.goodMs -> ConnectionSuccess
    ping <= PingThresholds.fairMs -> ConnectionWarning
    else -> ConnectionDanger
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

@Composable
fun LumenScreenHeader(
    title: String,
    onBack: (() -> Unit)? = null,
    subtitle: String? = null,
    actions: (@Composable () -> Unit)? = null,
    stackSubtitleOnCompact: Boolean = true,
    applyStatusBarPadding: Boolean = true,
    modifier: Modifier = Modifier
) {
    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .then(if (applyStatusBarPadding) Modifier.statusBarsPadding() else Modifier)
            .padding(vertical = 6.dp),
    ) {
        val compactHeader = maxWidth < 520.dp &&
            stackSubtitleOnCompact &&
            !subtitle.isNullOrBlank()
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.Start,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(
                    onClick = onBack,
                    modifier = Modifier
                        .size(48.dp)
                        .offset(x = (-2).dp)
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        tint = MaterialTheme.colorScheme.onSurface
                    )
                }
                Spacer(Modifier.width(6.dp))
            }
            if (compactHeader) {
                Column(modifier = Modifier.weight(1f)) {
                    HeaderTitle(title, Modifier.fillMaxWidth())
                    Text(
                        text = subtitle.orEmpty(),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.primary,
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Clip,
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            } else {
                if (subtitle.isNullOrBlank()) {
                    HeaderTitle(title, Modifier.weight(1f))
                } else {
                    HeaderTitle(title, Modifier.wrapContentWidth())
                    Spacer(Modifier.width(8.dp))
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.primary,
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Clip,
                        modifier = Modifier.wrapContentWidth()
                    )
                    Spacer(Modifier.weight(1f))
                }
            }
            if (actions != null) {
                actions()
            }
        }
    }
}

@Composable
private fun HeaderTitle(title: String, modifier: Modifier = Modifier) {
    Text(
        text = title,
        style = headerTitleStyle(title),
        fontWeight = FontWeight.ExtraBold,
        color = MaterialTheme.colorScheme.onSurface,
        maxLines = 2,
        overflow = TextOverflow.Clip,
        modifier = modifier
    )
}

@Composable
private fun headerTitleStyle(title: String) = MaterialTheme.typography.headlineSmall.copy(
    fontSize = if (title.length >= 18) 24.sp else MaterialTheme.typography.headlineSmall.fontSize,
    lineHeight = if (title.length >= 18) 30.sp else MaterialTheme.typography.headlineSmall.lineHeight
)

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
private val FilterChipHeight = 38.dp

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
    val shape = RoundedCornerShape(12.dp)
    val tick = rememberHapticTick()
    val bg by androidx.compose.animation.animateColorAsState(
        targetValue = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f)
            else MaterialTheme.colorScheme.surfaceVariant,
        label = "chip_bg"
    )
    val borderColor by androidx.compose.animation.animateColorAsState(
        targetValue = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.65f)
            else MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f),
        label = "chip_border"
    )
    val fg = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = modifier
            .height(FilterChipHeight)
            .clip(shape)
            .background(bg)
            .border(1.dp, borderColor, shape)
            .clickable {
                tick(androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove)
                onClick()
            }
            .padding(horizontal = 12.dp),
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
            style = MaterialTheme.typography.bodySmall,
            color = fg,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false)
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
