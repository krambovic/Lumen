package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.unit.dp
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

fun pingColor(ping: Int): Color = when {
    ping < 0 -> ConnectionDanger
    ping <= 120 -> ConnectionSuccess
    ping <= 300 -> ConnectionWarning
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
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(horizontal = 16.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.weight(1f)
        ) {
            if (onBack != null) {
                IconButton(
                    onClick = onBack,
                    modifier = Modifier.size(34.dp)
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        tint = MaterialTheme.colorScheme.onSurface
                    )
                }
                Spacer(Modifier.width(6.dp))
            }
            Text(
                text = title,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.ExtraBold,
                color = MaterialTheme.colorScheme.onSurface
            )
            if (!subtitle.isNullOrBlank()) {
                Spacer(Modifier.width(8.dp))
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.primary
                )
            }
        }
        if (actions != null) {
            actions()
        }
    }
}

/** Compose haptic tick, muted while the vibration setting is off. */
@Composable
fun rememberHapticTick(): (androidx.compose.ui.hapticfeedback.HapticFeedbackType) -> Unit {
    val haptics = androidx.compose.ui.platform.LocalHapticFeedback.current
    val enabled = LocalHapticsEnabled.current
    return remember(enabled, haptics) {
        { type: androidx.compose.ui.hapticfeedback.HapticFeedbackType ->
            if (enabled) runCatching { haptics.performHapticFeedback(type) }
        }
    }
}

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
            .clip(shape)
            .background(bg)
            .border(1.dp, borderColor, shape)
            .clickable {
                tick(androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove)
                onClick()
            }
            .padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (!leading.isNullOrBlank()) {
            Text(leading, style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.width(6.dp))
        }
        Text(
            text = label,
            style = MaterialTheme.typography.bodySmall,
            color = fg,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium
        )
        if (!badge.isNullOrBlank()) {
            Spacer(Modifier.width(6.dp))
            Text(
                text = badge,
                style = MaterialTheme.typography.labelSmall,
                color = fg.copy(alpha = 0.75f),
                fontWeight = FontWeight.SemiBold
            )
        }
    }
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
