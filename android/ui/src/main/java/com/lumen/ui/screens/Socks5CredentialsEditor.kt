package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.compose.runtime.saveable.rememberSaveable

fun isValidSocks5Credential(value: String): Boolean =
    value.length in 1..255 && value.none { it.code < 32 || it.code == 127 } &&
        Charsets.UTF_8.newEncoder().canEncode(value) && value.toByteArray(Charsets.UTF_8).size <= 255

@Composable
internal fun Socks5CredentialsEditor(
    username: String,
    password: String,
    onSave: (String, String) -> Unit
) {
    val s = LocalStrings.current
    var draftUsername by rememberSaveable(username) { mutableStateOf(username) }
    var draftPassword by rememberSaveable(password) { mutableStateOf(password) }
    val valid = isValidSocks5Credential(draftUsername) &&
        isValidSocks5Credential(draftPassword)

    Column(
        Modifier.fillMaxWidth().padding(vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        OutlinedTextField(
            value = draftUsername,
            onValueChange = { value ->
                draftUsername = value
                if (isValidSocks5Credential(value) && isValidSocks5Credential(draftPassword)) {
                    onSave(value, draftPassword)
                }
            },
            label = { Text(s.socks5Login) },
            singleLine = true,
            isError = !isValidSocks5Credential(draftUsername),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text),
            modifier = Modifier.fillMaxWidth()
        )
        OutlinedTextField(
            value = draftPassword,
            onValueChange = { value ->
                draftPassword = value
                if (isValidSocks5Credential(draftUsername) && isValidSocks5Credential(value)) {
                    onSave(draftUsername, value)
                }
            },
            label = { Text(s.socks5PasswordLabel) },
            singleLine = true,
            isError = !isValidSocks5Credential(draftPassword),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text),
            modifier = Modifier.fillMaxWidth()
        )
        if (!valid) {
            Text(
                s.socks5CredentialError,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall
            )
        }
        Text(s.socks5ReconnectNote, style = MaterialTheme.typography.bodySmall)
    }
}
