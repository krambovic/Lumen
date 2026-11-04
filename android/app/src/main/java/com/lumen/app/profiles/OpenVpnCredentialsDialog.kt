package com.lumen.app.profiles

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.lumen.app.vm.MainViewModel
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.screens.ExpressiveTextField
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.screens.LumenProfileText

@Composable
fun OpenVpnCredentialsDialog(request: MainViewModel.OpenVpnCredentialRequest,
    onConfirm: (String, String, String) -> Unit, onDismiss: () -> Unit) {
    val s = LocalStrings.current
    fun t(text: LumenProfileText) = text.inLanguage(s.localeCode)
    var username by remember(request.nodeId) { mutableStateOf("") }
    var password by remember(request.nodeId) { mutableStateOf("") }
    var keyPassword by remember(request.nodeId) { mutableStateOf("") }
    val missing = request.missing
    val ready = (!missing.username || username.isNotBlank()) && (!missing.password || password.isNotBlank()) &&
        (!missing.keyPassword || keyPassword.isNotBlank())
    LumenDialog(title = t(LumenProfileText.CREDENTIALS), onDismissRequest = onDismiss,
        dismissText = s.cancel, onDismiss = onDismiss, content = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(request.name, style = MaterialTheme.typography.titleSmall)
                if (missing.username) ExpressiveTextField(username, { username = it.take(1024) }, Modifier.fillMaxWidth(),
                    singleLine = true, label = { Text(t(LumenProfileText.USERNAME)) })
                if (missing.password) ExpressiveTextField(password, { password = it.take(1024) }, Modifier.fillMaxWidth(),
                    singleLine = true, label = { Text(t(LumenProfileText.PASSWORD)) }, visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Password))
                if (missing.keyPassword) ExpressiveTextField(keyPassword, { keyPassword = it.take(1024) }, Modifier.fillMaxWidth(),
                    singleLine = true, label = { Text(s.ovpnKeyPasswordLabel) }, visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Password))
                Button(onClick = { onConfirm(username, password, keyPassword); password = ""; keyPassword = "" },
                    enabled = ready, modifier = Modifier.fillMaxWidth()) { Text(t(LumenProfileText.CONNECT)) }
            }
        })
}
