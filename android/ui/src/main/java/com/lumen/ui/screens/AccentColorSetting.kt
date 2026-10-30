package com.lumen.ui.screens

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
internal fun AccentColorSetting(value: String, language: String, onChange: (String) -> Unit) {
    var custom by rememberSaveable { mutableStateOf(false) }
    val options = listOf("#6937D4", "#3174BF", "#237D70", "#22C55E", "#F97316", "#DB2777")
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(WelcomeText.ACCENT.inLanguage(language), style = MaterialTheme.typography.titleMedium)
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            options.forEach { hex ->
                val chosen = hex.equals(value, true)
                val color = Color(android.graphics.Color.parseColor(hex))
                Box(Modifier.size(48.dp).semantics {
                    selected = chosen
                    contentDescription = "${WelcomeText.ACCENT.inLanguage(language)} $hex"
                }.clip(com.lumen.ui.components.selectionShape(chosen, 12.dp, 22.dp)).background(color)
                    .clickable(role = Role.RadioButton) { onChange(if (chosen) "" else hex) }, contentAlignment = Alignment.Center) {
                    if (chosen) Icon(Icons.Default.Check, null, tint = if (color.luminance() > .45f) Color.Black else Color.White)
                }
            }
        }
        OutlinedButton(onClick = { custom = true }, modifier = Modifier.fillMaxWidth().testTag("accent_custom")) {
            Icon(Icons.Default.Palette, null, Modifier.size(20.dp))
            Text(WelcomeText.CUSTOM_ACCENT.inLanguage(language), Modifier.padding(start = 8.dp))
        }
    }
    if (custom) AccentPickerDialog(value, language, onDismiss = { custom = false }, onApply = {
        onChange(it)
        custom = false
    })
}

@Composable
private fun AccentPickerDialog(value: String, language: String, onDismiss: () -> Unit, onApply: (String) -> Unit) {
    val s = LocalStrings.current
    val initial = remember {
        FloatArray(3).also { android.graphics.Color.colorToHSV(
            android.graphics.Color.parseColor(value.takeIf { it.matches(Regex("#[0-9a-fA-F]{6}")) } ?: "#6937D4"), it) }
    }
    var hue by rememberSaveable { mutableFloatStateOf(initial[0]) }
    var saturation by rememberSaveable { mutableFloatStateOf(initial[1]) }
    var brightness by rememberSaveable { mutableFloatStateOf(initial[2]) }
    fun color(h: Float = hue, sat: Float = saturation, bright: Float = brightness) =
        Color(android.graphics.Color.HSVToColor(floatArrayOf(h, sat, bright)))
    val preview = color()
    val hex = "#%06X".format(android.graphics.Color.HSVToColor(floatArrayOf(hue, saturation, brightness)) and 0xFFFFFF)
    var hexDraft by rememberSaveable { mutableStateOf(hex) }
    LaunchedEffect(hex) { hexDraft = hex }
    AlertDialog(onDismissRequest = onDismiss, title = { Text(WelcomeText.CUSTOM_ACCENT.inLanguage(language)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.fillMaxWidth().height(56.dp).clip(RoundedCornerShape(18.dp)).background(preview), contentAlignment = Alignment.Center) {
                    Text(hex, color = if (preview.luminance() > .45f) Color.Black else Color.White,
                        style = MaterialTheme.typography.titleLarge)
                }
                Canvas(Modifier.fillMaxWidth().height(150.dp).clip(RoundedCornerShape(16.dp))
                    .testTag("accent_saturation_value")
                    .pointerInput(Unit) {
                        awaitEachGesture {
                            val down = awaitFirstDown()
                            fun choose(point: Offset) {
                                saturation = (point.x / size.width.coerceAtLeast(1)).coerceIn(0f, 1f)
                                brightness = (1f - point.y / size.height.coerceAtLeast(1)).coerceIn(0f, 1f)
                            }
                            choose(down.position)
                            down.consume()
                            do {
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                choose(change.position)
                                change.consume()
                            } while (event.changes.any { it.id == down.id && it.pressed })
                        }
                    }) {
                    drawRect(Brush.horizontalGradient(listOf(Color.White, color(hue, 1f, 1f))))
                    drawRect(Brush.verticalGradient(listOf(Color.Transparent, Color.Black)))
                    val cursor = Offset(saturation * size.width, (1f - brightness) * size.height)
                    drawCircle(Color.Black, 9.dp.toPx(), cursor, style = Stroke(4.dp.toPx()))
                    drawCircle(Color.White, 9.dp.toPx(), cursor, style = Stroke(2.dp.toPx()))
                }
                ExpressiveTextField(hexDraft, { entered ->
                    hexDraft = entered.take(7)
                    if (hexDraft.matches(Regex("#[0-9a-fA-F]{6}"))) {
                        val hsv = FloatArray(3)
                        android.graphics.Color.colorToHSV(android.graphics.Color.parseColor(hexDraft), hsv)
                        hue = hsv[0]; saturation = hsv[1]; brightness = hsv[2]
                    }
                }, Modifier.fillMaxWidth().testTag("accent_hex"), singleLine = true,
                    label = { Text("HEX") }, placeholder = { Text("#RRGGBB") },
                    isError = !hexDraft.matches(Regex("#[0-9a-fA-F]{6}")))
                Text(WelcomeText.HUE.inLanguage(language), style = MaterialTheme.typography.labelMedium)
                Box(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(8.dp)).background(Brush.horizontalGradient(
                    (0..6).map { color(it * 60f, 1f, 1f) })))
                Slider(hue, { hue = it }, valueRange = 0f..360f, modifier = Modifier.testTag("accent_hue")
                    .semantics { contentDescription = WelcomeText.HUE.inLanguage(language) })
                Text(WelcomeText.SATURATION.inLanguage(language), style = MaterialTheme.typography.labelMedium)
                Slider(saturation, { saturation = it }, modifier = Modifier.semantics { contentDescription = WelcomeText.SATURATION.inLanguage(language) })
                Text(WelcomeText.BRIGHTNESS.inLanguage(language), style = MaterialTheme.typography.labelMedium)
                Slider(brightness, { brightness = it }, modifier = Modifier.semantics { contentDescription = WelcomeText.BRIGHTNESS.inLanguage(language) })
            }
        }, confirmButton = { TextButton({ onApply(hex) }, Modifier.testTag("accent_apply"), enabled = hexDraft.matches(Regex("#[0-9a-fA-F]{6}"))) { Text(s.done) } },
        dismissButton = { TextButton(onDismiss) { Text(s.cancel) } })
}
