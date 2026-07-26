package com.lumen.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * Vector country flag. Simple stripe flags are painted from [CountryFlagHelper.STRIPES];
 * detailed flags (US, UK, JP, CN, KR, TR, CA...) have dedicated painters so they no
 * longer degrade into two or three meaningless bands.
 */
@Composable
fun CountryFlagIcon(
    countryCode: String,
    modifier: Modifier = Modifier,
    width: Dp = 24.dp,
    height: Dp = 16.dp,
    fallbackText: String? = null
) {
    val rawCode = countryCode.trim().uppercase()
    val flagData = CountryFlagHelper.STRIPES[rawCode]
    val shape = RoundedCornerShape(3.dp)
    Box(
        modifier = modifier
            .width(width)
            .height(height)
            .clip(shape)
            .border(1.dp, Color(0x26000000), shape)
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val w = size.width
            val h = size.height
            if (flagData == null) {
                // Unknown non-WARP locations stay neutral instead of being
                // misrepresented as the United States.
                drawRect(Color(0xFF607080))
                drawLine(
                    Color(0x80FFFFFF),
                    Offset(0f, h),
                    Offset(w, 0f),
                    strokeWidth = (h * 0.08f).coerceAtLeast(1f)
                )
                return@Canvas
            }
            val c = flagData.colors
            when (flagData.style) {
                StripeStyle.Horizontal -> drawBands(c, horizontal = true)
                StripeStyle.Vertical -> drawBands(c, horizontal = false)
                StripeStyle.Nordic -> drawNordic(c)
                StripeStyle.Cross -> drawCross(c)
                StripeStyle.UsStars -> drawUsFlag(c)
                StripeStyle.UnionJack -> drawUnionJack(c)
                StripeStyle.Disc -> drawDisc(c)
                StripeStyle.CnStars -> drawChina(c)
                StripeStyle.Crescent -> drawCrescent(c)
                StripeStyle.MapleLeaf -> drawCanada(c)
                StripeStyle.Taegeuk -> drawKorea(c)
                StripeStyle.Albania -> drawAlbania(c)
                StripeStyle.Bauhinia -> drawHongKong(c)
                StripeStyle.Spain -> drawSpain(c)
                StripeStyle.India -> drawIndia(c)
                StripeStyle.Kazakhstan -> drawKazakhstan(c)
                StripeStyle.Australia -> drawAustralia(c)
                StripeStyle.Malaysia -> drawMalaysia(c)
                StripeStyle.Macedonia -> drawMacedonia(c)
                StripeStyle.Czech -> drawCzech(c)
                StripeStyle.EuropeanUnion -> drawEuropeanUnion(c)
            }
            // Subtle top highlight keeps the tiny flags from looking flat.
            drawRect(color = Color(0x14FFFFFF), size = Size(w, h * 0.5f))
        }
    }
}

private fun DrawScope.drawBands(colors: List<Color>, horizontal: Boolean) {
    if (colors.isEmpty()) return
    val count = colors.size
    if (horizontal) {
        val band = size.height / count
        colors.forEachIndexed { index, color ->
            drawRect(color = color, topLeft = Offset(0f, band * index), size = Size(size.width, band + 0.5f))
        }
    } else {
        val band = size.width / count
        colors.forEachIndexed { index, color ->
            drawRect(color = color, topLeft = Offset(band * index, 0f), size = Size(band + 0.5f, size.height))
        }
    }
}

/** Czechia: white/red halves with the blue hoist triangle. */
private fun DrawScope.drawCzech(colors: List<Color>) {
    val white = colors.getOrElse(0) { Color.White }
    val red = colors.getOrElse(1) { Color(0xFFD7141A) }
    val blue = colors.getOrElse(2) { Color(0xFF11457E) }
    drawRect(white)
    drawRect(
        color = red,
        topLeft = Offset(0f, size.height / 2f),
        size = Size(size.width, size.height / 2f)
    )
    val triangle = Path().apply {
        moveTo(0f, 0f)
        lineTo(size.width * 0.46f, size.height / 2f)
        lineTo(0f, size.height)
        close()
    }
    drawPath(triangle, blue)
}

