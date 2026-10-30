package com.lumen.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.focus.FocusDirection
import com.lumen.ui.theme.LumenMono
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Lock
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation

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
    var passwordVisible by rememberSaveable { mutableStateOf(false) }
    val valid = isValidSocks5Credential(draftUsername) &&
        isValidSocks5Credential(draftPassword)

    val focus = LocalFocusManager.current
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp))
        .background(MaterialTheme.colorScheme.surfaceContainerLow)
        .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(24.dp)).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Person, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(8.dp))
            Text(s.socks5Login, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
        }
        ExpressiveTextField(
            value = draftUsername,
            onValueChange = { value ->
                draftUsername = value
                if (isValidSocks5Credential(value) && isValidSocks5Credential(draftPassword)) {
                    onSave(value, draftPassword)
                }
            },

            singleLine = true,
            isError = !isValidSocks5Credential(draftUsername),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Next),
            keyboardActions = KeyboardActions(onNext = { focus.moveFocus(FocusDirection.Down) }),
            textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = LumenMono),
            shape = RoundedCornerShape(12.dp),
            colors = credentialFieldColors(),
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(Modifier.height(6.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Lock, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(8.dp))
            Text(s.socks5PasswordLabel, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
        }
        ExpressiveTextField(
            value = draftPassword,
            onValueChange = { value ->
                draftPassword = value
                if (isValidSocks5Credential(draftUsername) && isValidSocks5Credential(value)) {
                    onSave(draftUsername, value)
                }
            },

            singleLine = true,
            isError = !isValidSocks5Credential(draftPassword),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
            textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = LumenMono),
            visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
            trailingIcon = {
                IconButton(onClick = { passwordVisible = !passwordVisible }) {
                    Icon(if (passwordVisible) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                        contentDescription = if (passwordVisible) s.hidePassword else s.showPassword)
                }
            },
            shape = RoundedCornerShape(12.dp),
            colors = credentialFieldColors(),
            modifier = Modifier.fillMaxWidth()
        )
        if (!valid) {
            Text(
                s.socks5CredentialError,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall
            )
        }

    }
}

@Composable
private fun credentialFieldColors() = expressiveFieldColors().copy(
    unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh)
