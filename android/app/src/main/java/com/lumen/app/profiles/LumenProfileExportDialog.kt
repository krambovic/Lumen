package com.lumen.app.profiles

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.lumen.core.config.crypto.LumenProfileEnvelope
import com.lumen.core.config.crypto.LumenProfilePolicy
import com.lumen.core.config.crypto.ProtectedProfileCodec
import com.lumen.ui.components.LumenDialog
import com.lumen.ui.screens.ExpressiveTextField
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.screens.LumenProfileText
import com.lumen.ui.screens.LumenDropdown
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LumenProfileExportDialog(nodeId: String, profileName: String, loadSource: suspend () -> String, onDismiss: () -> Unit) {
    val s = LocalStrings.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val colors = MaterialTheme.colorScheme
    fun t(text: LumenProfileText) = text.inLanguage(s.localeCode)
    var source by remember(nodeId) { mutableStateOf<String?>(null) }
    var name by remember(nodeId) { mutableStateOf(profileName.take(120)) }
    var fileName by remember(nodeId) { mutableStateOf(profileName.map { if (it in "\\/:*?\"<>|" || it.isISOControl()) '_' else it }
        .joinToString("").take(100).ifBlank { "profile" }) }
    var locked by remember(nodeId) { mutableStateOf(true) }
    var delivery by remember(nodeId) { mutableStateOf(LumenProfileEnvelope.Delivery.RECIPIENTS) }
    var recipients by remember(nodeId) { mutableStateOf("") }
    var password by remember(nodeId) { mutableStateOf("") }
    var lifetime by remember(nodeId) { mutableStateOf("0") }
    var customHours by remember(nodeId) { mutableStateOf("") }
    var wifiOnly by remember(nodeId) { mutableStateOf(false) }
    var unmeteredOnly by remember(nodeId) { mutableStateOf(false) }
    var payload by remember(nodeId) { mutableStateOf<String?>(null) }
    var busy by remember(nodeId) { mutableStateOf(false) }
    var error by remember(nodeId) { mutableStateOf(false) }
    var showKey by remember { mutableStateOf(false) }
    if (showKey) ProtectedProfilesDialog { showKey = false }

    LaunchedEffect(nodeId) {
        try {
            source = withContext(Dispatchers.Default) { loadSource().takeIf(String::isNotBlank) }
            error = source == null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { error = true }
    }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/vnd.lumen.profile")) { uri ->
        val pending = payload
        if (uri == null || pending == null) { payload = null; busy = false }
        else scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    checkNotNull(context.contentResolver.openOutputStream(uri, "wt")).use {
                        it.write(pending.toByteArray(Charsets.UTF_8))
                    }
                }
                payload = null
                password = ""
                onDismiss()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { error = true }
            finally { payload = null; busy = false }
        }
    }
    val hours = if (lifetime == "custom") customHours.toLongOrNull() else lifetime.toLongOrNull()
    val validLifetime = !locked || (hours != null && hours in 0..8760 && (lifetime != "custom" || hours > 0))
    val ready = validLifetime && source != null && name.isNotBlank() && fileName.isNotBlank() && !busy && when (delivery) {
        LumenProfileEnvelope.Delivery.RECIPIENTS -> recipients.isNotBlank()
        LumenProfileEnvelope.Delivery.PASSWORD -> password.length >= 12
        LumenProfileEnvelope.Delivery.ANYONE -> true
    }
    Dialog(onDismissRequest = { if (!busy) onDismiss() }, properties = DialogProperties(
        usePlatformDefaultWidth = false, dismissOnBackPress = !busy, dismissOnClickOutside = false)) {
        Scaffold(modifier = Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding().imePadding(),
            containerColor = colors.background, contentWindowInsets = WindowInsets(0, 0, 0, 0),
            topBar = {
                Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(t(LumenProfileText.EXPORT), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
                    IconButton(onClick = { showKey = true }, enabled = !busy) { Icon(Icons.Default.Key, t(LumenProfileText.KEY)) }
                    IconButton(onClick = onDismiss, enabled = !busy) { Icon(Icons.Default.Close, s.close) }
                }
            }, bottomBar = {
                Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp)) {
                    if (error) Text(t(LumenProfileText.ERROR), color = colors.error,
                        style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(bottom = 10.dp))
                    Button(enabled = ready, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp), onClick = {
                        val config = source ?: return@Button
                        busy = true; error = false
                        scope.launch {
                            try {
                                val created = System.currentTimeMillis()
                                val policy = if (locked) LumenProfilePolicy(createdAtMs = created,
                                    expiresAtMs = hours?.takeIf { it > 0 }?.let { Math.addExact(created, it * 3_600_000) } ?: 0,
                                    wifiOnly = wifiOnly, unmeteredOnly = unmeteredOnly) else LumenProfilePolicy()
                                val profile = LumenProfileEnvelope.Profile(name.trim(), config, locked, policy)
                                val recipientText = recipients
                                val mode = delivery
                                val secret = password.toCharArray()
                                payload = withContext(Dispatchers.Default) {
                                    try {
                                        val keys = if (mode == LumenProfileEnvelope.Delivery.RECIPIENTS) {
                                            val lines = recipientText.lineSequence().map(String::trim).filter(String::isNotEmpty).distinct().toList()
                                            require(lines.size in 1..16)
                                            lines.map(ProtectedProfileCodec::publicKey)
                                        } else emptyList()
                                        LumenProfileEnvelope.seal(profile, mode, keys, secret)
                                    } finally { secret.fill('\u0000') }
                                }
                                save.launch(fileName.trim().removeSuffix(".lumen") + ".lumen")
                            } catch (cancelled: CancellationException) { throw cancelled }
                            catch (_: Exception) { payload = null; busy = false; error = true }
                        }
                    }) {
                        if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = colors.onPrimary)
                        else Icon(Icons.Default.FileDownload, null, Modifier.size(20.dp))
                        Spacer(Modifier.width(8.dp)); Text(t(LumenProfileText.SAVE))
                    }
                }
            }) { padding ->
            Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Surface(shape = RoundedCornerShape(30.dp, 12.dp, 30.dp, 12.dp), color = colors.primaryContainer) {
                    Row(Modifier.fillMaxWidth().padding(20.dp), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        Icon(Icons.Default.Description, null, Modifier.size(36.dp), tint = colors.onPrimaryContainer)
                        Column(Modifier.weight(1f)) {
                            Text(profileName, style = MaterialTheme.typography.titleLarge, maxLines = 2, overflow = TextOverflow.Ellipsis,
                                color = colors.onPrimaryContainer)
                            Text(".lumen", style = MaterialTheme.typography.labelLarge, color = colors.onPrimaryContainer)
                        }
                    }
                }
                ExpressiveTextField(name, { name = it.take(120) }, Modifier.fillMaxWidth(),
                    label = { Text(t(LumenProfileText.NAME)) }, singleLine = true, enabled = !busy)
                ExpressiveTextField(fileName, { fileName = it.filterNot { c -> c in "\\/:*?\"<>|" || c.isISOControl() }.take(100) },
                    Modifier.fillMaxWidth(), label = { Text(t(LumenProfileText.FILE)) },
                    trailingIcon = { Text(".lumen", Modifier.padding(end = 12.dp), style = MaterialTheme.typography.labelMedium) },
                    singleLine = true, enabled = !busy)
                Surface(color = colors.secondaryContainer, shape = RoundedCornerShape(24.dp)) {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(if (locked) Icons.Default.Lock else Icons.Default.LockOpen, null, tint = colors.onSecondaryContainer)
                            Text(t(LumenProfileText.LOCK), Modifier.weight(1f).padding(horizontal = 12.dp),
                                style = MaterialTheme.typography.titleSmall, color = colors.onSecondaryContainer)
                            Switch(checked = locked, onCheckedChange = { locked = it }, enabled = !busy)
                        }
                        Text(t(LumenProfileText.LOCK_HELP), style = MaterialTheme.typography.bodySmall, color = colors.onSecondaryContainer)
                    }
                }
                Text(t(LumenProfileText.ACCESS), style = MaterialTheme.typography.titleMedium)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    LumenProfileEnvelope.Delivery.entries.forEachIndexed { index, value ->
                        SegmentedButton(selected = delivery == value, onClick = { delivery = value; error = false }, enabled = !busy,
                            shape = SegmentedButtonDefaults.itemShape(index, 3), modifier = Modifier.weight(1f), icon = {}, label = {
                                Text(t(when (value) { LumenProfileEnvelope.Delivery.RECIPIENTS -> LumenProfileText.RECIPIENTS
                                    LumenProfileEnvelope.Delivery.PASSWORD -> LumenProfileText.PASSWORD
                                    LumenProfileEnvelope.Delivery.ANYONE -> LumenProfileText.ANYONE }), maxLines = 1)
                            })
                    }
                }
                when (delivery) {
                    LumenProfileEnvelope.Delivery.RECIPIENTS -> ExpressiveTextField(recipients,
                        { recipients = it.take(24_000) }, Modifier.fillMaxWidth(), label = { Text(t(LumenProfileText.KEYS)) },
                        minLines = 2, maxLines = 4, enabled = !busy)
                    LumenProfileEnvelope.Delivery.PASSWORD -> ExpressiveTextField(password,
                        { password = it.take(1024) }, Modifier.fillMaxWidth(), label = { Text(t(LumenProfileText.PASSWORD)) },
                        singleLine = true, enabled = !busy, visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Password))
                    LumenProfileEnvelope.Delivery.ANYONE -> Unit
                }
                Text(t(when (delivery) { LumenProfileEnvelope.Delivery.RECIPIENTS -> LumenProfileText.KEYS_HELP
                    LumenProfileEnvelope.Delivery.PASSWORD -> LumenProfileText.PASSWORD_HELP
                    LumenProfileEnvelope.Delivery.ANYONE -> LumenProfileText.ANYONE_HELP }),
                    style = MaterialTheme.typography.bodySmall, color = colors.onSurfaceVariant)
                Text(t(LumenProfileText.LIMITS), style = MaterialTheme.typography.titleMedium)
                Text(t(LumenProfileText.LIMITS_HELP), style = MaterialTheme.typography.bodySmall,
                    color = colors.onSurfaceVariant)
                if (locked) {
                    LumenDropdown(t(LumenProfileText.LIFETIME), listOf("0", "1", "6", "24", "72", "168", "720", "custom"),
                        lifetime, { if (!busy) lifetime = it }, Modifier.fillMaxWidth(), optionLabel = {
                            when (it) { "0" -> t(LumenProfileText.FOREVER); "custom" -> t(LumenProfileText.CUSTOM)
                                else -> when (s.localeCode) { "ru" -> "$it ч"; "fa" -> "$it ساعت"; "zh" -> "$it 小时"; else -> "$it h" } }
                        })
                    if (lifetime == "custom") ExpressiveTextField(customHours,
                        { customHours = it.filter(Char::isDigit).take(4) }, Modifier.fillMaxWidth(),
                        singleLine = true, enabled = !busy, label = { Text("${t(LumenProfileText.HOURS)} · 1–8760") },
                        keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(t(LumenProfileText.WIFI_ONLY), Modifier.weight(1f))
                        Switch(wifiOnly, { wifiOnly = it }, enabled = !busy)
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(t(LumenProfileText.UNMETERED_ONLY), Modifier.weight(1f))
                        Switch(unmeteredOnly, { unmeteredOnly = it }, enabled = !busy)
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
        }
    }
}

@Composable
fun ProfilePasswordDialog(busy: Boolean, failed: Boolean, onUnlock: (String) -> Unit, onDismiss: () -> Unit) {
    val s = LocalStrings.current
    var password by remember { mutableStateOf("") }
    LumenDialog(title = LumenProfileText.UNLOCK.inLanguage(s.localeCode),
        onDismissRequest = { if (!busy) onDismiss() }, busy = busy,
        dismissText = s.cancel, onDismiss = onDismiss, content = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                ExpressiveTextField(password, { password = it.take(1024) }, Modifier.fillMaxWidth(),
                    label = { Text(LumenProfileText.PASSWORD.inLanguage(s.localeCode)) },
                    enabled = !busy, singleLine = true, isError = failed, visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Password))
                if (failed) Text(LumenProfileText.BAD_PASSWORD.inLanguage(s.localeCode), color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall)
                Button(onClick = { onUnlock(password); password = "" }, enabled = !busy && password.isNotEmpty(),
                    modifier = Modifier.fillMaxWidth()) { Text(LumenProfileText.IMPORT.inLanguage(s.localeCode)) }
            }
        })
}
