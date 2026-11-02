package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
fun LivePingCard(ping: String?, isConnected: Boolean, modifier: Modifier = Modifier) {
    val statusText = when {
        !isConnected -> "خاموش"
        ping != null -> ping
        else -> "در حال بررسی..."
    }
    
    val statusColor = when {
        !isConnected -> Color(0xFF757D8A)
        ping != null -> Color(0xFF00E676) // سبز نئونی
        else -> Color(0xFFFFB800) // زرد
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .background(Color(0xFF10121A), shape = RoundedCornerShape(18.dp))
            .border(1.dp, Color(0xFF1B1E2B), RoundedCornerShape(18.dp))
            .padding(16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column {
                Text(text = "Live Ping", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(4.dp))
                Text(text = "پینگ لحظه‌ای سرور", color = Color(0xFF6B7280), fontSize = 11.sp)
            }
            
            Text(
                text = statusText,
                color = statusColor,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
                style = androidx.compose.material3.MaterialTheme.typography.labelSmall.copy(
                    textDirection = androidx.compose.ui.text.style.TextDirection.Ltr
                )
            )
        }
    }
}
