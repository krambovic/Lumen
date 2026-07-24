package com.lumen.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.util.Locale

@Composable
fun CountryFlagIcon(
    countryCode: String,
    modifier: Modifier = Modifier,
    width: Dp = 24.dp,
    height: Dp = 16.dp,
    fallbackText: String? = null
) {
    val code = countryCode.trim().uppercase(Locale.US)
    // Unknown country: show the US flag placeholder instead of a protocol text badge.
    val flagData = CountryFlagHelper.STRIPES[code] ?: CountryFlagHelper.STRIPES["US"]
    val shape = RoundedCornerShape(3.dp)

    if (flagData != null) {
        Canvas(
            modifier = modifier
                .size(width, height)
                .clip(shape)
        ) {
            val w = size.width
            val h = size.height

            val roundedRectPath = Path().apply {
                addRoundRect(
                    RoundRect(
                        rect = Rect(0f, 0f, w, h),
                        cornerRadius = CornerRadius(3.dp.toPx(), 3.dp.toPx())
                    )
                )
            }

            clipPath(roundedRectPath) {
                when (flagData.style) {
                    StripeStyle.Horizontal -> {
                        val stripeHeight = h / flagData.colors.size
                        flagData.colors.forEachIndexed { i, color ->
                            drawRect(
                                color = color,
                                topLeft = Offset(0f, i * stripeHeight),
                                size = Size(w, stripeHeight + 0.5f)
                            )
                        }
                    }
                    StripeStyle.Vertical -> {
                        val stripeWidth = w / flagData.colors.size
                        flagData.colors.forEachIndexed { i, color ->
                            drawRect(
                                color = color,
                                topLeft = Offset(i * stripeWidth, 0f),
                                size = Size(stripeWidth + 0.5f, h)
                            )
                        }
                    }
                    StripeStyle.Nordic -> {
                        val baseColor = flagData.colors[0]
                        val outerColor = flagData.colors.getOrElse(1) { Color.White }
                        val innerColor = flagData.colors.getOrNull(2)

                        drawRect(color = baseColor, size = Size(w, h))

                        // Outer cross
                        drawRect(color = outerColor, topLeft = Offset(0f, h * 0.35f), size = Size(w, h * 0.3f))
                        drawRect(color = outerColor, topLeft = Offset(w * 0.3f, 0f), size = Size(w * 0.2f, h))

                        // Inner cross if 3 colors
                        if (innerColor != null) {
                            drawRect(color = innerColor, topLeft = Offset(0f, h * 0.42f), size = Size(w, h * 0.16f))
                            drawRect(color = innerColor, topLeft = Offset(w * 0.34f, 0f), size = Size(w * 0.12f, h))
                        }
                    }
                    StripeStyle.Cross -> {
                        val baseColor = flagData.colors[0]
                        val crossColor = flagData.colors.getOrElse(1) { Color.White }

                        drawRect(color = baseColor, size = Size(w, h))
                        drawRect(color = crossColor, topLeft = Offset(0f, h * 0.35f), size = Size(w, h * 0.3f))
                        drawRect(color = crossColor, topLeft = Offset(w * 0.4f, 0f), size = Size(w * 0.2f, h))
                    }
                }
            }

            // Outer border line
            drawRoundRect(
                color = Color(0x26000000),
                topLeft = Offset(0.5f, 0.5f),
                size = Size(w - 1f, h - 1f),
                cornerRadius = CornerRadius(3.dp.toPx(), 3.dp.toPx()),
                style = Stroke(width = 1.dp.toPx())
            )
        }
    } else {
        val labelText = (fallbackText ?: code.take(3)).ifBlank { "VPN" }
        Box(
            modifier = modifier
                .size(width, height)
                .clip(shape)
                .background(androidx.compose.material3.MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.85f))
                .border(1.dp, androidx.compose.material3.MaterialTheme.colorScheme.primary.copy(alpha = 0.4f), shape),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = labelText.take(3).uppercase(Locale.US),
                color = androidx.compose.material3.MaterialTheme.colorScheme.onPrimaryContainer,
                fontSize = 8.5.sp,
                fontWeight = FontWeight.Bold
            )
        }
    }
}
