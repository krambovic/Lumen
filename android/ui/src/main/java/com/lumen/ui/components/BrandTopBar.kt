package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.theme.expressivePalette

@Composable
fun BrandTopBar(
    onAddClick: (() -> Unit)? = null, modifier: Modifier = Modifier,
    title: String = "Lumen", horizontalPadding: Dp = 16.dp,
    actions: (@Composable () -> Unit)? = null
) {
    val art = expressivePalette()
    Row(modifier.fillMaxWidth().padding(horizontal = horizontalPadding, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, modifier = Modifier.weight(1f),
            color = MaterialTheme.colorScheme.onSurface,
            style = MaterialTheme.typography.headlineLarge.copy(
                fontSize = if (title == "Lumen") 44.sp else 36.sp,
                letterSpacing = if (LocalStrings.current.localeCode == "fa") 0.sp else (-1.6).sp, lineHeight = 44.sp),
            fontWeight = FontWeight.ExtraBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (actions != null) {
            actions()
        } else if (onAddClick != null) {
            Box(Modifier.expressiveButtonSemantics(LocalStrings.current.add, onAddClick).size(54.dp)
                .clip(expressiveShape(Motif.Clover)).background(art.lime)
                .clickable(role = Role.Button, onClick = onAddClick), contentAlignment = Alignment.Center) {
                Icon(Icons.Default.Add, null, tint = art.onLime, modifier = Modifier.size(28.dp))
            }
        } else {
            ExpressiveMotif(Motif.Sun, art.stage, Modifier.size(34.dp))
        }
    }
}
