package com.lumen.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/**
 * Full node editor supporting all protocols from the desktop app:
 * VLESS / VMess / Trojan / Shadowsocks / Hysteria2 / TUIC / WireGuard /
 * AmneziaWG / OpenVPN / SOCKS / HTTP / Auto (url-test).
 */
@Composable
fun NodeEditorModal(
    initial: NodeDraft,
    onDismiss: () -> Unit,
    onSave: (NodeDraft) -> Unit
) {
    var draft by remember(initial) { mutableStateOf(initial) }
    val s = LocalStrings.current

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.94f)
                .padding(horizontal = 10.dp),
            shape = RoundedCornerShape(18.dp),
            color = MaterialTheme.colorScheme.surface
        ) {
            Column(Modifier.padding(16.dp)) {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        if (initial.id == null) s.addNode else s.editNode,
                        style = MaterialTheme.typography.titleMedium
                    )
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Filled.Close, contentDescription = s.close, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                Column(
                    Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState())
                ) {
                    Field(s.nodeName, draft.name) { draft = draft.copy(name = it) }
                    LumenDropdown(
                        label = s.protocolField,
                        options = SUPPORTED_PROTOCOLS,
                        selected = draft.protocol,
                        onSelected = { draft = draft.copy(protocol = it) },
                        optionLabel = { protocolLabel(it) }
                    )
                    Spacer(Modifier.height(8.dp))
                    when (draft.protocol) {
                        "auto" -> Text(
                            s.autoNodeDescriptionLabel,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        "openvpn" -> OpenVpnFields(draft = draft, onChange = { draft = it })
                        "wireguard", "awg" -> WireGuardFields(
                            draft = draft,
                            isAwg = draft.protocol == "awg",
                            onChange = { draft = it }
                        )
                        else -> UriProtocolFields(draft = draft, onChange = { draft = it })
                    }
                    Spacer(Modifier.height(12.dp))
                }
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End
                ) {
                    TextButton(onClick = onDismiss) { Text(s.cancel) }
                    Spacer(Modifier.width(8.dp))
                    OutlinedButton(
                        onClick = { onSave(draft) },
                        enabled = isDraftValid(draft)
                    ) {
                        Text(s.saveAction)
                    }
                }
            }
        }
    }
}

private fun protocolLabel(p: String): String = when (p) {
    "vless" -> "VLESS"
    "vmess" -> "VMess"
    "trojan" -> "Trojan"
    "ss" -> "Shadowsocks"
    "hysteria2" -> "Hysteria2"
    "tuic" -> "TUIC v5"
    "wireguard" -> "WireGuard"
    "awg" -> "AmneziaWG"
    "openvpn" -> "OpenVPN"
    "socks" -> "SOCKS5"
    "http" -> "HTTP proxy"
    "auto" -> "Auto (url-test)"
    else -> p
}

private fun isDraftValid(d: NodeDraft): Boolean = when (d.protocol) {
    "auto" -> d.name.isNotBlank()
    "openvpn" -> d.server.isNotBlank() && d.port.toIntOrNull() != null && d.ovpnCa.isNotBlank()
    else -> d.server.isNotBlank() && d.port.toIntOrNull() != null
}

@Composable
private fun Field(
    label: String,
    value: String,
    keyboardType: KeyboardType = KeyboardType.Text,
    singleLine: Boolean = true,
    minLines: Int = 1,
    onChange: (String) -> Unit
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = singleLine,
        minLines = minLines,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
    )
}