private fun DrawScope.drawNordic(colors: List<Color>) {
    val base = colors.getOrElse(0) { Color.White }
    val cross = colors.getOrElse(1) { Color.White }
    val inner = colors.getOrNull(2)
    drawRect(color = base)
    val armY = size.height * 0.28f
    val armX = size.width * 0.30f
    val thickY = size.height * 0.26f
    val thickX = size.width * 0.18f
    drawRect(color = cross, topLeft = Offset(0f, armY), size = Size(size.width, thickY))
    drawRect(color = cross, topLeft = Offset(armX, 0f), size = Size(thickX, size.height))
    if (inner != null) {
        val insetY = thickY * 0.34f
        val insetX = thickX * 0.34f
        drawRect(
            color = inner,
            topLeft = Offset(0f, armY + insetY),
            size = Size(size.width, thickY - insetY * 2)
        )
        drawRect(
            color = inner,
            topLeft = Offset(armX + insetX, 0f),
            size = Size(thickX - insetX * 2, size.height)
        )
    }
}

private fun DrawScope.drawCross(colors: List<Color>) {
    val base = colors.getOrElse(0) { Color.Red }
    val cross = colors.getOrElse(1) { Color.White }
    drawRect(color = base)
    val thickY = size.height * 0.24f
    val thickX = size.width * 0.16f
    drawRect(
        color = cross,
        topLeft = Offset(0f, size.height / 2 - thickY / 2),
        size = Size(size.width, thickY)
    )
    drawRect(
        color = cross,
        topLeft = Offset(size.width / 2 - thickX / 2, 0f),
        size = Size(thickX, size.height)
    )
}

/** 13 stripes plus the blue canton with a star grid. */
private fun DrawScope.drawUsFlag(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFB22234) }
    val white = colors.getOrElse(1) { Color.White }
    val blue = colors.getOrElse(2) { Color(0xFF3C3B6E) }
    val stripe = size.height / 13f
    drawRect(color = white)
    for (i in 0 until 13 step 2) {
        drawRect(color = red, topLeft = Offset(0f, stripe * i), size = Size(size.width, stripe + 0.4f))
    }
    val cantonW = size.width * 0.42f
    val cantonH = stripe * 7f
    drawRect(color = blue, size = Size(cantonW, cantonH))
    val starR = min(cantonW / 12f, cantonH / 12f)
    for (row in 0 until 5) {
        val odd = row % 2 == 1
        val count = if (odd) 5 else 6
        val stepX = cantonW / 6f
        val y = cantonH * (row + 0.5f) / 5f
        for (col in 0 until count) {
            val x = stepX * (col + if (odd) 1f else 0.5f)
            drawStar(Offset(x, y), starR, white)
        }
    }
}

private fun DrawScope.drawUnionJack(colors: List<Color>) {
    val blue = colors.getOrElse(0) { Color(0xFF012169) }
    val white = colors.getOrElse(1) { Color.White }
    val red = colors.getOrElse(2) { Color(0xFFC8102E) }
    drawRect(color = blue)
    val w = size.width
    val h = size.height
    // Diagonals: white saltire first, thin red on top.
    drawLine(white, Offset(0f, 0f), Offset(w, h), strokeWidth = h * 0.30f)
    drawLine(white, Offset(w, 0f), Offset(0f, h), strokeWidth = h * 0.30f)
    drawLine(red, Offset(0f, 0f), Offset(w, h), strokeWidth = h * 0.13f)
    drawLine(red, Offset(w, 0f), Offset(0f, h), strokeWidth = h * 0.13f)
    // Straight cross.
    drawRect(color = white, topLeft = Offset(0f, h * 0.34f), size = Size(w, h * 0.32f))
    drawRect(color = white, topLeft = Offset(w * 0.38f, 0f), size = Size(w * 0.24f, h))
    drawRect(color = red, topLeft = Offset(0f, h * 0.40f), size = Size(w, h * 0.20f))
    drawRect(color = red, topLeft = Offset(w * 0.43f, 0f), size = Size(w * 0.14f, h))
}

private fun DrawScope.drawDisc(colors: List<Color>) {
    val base = colors.getOrElse(0) { Color.White }
    val disc = colors.getOrElse(1) { Color.Red }
    drawRect(color = base)
    drawCircle(color = disc, radius = min(size.width, size.height) * 0.30f, center = Offset(size.width / 2, size.height / 2))
}

