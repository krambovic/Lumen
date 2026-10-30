package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material3.*
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.theme.expressivePalette

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ConnectionStage(state: ConnectionState, serverName: String?, onToggle: () -> Unit, compact: Boolean = false) {
    val art = expressivePalette()
    val s = LocalStrings.current
    val connected = state == ConnectionState.Connected
    val connecting = state == ConnectionState.Connecting
    val failed = state == ConnectionState.Error
    val decorativeRotation = rememberDecorativeRotation(36000)
    Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 36.dp, topEnd = 36.dp,
        bottomEnd = 36.dp, bottomStart = 12.dp)).background(art.stage)) {
        ExpressiveMotif(Motif.Flower, art.onStage.copy(alpha = .1f),
            Modifier.align(Alignment.TopEnd).offset(x = 34.dp, y = (-30).dp).size(156.dp)
                .graphicsLayer { rotationZ = -12f + decorativeRotation.value })
        Column(Modifier.fillMaxWidth().padding(18.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween) {
                Text(s.connection.uppercase(), style = MaterialTheme.typography.labelSmall, color = art.onStage.copy(alpha = .7f))
                Box(Modifier.size(10.dp).clip(RoundedCornerShape(50))
                    .background(if (connected) art.lime else if (failed) art.coral else art.onStage.copy(alpha = .4f)))
            }
            Spacer(Modifier.height(12.dp))
            Row(Modifier.height(72.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Box(Modifier.fillMaxWidth().height(64.dp), contentAlignment = Alignment.CenterStart) {
                        AnimatedContent(state, transitionSpec = {
                            expressivePageEnter().togetherWith(expressivePageExit()).using(null)
                        }, label = "connection_status") { status ->
                            Text(when (status) {
                                ConnectionState.Connected -> s.centerConnected
                                ConnectionState.Connecting -> s.centerConnecting
                                ConnectionState.Error -> s.centerError
                                else -> s.off
                            }, style = MaterialTheme.typography.displaySmall.copy(fontSize = 28.sp, lineHeight = 32.sp),
                                color = art.onStage, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
                val silhouette = connectionMorphShape(state)
                Box(Modifier.semantics {
                    contentDescription = if (connecting) s.cancel else if (connected) s.centerDisconnect else s.centerConnect
                }
                    .size(60.dp).testTag(LumenMotionTags.ConnectionIndicator).graphicsLayer { shape = silhouette; clip = true }
                    .background(art.lime).clickable(role = Role.Button, onClick = onToggle), contentAlignment = Alignment.Center) {
                    AnimatedContent(connecting, modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center,
                        transitionSpec = {
                        (androidx.compose.animation.fadeIn(androidx.compose.animation.core.spring(stiffness = 700f)) +
                            androidx.compose.animation.scaleIn(androidx.compose.animation.core.spring(stiffness = 450f), initialScale = .85f))
                            .togetherWith(androidx.compose.animation.fadeOut(androidx.compose.animation.core.spring(stiffness = 1000f)))
                            .using(null)
                    }, label = "connection_indicator") { loading ->
                        Box(Modifier.size(44.dp).testTag(LumenMotionTags.ConnectionGlyph), contentAlignment = Alignment.Center) {
                            if (loading) ContainedLoadingIndicator(Modifier.size(44.dp),
                                containerColor = art.lime, indicatorColor = art.onLime)
                            else Icon(Icons.Default.PowerSettingsNew, null, tint = art.onLime, modifier = Modifier.size(30.dp))
                        }
                    }
                }
            }
            if (!compact) {
                Spacer(Modifier.height(16.dp))
                ExpressiveAction(if (connected) s.centerDisconnect else if (connecting) s.centerConnecting else s.centerConnect,
                    onToggle, Modifier.fillMaxWidth().height(56.dp), icon = Icons.Default.PowerSettingsNew,
                    container = art.lime, content = art.onLime)
            }
        }
    }
}
