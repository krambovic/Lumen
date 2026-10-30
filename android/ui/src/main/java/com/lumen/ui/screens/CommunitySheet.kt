package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextDirection
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.ExpressiveAction
import com.lumen.ui.components.LocalQrEncoder

internal const val SUPPORT_USDT_TRC20 = "TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s"

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun CommunitySheet(onDismiss: () -> Unit, onOpenLink: (String) -> Unit) {
    val s = LocalStrings.current
    var support by remember { mutableStateOf(false) }
    var crypto by remember { mutableStateOf(false) }
    var copied by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val encoder = LocalQrEncoder.current
    val qr by produceState<ImageBitmap?>(null, crypto, encoder) {
        if (crypto) value = withContext(Dispatchers.Default) { encoder(SUPPORT_USDT_TRC20) }
    }
    ModalBottomSheet(onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(if (support) s.supportProject else s.community, style = MaterialTheme.typography.headlineLarge)
            if (support) {
                TextButton({ if (crypto) crypto = false else support = false }) { Text(s.back) }
                if (!crypto) {
                    ExpressiveAction("Donation Alerts", { onOpenLink("https://www.donationalerts.com/r/studiobebraedition") }, Modifier.fillMaxWidth(), Icons.Default.Favorite)
                    ExpressiveAction("${s.cryptoSupport} · USDT", { crypto = true }, Modifier.fillMaxWidth(), Icons.Default.CurrencyBitcoin)
                } else {
                    Text("USDT · TRC20", style = MaterialTheme.typography.titleLarge)
                    qr?.let { Image(it, s.walletAddress, Modifier.align(Alignment.CenterHorizontally).size(224.dp).background(Color.White).padding(8.dp)) }
                    Text(s.walletAddress, style = MaterialTheme.typography.labelLarge)
                    SelectionContainer { Text(SUPPORT_USDT_TRC20,
                        style = MaterialTheme.typography.bodyMedium.copy(textDirection = TextDirection.Ltr)) }
                    FilledTonalButton({
                        (context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager)
                            .setPrimaryClip(android.content.ClipData.newPlainText("USDT TRC20", SUPPORT_USDT_TRC20))
                        copied = true
                    }, Modifier.fillMaxWidth()) { Text(if (copied) s.copied else s.copyAction) }
                }
            } else {
            ExpressiveAction("GitHub", { onOpenLink("https://github.com/krambovic/Lumen") }, Modifier.fillMaxWidth(), Icons.Default.Code)
            ExpressiveAction(s.supportProject, { support = true }, Modifier.fillMaxWidth(), Icons.Default.Favorite)
            ExpressiveAction("Telegram · @lumenkvn", { onOpenLink("https://t.me/lumenkvn") }, Modifier.fillMaxWidth(), Icons.AutoMirrored.Filled.Send)
            }
        }
    }
}