private fun DrawScope.drawChina(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFDE2910) }
    val yellow = colors.getOrElse(1) { Color(0xFFFFDE00) }
    drawRect(color = red)
    val big = min(size.width, size.height) * 0.20f
    val center = Offset(size.width * 0.20f, size.height * 0.30f)
    drawStar(center, big, yellow)
    val small = big * 0.38f
    listOf(
        Offset(size.width * 0.36f, size.height * 0.14f),
        Offset(size.width * 0.44f, size.height * 0.30f),
        Offset(size.width * 0.44f, size.height * 0.50f),
        Offset(size.width * 0.36f, size.height * 0.64f)
    ).forEach { drawStar(it, small, yellow) }
}

private fun DrawScope.drawCrescent(colors: List<Color>) {
    val base = colors.getOrElse(0) { Color(0xFFE30A17) }
    val white = colors.getOrElse(1) { Color.White }
    drawRect(color = base)
    val r = min(size.width, size.height) * 0.30f
    val center = Offset(size.width * 0.40f, size.height / 2)
    drawCircle(color = white, radius = r, center = center)
    drawCircle(color = base, radius = r * 0.80f, center = Offset(center.x + r * 0.32f, center.y))
    drawStar(Offset(size.width * 0.62f, size.height / 2), r * 0.52f, white)
}

private fun DrawScope.drawCanada(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFFF0000) }
    val white = colors.getOrElse(1) { Color.White }
    drawRect(color = white)
    drawRect(color = red, size = Size(size.width * 0.25f, size.height))
    drawRect(color = red, topLeft = Offset(size.width * 0.75f, 0f), size = Size(size.width * 0.25f, size.height))
    // Stylised leaf: a star reads better than a blob at 24dp.
    drawStar(Offset(size.width / 2, size.height / 2), min(size.width, size.height) * 0.26f, red)
}

private fun DrawScope.drawKorea(colors: List<Color>) {
    val white = colors.getOrElse(0) { Color.White }
    val red = colors.getOrElse(1) { Color(0xFFCD2E3A) }
    val blue = colors.getOrElse(2) { Color(0xFF003478) }
    drawRect(color = white)
    val r = min(size.width, size.height) * 0.28f
    val center = Offset(size.width / 2, size.height / 2)
    drawCircle(color = red, radius = r, center = center)
    val half = Path().apply {
        addArc(
            Rect(center.x - r, center.y - r, center.x + r, center.y + r),
            0f,
            180f
        )
        close()
    }
    clipPath(half) { drawCircle(color = blue, radius = r, center = center) }
    drawCircle(color = red, radius = r / 2, center = Offset(center.x - r / 2, center.y))
    drawCircle(color = blue, radius = r / 2, center = Offset(center.x + r / 2, center.y))
    // Four corner trigrams, reduced to short black bars.
    val bar = Size(r * 0.62f, r * 0.16f)
    listOf(
        Offset(size.width * 0.13f, size.height * 0.22f),
        Offset(size.width * 0.13f, size.height * 0.72f),
        Offset(size.width * 0.74f, size.height * 0.22f),
        Offset(size.width * 0.74f, size.height * 0.72f)
    ).forEach { p ->
        rotate(degrees = 30f, pivot = Offset(p.x + bar.width / 2, p.y)) {
            drawRect(color = Color(0xFF111111), topLeft = p, size = bar)
        }
    }
}

private fun DrawScope.drawAlbania(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFE41E20) }
    val black = colors.getOrElse(1) { Color.Black }
    drawRect(red)
    val cx = size.width / 2f
    val cy = size.height / 2f
    val eagle = Path().apply {
        moveTo(cx, cy - size.height * 0.28f)
        lineTo(cx - size.width * 0.10f, cy - size.height * 0.10f)
        lineTo(cx - size.width * 0.28f, cy - size.height * 0.22f)
        lineTo(cx - size.width * 0.20f, cy)
        lineTo(cx - size.width * 0.31f, cy + size.height * 0.12f)
        lineTo(cx - size.width * 0.08f, cy + size.height * 0.08f)
        lineTo(cx, cy + size.height * 0.30f)
        lineTo(cx + size.width * 0.08f, cy + size.height * 0.08f)
        lineTo(cx + size.width * 0.31f, cy + size.height * 0.12f)
        lineTo(cx + size.width * 0.20f, cy)
        lineTo(cx + size.width * 0.28f, cy - size.height * 0.22f)
        lineTo(cx + size.width * 0.10f, cy - size.height * 0.10f)
        close()
    }
    drawPath(eagle, black)
    drawCircle(black, size.height * 0.09f, Offset(cx - size.width * 0.055f, cy - size.height * 0.23f))
    drawCircle(black, size.height * 0.09f, Offset(cx + size.width * 0.055f, cy - size.height * 0.23f))
}

