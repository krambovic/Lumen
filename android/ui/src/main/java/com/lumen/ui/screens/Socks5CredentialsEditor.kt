package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp

fun isValidSocks5Credential(value: String): Boolean =
    value.length in 1..255 && value.none { it.code < 32 || it.code == 127 } &&
        Charsets.UTF_8.newEncoder().canEncode(value) && value.toByteArray(Charsets.UTF_8).size <= 255

@Composable
internal fun Socks5CredentialsEditor(
    username: String,
    password: String,
    onSave: (String, String) -> Unit,
    onReset: () -> Unit
) {
    val s = LocalStrings.current
    val clipboard = LocalClipboardManager.current
    var editing by remember { mutableStateOf(false) }
    var draftUsername by remember { mutableStateOf("") }
    var draftPassword by remember { mutableStateOf("") }
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("${s.socks5Login}: $username", modifier = Modifier.weight(1f))
            TextButton(onClick = { clipboard.setText(AnnotatedString(username)) }) { Text(s.socks5Copy) }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("${s.socks5PasswordLabel}: ••••••••", modifier = Modifier.weight(1f))
            TextButton(onClick = { clipboard.setText(AnnotatedString(password)) }) { Text(s.socks5Copy) }
        }
        Row {
            TextButton(onClick = {
                draftUsername = username
                draftPassword = password
                editing = true
            }) { Text(s.edit) }
            TextButton(onClick = onReset) { Text(s.socks5Reset) }
        }
        Text(s.socks5ReconnectNote, style = MaterialTheme.typography.bodySmall)
    }
    if (editing) {
        val valid = isValidSocks5Credential(draftUsername) && isValidSocks5Credential(draftPassword)
        AlertDialog(
            onDismissRequest = { editing = false },
            title = { Text(s.socks5Auth) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = draftUsername, onValueChange = { draftUsername = it },
                        label = { Text(s.socks5Login) }, singleLine = true,
                        isError = !isValidSocks5Credential(draftUsername)
                    )
                    OutlinedTextField(
                        value = draftPassword, onValueChange = { draftPassword = it },
                        label = { Text(s.socks5PasswordLabel) }, singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                        isError = !isValidSocks5Credential(draftPassword)
                    )
                    if (!valid) Text(s.socks5CredentialError, color = MaterialTheme.colorScheme.error)
                }
            },
            confirmButton = {
                TextButton(enabled = valid, onClick = {
                    onSave(draftUsername, draftPassword)
                    editing = false
                }) { Text(s.saveAction) }
            },
            dismissButton = { TextButton(onClick = { editing = false }) { Text(s.cancel) } }
        )
    }
}
