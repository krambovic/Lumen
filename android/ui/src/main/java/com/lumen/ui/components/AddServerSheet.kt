package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.theme.expressivePalette

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddServerSheet(onDismissRequest: () -> Unit, onImportClipboard: () -> Unit,
    onImportQr: () -> Unit, onImportFile: () -> Unit, onAddManualNode: () -> Unit) {
    val s = LocalStrings.current
    val art = expressivePalette()
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    fun run(action: () -> Unit) { onDismissRequest(); action() }
    ModalBottomSheet(onDismissRequest = onDismissRequest, sheetState = state,
        containerColor = MaterialTheme.colorScheme.background,
        shape = RoundedCornerShape(topStart = 36.dp, topEnd = 36.dp)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(s.add, style = MaterialTheme.typography.headlineLarge, modifier = Modifier.weight(1f))
                IconButton(onClick = onDismissRequest, modifier = Modifier.size(48.dp)
                    .clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.surfaceContainer)) {
                    Icon(Icons.Default.Close, s.close)
                }
            }
            Text(s.addServerOrSubscription, style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 4.dp, bottom = 20.dp))
            Row(Modifier.fillMaxWidth().height(172.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ExpressiveNavigationTile(s.importFromClipboard, "", Icons.Default.ContentPaste, "01",
                    art.stage, art.onStage, art.onStage, Motif.Pebble,
                    { run(onImportClipboard) }, Modifier.weight(1.12f).fillMaxHeight())
                ExpressiveNavigationTile(s.importQrCode, "", Icons.Default.QrCodeScanner, "02",
                    art.lime, art.onLime, art.onLime, Motif.Clover,
                    { run(onImportQr) }, Modifier.weight(.88f).fillMaxHeight())
            }
            Spacer(Modifier.height(8.dp))
            Row(Modifier.fillMaxWidth().height(144.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ExpressiveNavigationTile(s.importFromFile, "", Icons.Default.UploadFile, "03",
                    art.lavender, art.onLavender, art.onLavender, Motif.Fan,
                    { run(onImportFile) }, Modifier.weight(.88f).fillMaxHeight())
                ExpressiveNavigationTile(s.importManually, "", Icons.Default.Add, "04",
                    art.coral, art.onCoral, art.onCoral, Motif.Sun,
                    { run(onAddManualNode) }, Modifier.weight(1.12f).fillMaxHeight())
            }
        }
    }
}