private fun DrawScope.drawHongKong(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFDE2910) }
    val white = colors.getOrElse(1) { Color.White }
    drawRect(red)
    val center = Offset(size.width / 2f, size.height / 2f)
    val orbit = size.height * 0.19f
    repeat(5) { index ->
        val angle = Math.toRadians(index * 72.0 - 90.0)
        val petal = Offset(
            center.x + cos(angle).toFloat() * orbit,
            center.y + sin(angle).toFloat() * orbit
        )
        drawCircle(white, size.height * 0.12f, petal)
        drawCircle(red, size.height * 0.045f, Offset(
            petal.x + cos(angle).toFloat() * size.height * 0.045f,
            petal.y + sin(angle).toFloat() * size.height * 0.045f
        ))
    }
}

private fun DrawScope.drawSpain(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFAA151B) }
    val yellow = colors.getOrElse(1) { Color(0xFFF1BF00) }
    drawRect(red)
    drawRect(yellow, Offset(0f, size.height * 0.25f), Size(size.width, size.height * 0.5f))
    val crestX = size.width * 0.32f
    drawRect(red, Offset(crestX, size.height * 0.37f), Size(size.width * 0.08f, size.height * 0.26f))
    drawCircle(Color(0xFFFFD45A), size.height * 0.055f, Offset(crestX + size.width * 0.04f, size.height * 0.34f))
}

private fun DrawScope.drawIndia(colors: List<Color>) {
    drawBands(colors.take(3), horizontal = true)
    val blue = colors.getOrElse(3) { Color(0xFF000080) }
    val center = Offset(size.width / 2f, size.height / 2f)
    val r = size.height * 0.135f
    drawCircle(blue, r, center)
    drawCircle(Color.White, r * 0.72f, center)
    repeat(8) { index ->
        val angle = Math.toRadians(index * 45.0)
        drawLine(
            blue,
            center,
            Offset(center.x + cos(angle).toFloat() * r, center.y + sin(angle).toFloat() * r),
            strokeWidth = (size.height * 0.018f).coerceAtLeast(0.5f)
        )
    }
}

private fun DrawScope.drawKazakhstan(colors: List<Color>) {
    val blue = colors.getOrElse(0) { Color(0xFF00AFCA) }
    val gold = colors.getOrElse(1) { Color(0xFFFFD700) }
    drawRect(blue)
    drawRect(gold, Offset(size.width * 0.07f, size.height * 0.10f), Size(size.width * 0.035f, size.height * 0.80f))
    val sun = Offset(size.width * 0.57f, size.height * 0.40f)
    val radius = size.height * 0.15f
    drawCircle(gold, radius, sun)
    repeat(12) { index ->
        val angle = Math.toRadians(index * 30.0)
        drawLine(
            gold,
            Offset(sun.x + cos(angle).toFloat() * radius, sun.y + sin(angle).toFloat() * radius),
            Offset(sun.x + cos(angle).toFloat() * radius * 1.35f, sun.y + sin(angle).toFloat() * radius * 1.35f),
            strokeWidth = (size.height * 0.025f).coerceAtLeast(0.5f)
        )
    }
    val eagle = Path().apply {
        moveTo(size.width * 0.39f, size.height * 0.67f)
        lineTo(size.width * 0.57f, size.height * 0.58f)
        lineTo(size.width * 0.75f, size.height * 0.67f)
        lineTo(size.width * 0.57f, size.height * 0.76f)
        close()
    }
    drawPath(eagle, gold)
}

