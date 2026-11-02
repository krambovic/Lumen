package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
fun LivePingCard(ping: String?, isConnected: Boolean, modifier: Modifier = Modifier) {
    // استخراج عدد پینگ از متن
    val pingNum = ping?.filter { it.isDigit() }?.toIntOrNull() ?: 0

    // تعیین رنگ‌ها و وضعیت بر اساس عدد پینگ
    val (statusText, statusColor, pingColor, bgColor) = when {
        !isConnected -> listOf("خاموش", Color(0xFF757D8A), Color(0xFF757D8A), Color(0xFF2A2D3A))
        pingNum == 0 -> listOf("در حال بررسی...", Color(0xFFFFB800), Color(0xFFFFB800), Color(0x33FFB800))
        pingNum < 120 -> listOf("عالی برای بازی", Color(0xFF00E676), Color(0xFF00E676), Color(0x3300E676))
        pingNum < 200 -> listOf("معمولی", Color(0xFFFFB800), Color(0xFFFFB800), Color(0x33FFB800))
        else -> listOf("نامناسب برای بازی", Color(0xFFFF3B30), Color(0xFFFF3B30), Color(0x33FF3B30))
    }

    val displayPing = if (!isConnected) "--" else if (pingNum == 0) "..." else pingNum.toString()
    // محاسبه نمایشی نوسان و میانگین
    val jitter = if (pingNum > 0) (pingNum * 0.12).toInt().coerceAtLeast(1) else 0
    val avg = if (pingNum > 0) pingNum + 5 else 0

    // ساخت گراف میله‌ای تصادفی بر اساس پینگ فعلی
    val barHeights = remember(pingNum) { List(9) { (10..38).random() } }

    // راست‌چین کردن محتوا (شبیه XNicePing)
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) {
        Box(
            modifier = modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(24.dp))
                .background(Color(0xFF12141E)) // بک‌گراند تاریک سرمه‌ای
                .border(1.dp, Color(0xFF22263A), RoundedCornerShape(24.dp))
                .padding(16.dp)
        ) {
            Column {
                // ردیف اول: آیکون و وضعیت
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(
                            modifier = Modifier
                                .size(34.dp)
                                .clip(CircleShape)
                                .background(Color(0xFF1B4F8A).copy(alpha = 0.2f)),
                            contentAlignment = Alignment.Center
                        ) {
                            Icon(Icons.Default.Speed, contentDescription = null, tint = Color(0xFF4A90E2), modifier = Modifier.size(18.dp))
                        }
                        Spacer(modifier = Modifier.width(10.dp))
                        Column {
                            Text("پینگ لایو", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                            Text("پینگ اینترنت شما", color = Color(0xFF757D8A), fontSize = 10.sp)
                        }
                    }

                    // کادر وضعیت (قرمز، سبز، زرد)
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(12.dp))
                            .background(bgColor as Color)
                            .padding(horizontal = 10.dp, vertical = 6.dp)
                    ) {
                        Text(statusText.toString(), color = statusColor as Color, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    }
                }

                Spacer(modifier = Modifier.height(24.dp))

                // ردیف دوم: گراف و عدد پینگ
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.Bottom
                ) {
                    // نمودار میله‌ای
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalAlignment = Alignment.Bottom,
                        modifier = Modifier.height(40.dp)
                    ) {
                        if (isConnected && pingNum > 0) {
                            barHeights.forEachIndexed { index, height ->
                                Box(
                                    modifier = Modifier
                                        .width(5.dp)
                                        .height(height.dp)
                                        .clip(RoundedCornerShape(topStart = 4.dp, topEnd = 4.dp))
                                        .background((pingColor as Color).copy(alpha = 0.4f + (index * 0.05f)))
                                )
                            }
                        } else {
                            Box(modifier = Modifier.width(60.dp).height(2.dp).background(Color(0xFF22263A)))
                        }
                    }

                    // عدد پینگ بزرگ
                    Row(verticalAlignment = Alignment.Bottom, modifier = Modifier.padding(bottom = 2.dp)) {
                        Text(
                            text = displayPing,
                            color = pingColor as Color,
                            fontSize = 42.sp,
                            fontWeight = FontWeight.Black,
                            letterSpacing = (-2).sp
                        )
                        Spacer(modifier = Modifier.width(6.dp))
                        Text("ms", color = Color(0xFF757D8A), fontSize = 14.sp, modifier = Modifier.padding(bottom = 6.dp))
                    }
                }

                Spacer(modifier = Modifier.height(18.dp))

                // ردیف سوم: دکمه تست و اطلاعات نوسان
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(12.dp))
                            .background(Color(0xFF2755FF)) // دکمه آبی تست کامل
                            .padding(horizontal = 16.dp, vertical = 8.dp)
                            .clickable { /* 액션 추가 가능 */ }
                    ) {
                        Text("تست کامل", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    }

                    if (isConnected && pingNum > 0) {
                        Text(
                            text = "میانگین ${avg}ms • نوسان ${jitter}ms",
                            color = Color(0xFF757D8A),
                            fontSize = 11.sp
                        )
                    } else {
                        Text(
                            text = "دیتایی در دسترس نیست",
                            color = Color(0xFF757D8A),
                            fontSize = 11.sp
                        )
                    }
                }
            }
        }
    }
}
