package com.lumen.app.profiles

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.screens.LumenProfileText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Key sharing stays in App settings; creating a file belongs to a config. */
@Composable
fun ProtectedProfilesDialog(onDismiss: () -> Unit) {
    val s = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    var ownKey by remember { mutableStateOf<String?>(null) }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        val key = withContext(Dispatchers.IO) { runCatching { ProtectedProfiles.recipientKey() } }
        key.onSuccess { ownKey = it }.onFailure { failed = true }
    }
    LumenDialog(title = LumenProfileText.KEY.inLanguage(s.localeCode), onDismissRequest = onDismiss,
        dismissText = s.close, onDismiss = onDismiss, content = {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Text(LumenProfileText.KEY_HELP.inLanguage(s.localeCode), style = MaterialTheme.typography.bodyMedium)
                if (failed) Text(LumenProfileText.ERROR.inLanguage(s.localeCode), color = MaterialTheme.colorScheme.error)
                FilledTonalButton(onClick = { ownKey?.let { clipboard.setText(AnnotatedString(it)) } },
                    enabled = ownKey != null, modifier = Modifier.fillMaxWidth()) {
                    if (ownKey == null && !failed) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    else Text(LumenProfileText.COPY_KEY.inLanguage(s.localeCode))
                }
            }
        })
}