@Composable
private fun ServerPortRow(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current
    Row(Modifier.fillMaxWidth()) {
        OutlinedTextField(
            value = draft.server,
            onValueChange = { onChange(draft.copy(server = it)) },
            label = { Text(s.serverLabel) },
            singleLine = true,
            modifier = Modifier
                .weight(0.65f)
                .padding(vertical = 4.dp)
        )
        Spacer(Modifier.width(8.dp))
        OutlinedTextField(
            value = draft.port,
            onValueChange = { onChange(draft.copy(port = it)) },
            label = { Text(s.portLabel) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier
                .weight(0.35f)
                .padding(vertical = 4.dp)
        )
    }
}

@Composable
private fun ToggleField(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        LumenSwitch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun TransportFields(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current
    LumenDropdown(
        label = s.transportLabel,
        options = NETWORK_TRANSPORTS,
        selected = draft.network,
        onSelected = { onChange(draft.copy(network = it)) }
    )
    Spacer(Modifier.height(4.dp))
    if (draft.network in listOf("ws", "xhttp", "http")) {
        Field(s.pathLabel, draft.path) { onChange(draft.copy(path = it)) }
        Field(s.hostHeaderLabel, draft.host) { onChange(draft.copy(host = it)) }
    }
    if (draft.network == "grpc") {
        Field(s.grpcServiceNameLabel, draft.serviceName) { onChange(draft.copy(serviceName = it)) }
    }
}

@Composable
private fun SecurityFields(
    draft: NodeDraft,
    onChange: (NodeDraft) -> Unit,
    allowReality: Boolean = true
) {
    val s = LocalStrings.current
    val options = if (allowReality) SECURITY_OPTIONS else listOf("none", "tls")
    LumenDropdown(
        label = s.securityLabel,
        options = options,
        selected = draft.security,
        onSelected = { onChange(draft.copy(security = it)) }
    )
    Spacer(Modifier.height(4.dp))
    if (draft.security != "none") {
        Field(s.sniLabel, draft.sni) { onChange(draft.copy(sni = it)) }
        Field(s.alpnLabel, draft.alpn) { onChange(draft.copy(alpn = it)) }
        Field(s.utlsFingerprintLabel, draft.fingerprint) { onChange(draft.copy(fingerprint = it)) }
        ToggleField(s.allowInsecureTlsLabel, draft.insecure) { onChange(draft.copy(insecure = it)) }
    }
    if (draft.security == "reality") {
        Field(s.realityPublicKeyLabel, draft.publicKey) { onChange(draft.copy(publicKey = it)) }
        Field(s.realityShortIdLabel, draft.shortId) { onChange(draft.copy(shortId = it)) }
    }
}

@Composable
private fun UriProtocolFields(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current
    ServerPortRow(draft, onChange)
    when (draft.protocol) {
        "vless" -> {
            Field(s.uuidLabel, draft.secret) { onChange(draft.copy(secret = it)) }
            LumenDropdown(
                label = s.flowLabel,
                options = listOf("", "xtls-rprx-vision"),
                selected = draft.flow,
                onSelected = { onChange(draft.copy(flow = it)) },
                optionLabel = { it.ifEmpty { s.noneOption } }
            )
            Spacer(Modifier.height(4.dp))
            TransportFields(draft, onChange)
            SecurityFields(draft, onChange)
        }
        "vmess" -> {
            Field(s.uuidLabel, draft.secret) { onChange(draft.copy(secret = it)) }
            TransportFields(draft, onChange)
            SecurityFields(draft, onChange, allowReality = false)
        }
        "trojan" -> {
            Field(s.password, draft.secret) { onChange(draft.copy(secret = it)) }
            TransportFields(draft, onChange)
            SecurityFields(draft, onChange)
        }
        "ss" -> {
            LumenDropdown(
                label = s.encryptionMethodLabel,
                options = SS_METHODS,
                selected = draft.method,
                onSelected = { onChange(draft.copy(method = it)) }
            )
            Field(s.password, draft.secret) { onChange(draft.copy(secret = it)) }
        }
        "hysteria2" -> {
            Field(s.passwordAuthLabel, draft.secret) { onChange(draft.copy(secret = it)) }
            Field(s.sniLabel, draft.sni) { onChange(draft.copy(sni = it)) }
            Field(s.obfsLabel, draft.obfs) { onChange(draft.copy(obfs = it)) }
            Field(s.obfsPasswordLabel, draft.obfsPassword) { onChange(draft.copy(obfsPassword = it)) }
            ToggleField(s.allowInsecureTlsLabel, draft.insecure) { onChange(draft.copy(insecure = it)) }
        }
        "tuic" -> {
            Field(s.uuidPasswordLabel, draft.secret) { onChange(draft.copy(secret = it)) }
            Field(s.sniLabel, draft.sni) { onChange(draft.copy(sni = it)) }
            Field(s.alpnLabel, draft.alpn) { onChange(draft.copy(alpn = it)) }
            LumenDropdown(
                label = s.congestionControlLabel,
                options = CONGESTION_OPTIONS,
                selected = draft.congestionControl,
                onSelected = { onChange(draft.copy(congestionControl = it)) }
            )
            Spacer(Modifier.height(4.dp))
            ToggleField(s.allowInsecureTlsLabel, draft.insecure) { onChange(draft.copy(insecure = it)) }
        }
        "socks", "http" -> {
            Field(s.userPasswordOptionalLabel, draft.secret) { onChange(draft.copy(secret = it)) }
        }
    }
}

@Composable
private fun WireGuardFields(
    draft: NodeDraft,
    isAwg: Boolean,
    onChange: (NodeDraft) -> Unit
) {
    val s = LocalStrings.current
    ServerPortRow(draft, onChange)
    Field(s.privateKeyLabel, draft.secret) { onChange(draft.copy(secret = it)) }
    Field(s.peerPublicKeyLabel, draft.publicKey) { onChange(draft.copy(publicKey = it)) }
    Field(s.wgAddressLabel, draft.address) { onChange(draft.copy(address = it)) }
    Field(s.presharedKeyLabel, draft.presharedKey) { onChange(draft.copy(presharedKey = it)) }
    Field(s.allowedIpsLabel, draft.allowedIps) { onChange(draft.copy(allowedIps = it)) }
    Field(s.reservedLabel, draft.reserved) { onChange(draft.copy(reserved = it)) }
    if (!isAwg) {
        // wg -> awg with the Amnezia defaults, same as desktop Lumen / wgtunnel.
        OutlinedButton(
            onClick = { onChange(amneziaCompatible(draft)) },
            modifier = Modifier.fillMaxWidth().padding(top = 8.dp)
        ) {
            Text(s.amneziaCompatible)
        }
        Text(
            s.amneziaCompatibleDesc,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 4.dp)
        )
    }
    if (isAwg) {
        SectionHeader(s.awgJunkParamsLabel)
        Row(Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = draft.jc,
                onValueChange = { onChange(draft.copy(jc = it)) },
                label = { Text("Jc") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.jmin,
                onValueChange = { onChange(draft.copy(jmin = it)) },
                label = { Text("Jmin") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.jmax,
                onValueChange = { onChange(draft.copy(jmax = it)) },
                label = { Text("Jmax") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
        }
        Row(Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = draft.s1,
                onValueChange = { onChange(draft.copy(s1 = it)) },
                label = { Text("S1") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.s2,
                onValueChange = { onChange(draft.copy(s2 = it)) },
                label = { Text("S2") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.s3,
                onValueChange = { onChange(draft.copy(s3 = it)) },
                label = { Text("S3") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.s4,
                onValueChange = { onChange(draft.copy(s4 = it)) },
                label = { Text("S4") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
        }
    }
}

/**
 * Structured OpenVPN editor, same field set as the desktop Lumen node editor.
 * The .ovpn text stays available as an import shortcut that fills the fields.
 */
@Composable
private fun OpenVpnFields(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current
    var pasteOpen by remember { mutableStateOf(draft.ovpnCa.isBlank()) }
    var pasteText by remember { mutableStateOf(draft.rawConfig) }

    ServerPortRow(draft, onChange)
    LumenDropdown(
        label = "Protocol",
        options = OPENVPN_PROTOCOLS,
        selected = draft.ovpnProto.ifBlank { "udp" },
        onSelected = { onChange(draft.copy(ovpnProto = it)) }
    )
    Spacer(Modifier.height(4.dp))
    Field("Additional remotes (host port per line)", draft.ovpnExtraRemotes, singleLine = false, minLines = 2) {
        onChange(draft.copy(ovpnExtraRemotes = it))
    }

    SectionHeader("Encryption")
    LumenDropdown(
        label = "Cipher",
        options = OPENVPN_CIPHERS,
        selected = draft.ovpnCipher,
        onSelected = { onChange(draft.copy(ovpnCipher = it)) },
        optionLabel = { it.ifEmpty { s.noneOption } }
    )
    Spacer(Modifier.height(4.dp))
    LumenDropdown(
        label = "Auth digest",
        options = OPENVPN_AUTH_DIGESTS,
        selected = draft.ovpnAuth,
        onSelected = { onChange(draft.copy(ovpnAuth = it)) },
        optionLabel = { it.ifEmpty { s.noneOption } }
    )
    Spacer(Modifier.height(4.dp))

    SectionHeader("Credentials")
    Field("Username", draft.ovpnUsername) { onChange(draft.copy(ovpnUsername = it)) }
    Field("Password", draft.ovpnPassword) { onChange(draft.copy(ovpnPassword = it)) }

    SectionHeader("Certificates")
    Field("CA certificate", draft.ovpnCa, singleLine = false, minLines = 4) { onChange(draft.copy(ovpnCa = it)) }
    Field("Client certificate", draft.ovpnCert, singleLine = false, minLines = 3) { onChange(draft.copy(ovpnCert = it)) }
    Field("Private key", draft.ovpnKey, singleLine = false, minLines = 3) { onChange(draft.copy(ovpnKey = it)) }
    Field("tls-crypt key", draft.ovpnTlsCrypt, singleLine = false, minLines = 3) { onChange(draft.copy(ovpnTlsCrypt = it)) }
    ToggleField("tls-crypt-v2", draft.ovpnTlsCryptV2) { onChange(draft.copy(ovpnTlsCryptV2 = it)) }
    Field("tls-auth key", draft.ovpnTlsAuth, singleLine = false, minLines = 3) { onChange(draft.copy(ovpnTlsAuth = it)) }
    LumenDropdown(
        label = "Key direction",
        options = OPENVPN_KEY_DIRECTIONS,
        selected = draft.ovpnKeyDirection,
        onSelected = { onChange(draft.copy(ovpnKeyDirection = it)) },
        optionLabel = { it.ifEmpty { s.noneOption } }
    )
    Spacer(Modifier.height(4.dp))
    Field("TLS cipher suites", draft.ovpnTlsCipherSuites) { onChange(draft.copy(ovpnTlsCipherSuites = it)) }
    Field("verify-x509-name", draft.ovpnVerifyX509Name) { onChange(draft.copy(ovpnVerifyX509Name = it)) }
    LumenDropdown(
        label = "verify-x509-name mode",
        options = OPENVPN_X509_MODES,
        selected = draft.ovpnVerifyX509Mode,
        onSelected = { onChange(draft.copy(ovpnVerifyX509Mode = it)) },
        optionLabel = { it.ifEmpty { s.noneOption } }
    )
    Spacer(Modifier.height(4.dp))

    SectionHeader("Connection")
    Row(Modifier.fillMaxWidth()) {
        OutlinedTextField(
            value = draft.ovpnReconnectDelay,
            onValueChange = { onChange(draft.copy(ovpnReconnectDelay = it)) },
            label = { Text("Reconnect, s") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.weight(1f).padding(vertical = 4.dp)
        )
        Spacer(Modifier.width(6.dp))
        OutlinedTextField(
            value = draft.ovpnPingInterval,
            onValueChange = { onChange(draft.copy(ovpnPingInterval = it)) },
            label = { Text("Ping, s") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.weight(1f).padding(vertical = 4.dp)
        )
        Spacer(Modifier.width(6.dp))
        OutlinedTextField(
            value = draft.ovpnPingRestart,
            onValueChange = { onChange(draft.copy(ovpnPingRestart = it)) },
            label = { Text("Ping restart, s") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.weight(1f).padding(vertical = 4.dp)
        )
    }
    Field("DNS servers (one per line)", draft.ovpnDns, singleLine = false, minLines = 2) {
        onChange(draft.copy(ovpnDns = it))
    }

    SectionHeader(s.openvpnConfigLabel)
    OutlinedButton(
        onClick = { pasteOpen = !pasteOpen },
        modifier = Modifier.fillMaxWidth().padding(top = 4.dp)
    ) {
        Text(if (pasteOpen) s.close else s.importAction)
    }
    if (pasteOpen) {
        Field(s.openvpnConfigLabel, pasteText, singleLine = false, minLines = 8) { pasteText = it }
        OutlinedButton(
            onClick = { onChange(openVpnDraftFromProfile(draft, pasteText)) },
            enabled = pasteText.isNotBlank(),
            modifier = Modifier.fillMaxWidth().padding(top = 4.dp)
        ) {
            Text(s.importAction)
        }
    }
}

/** Amnezia defaults (Jc/Jmin/Jmax + S1/S2), same values as desktop Lumen and wgtunnel. */
private fun amneziaCompatible(d: NodeDraft): NodeDraft = d.copy(
    protocol = "awg",
    jc = d.jc.ifBlank { "4" },
    jmin = d.jmin.ifBlank { "40" },
    jmax = d.jmax.ifBlank { "70" },
    s1 = d.s1.ifBlank { "0" },
    s2 = d.s2.ifBlank { "0" }
)