private fun DrawScope.drawAustralia(colors: List<Color>) {
    val blue = colors.getOrElse(0) { Color(0xFF012169) }
    val white = colors.getOrElse(1) { Color.White }
    val red = colors.getOrElse(2) { Color(0xFFC8102E) }
    drawRect(blue)
    val cw = size.width * 0.46f
    val ch = size.height * 0.52f
    drawLine(white, Offset(0f, 0f), Offset(cw, ch), strokeWidth = ch * 0.24f)
    drawLine(white, Offset(cw, 0f), Offset(0f, ch), strokeWidth = ch * 0.24f)
    drawLine(red, Offset(0f, 0f), Offset(cw, ch), strokeWidth = ch * 0.09f)
    drawLine(red, Offset(cw, 0f), Offset(0f, ch), strokeWidth = ch * 0.09f)
    drawRect(white, Offset(0f, ch * 0.35f), Size(cw, ch * 0.30f))
    drawRect(white, Offset(cw * 0.38f, 0f), Size(cw * 0.24f, ch))
    drawRect(red, Offset(0f, ch * 0.42f), Size(cw, ch * 0.16f))
    drawRect(red, Offset(cw * 0.43f, 0f), Size(cw * 0.14f, ch))
    drawStar(Offset(size.width * 0.24f, size.height * 0.73f), size.height * 0.12f, white)
    listOf(
        Offset(size.width * 0.70f, size.height * 0.25f),
        Offset(size.width * 0.82f, size.height * 0.48f),
        Offset(size.width * 0.68f, size.height * 0.70f),
        Offset(size.width * 0.88f, size.height * 0.78f)
    ).forEach { drawStar(it, size.height * 0.065f, white) }
}

private fun DrawScope.drawMalaysia(colors: List<Color>) {
    val blue = colors.getOrElse(0) { Color(0xFF010066) }
    val red = colors.getOrElse(1) { Color(0xFFCC0001) }
    val white = colors.getOrElse(2) { Color.White }
    val yellow = colors.getOrElse(3) { Color(0xFFFFCC00) }
    val stripe = size.height / 14f
    drawRect(white)
    for (index in 0 until 14 step 2) {
        drawRect(red, Offset(0f, index * stripe), Size(size.width, stripe + 0.3f))
    }
    val canton = Size(size.width * 0.52f, stripe * 8f)
    drawRect(blue, size = canton)
    val center = Offset(canton.width * 0.42f, canton.height * 0.50f)
    val r = canton.height * 0.25f
    drawCircle(yellow, r, center)
    drawCircle(blue, r * 0.78f, Offset(center.x + r * 0.35f, center.y))
    drawStar(Offset(canton.width * 0.72f, canton.height * 0.50f), r * 0.52f, yellow)
}

private fun DrawScope.drawMacedonia(colors: List<Color>) {
    val red = colors.getOrElse(0) { Color(0xFFD20000) }
    val yellow = colors.getOrElse(1) { Color(0xFFFFE600) }
    drawRect(red)
    val center = Offset(size.width / 2f, size.height / 2f)
    val ends = listOf(
        Offset(0f, 0f), Offset(size.width / 2f, 0f), Offset(size.width, 0f),
        Offset(0f, size.height / 2f), Offset(size.width, size.height / 2f),
        Offset(0f, size.height), Offset(size.width / 2f, size.height), Offset(size.width, size.height)
    )
    ends.forEach { drawLine(yellow, center, it, strokeWidth = size.height * 0.10f) }
    drawCircle(red, size.height * 0.18f, center)
    drawCircle(yellow, size.height * 0.13f, center)
}

private fun DrawScope.drawEuropeanUnion(colors: List<Color>) {
    val blue = colors.getOrElse(0) { Color(0xFF003399) }
    val gold = colors.getOrElse(1) { Color(0xFFFFCC00) }
    drawRect(blue)
    val center = Offset(size.width / 2f, size.height / 2f)
    val orbit = size.height * 0.28f
    repeat(12) { index ->
        val angle = Math.toRadians(index * 30.0 - 90.0)
        drawStar(
            Offset(
                center.x + cos(angle).toFloat() * orbit,
                center.y + sin(angle).toFloat() * orbit
            ),
            size.height * 0.045f,
            gold
        )
    }
}

/** Five pointed star used by the US, Chinese and Turkish flags. */
private fun DrawScope.drawStar(center: Offset, radius: Float, color: Color) {
    if (radius <= 0.4f) {
        drawCircle(color = color, radius = radius.coerceAtLeast(0.5f), center = center)
        return
    }
    val path = Path()
    val inner = radius * 0.42f
    for (i in 0 until 10) {
        val r = if (i % 2 == 0) radius else inner
        val angle = Math.toRadians((i * 36.0) - 90.0)
        val x = center.x + (r * cos(angle)).toFloat()
        val y = center.y + (r * sin(angle)).toFloat()
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.close()
    drawPath(path, color)
}
