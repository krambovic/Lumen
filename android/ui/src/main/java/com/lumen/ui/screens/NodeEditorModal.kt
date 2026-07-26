package com.lumen.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Warning
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/**
 * Full node editor supporting all protocols from the desktop app:
 * VLESS / VMess / Trojan / Shadowsocks / Hysteria2 / TUIC / WireGuard /
 * AmneziaWG / OpenVPN / SOCKS / HTTP.
 */
@Composable
fun NodeEditorModal(
    initial: NodeDraft,
    onDismiss: () -> Unit,
    onSave: (NodeDraft) -> Unit
) {
    var draft by remember(initial) { mutableStateOf(initial) }
    val s = LocalStrings.current
    val tick = rememberHapticTick()

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
                        "masque" -> MasqueFields(draft = draft, onChange = { draft = it })
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
                        onClick = {
                            // A saved node is the "server added" moment the user feels.
                            tick(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                            onSave(draft)
                        },
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
    "masque" -> "MASQUE"
    "openvpn" -> "OpenVPN"
    "socks" -> "SOCKS5"
    "http" -> "HTTP"
    "auto" -> "Auto (url-test)"
    else -> p
}

private fun isDraftValid(d: NodeDraft): Boolean = when (d.protocol) {
    "auto" -> d.name.isNotBlank()
    // The MASQUE link carries a profile id instead of a host:port pair.
    "masque" -> d.server.isNotBlank()
    "openvpn" -> d.server.isNotBlank() && d.port.toIntOrNull() != null && d.ovpnCa.isNotBlank() &&
        !openVpnCredentialsMissing(d)
    else -> d.server.isNotBlank() && d.port.toIntOrNull() != null
}

/**
 * True when the profile authenticates the user interactively: either it asks for it
 * (`auth-user-pass`), or it carries no client certificate at all, which leaves the
 * username/password as the only credential the server can check. A certificate-only
 * profile has <cert> and <key> and legitimately needs neither, so it is not flagged.
 */
fun openVpnRequiresCredentials(d: NodeDraft): Boolean {
    if (d.protocol != "openvpn") return false
    val declared = d.rawConfig.lineSequence().any { line ->
        val token = line.trim().trimStart('-').lowercase()
        token == "auth-user-pass" || token == "<auth-user-pass>" ||
            token.startsWith("auth-user-pass ") || token.startsWith("auth-user-pass\t")
    }
    return declared || (d.ovpnCert.isBlank() && d.ovpnKey.isBlank())
}

/**
 * The node would be saved looking ready while the core has nothing to authenticate with:
 * the tunnel then fails with an obscure handshake error instead of a missing password.
 */
fun openVpnCredentialsMissing(d: NodeDraft): Boolean =
    openVpnRequiresCredentials(d) && (d.ovpnUsername.isBlank() || d.ovpnPassword.isBlank())

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

/** Inline blocker: the field above is missing something the node cannot be saved without. */
@Composable
private fun EditorWarning(text: String) {
    val shape = RoundedCornerShape(12.dp)
    val accent = MaterialTheme.colorScheme.error
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .clip(shape)
            .background(accent.copy(alpha = 0.12f))
            .border(1.dp, accent.copy(alpha = 0.55f), shape)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            Icons.Filled.Warning,
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(18.dp)
        )
        Spacer(Modifier.width(10.dp))
        Text(
            text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface
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

/**
 * MASQUE nodes are addressed by a WARP profile id and an auth token, so they
 * have no host/port row.
 */
@Composable
private fun MasqueFields(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current
    Field(s.masqueProfileIdLabel, draft.server) { onChange(draft.copy(server = it)) }
    Field(s.masqueAuthTokenLabel, draft.secret) { onChange(draft.copy(secret = it)) }
    Field(s.sniLabel, draft.sni) { onChange(draft.copy(sni = it)) }
    ToggleField(s.allowInsecureTlsLabel, draft.insecure) { onChange(draft.copy(insecure = it)) }
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
    Field(s.wgMtuLabel, draft.mtu, keyboardType = KeyboardType.Number) { onChange(draft.copy(mtu = it)) }
    Field(s.wgDnsLabel, draft.dns) { onChange(draft.copy(dns = it)) }
    Field(s.wgKeepaliveLabel, draft.persistentKeepalive, keyboardType = KeyboardType.Number) {
        onChange(draft.copy(persistentKeepalive = it))
    }
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
        // awg -> wg: drop the obfuscation parameters and go back to plain WireGuard.
        OutlinedButton(
            onClick = { onChange(plainWireGuard(draft)) },
            modifier = Modifier.fillMaxWidth().padding(top = 8.dp)
        ) {
            Text(s.disableAmnezia)
        }
        Text(
            s.disableAmneziaDesc,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 4.dp)
        )
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
        SectionHeader(s.awgHeadersLabel)
        Row(Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = draft.h1,
                onValueChange = { onChange(draft.copy(h1 = it)) },
                label = { Text("H1") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.h2,
                onValueChange = { onChange(draft.copy(h2 = it)) },
                label = { Text("H2") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.h3,
                onValueChange = { onChange(draft.copy(h3 = it)) },
                label = { Text("H3") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.h4,
                onValueChange = { onChange(draft.copy(h4 = it)) },
                label = { Text("H4") },
                singleLine = true,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
        }
    }
}

/**
 * Structured OpenVPN editor, same field set as the desktop Lumen node editor.
 */
@Composable
private fun OpenVpnFields(draft: NodeDraft, onChange: (NodeDraft) -> Unit) {
    val s = LocalStrings.current

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
    val credentialsRequired = openVpnRequiresCredentials(draft)
    Field("Username", draft.ovpnUsername) { onChange(draft.copy(ovpnUsername = it)) }
    Field("Password", draft.ovpnPassword) { onChange(draft.copy(ovpnPassword = it)) }
    if (credentialsRequired && openVpnCredentialsMissing(draft)) {
        EditorWarning(s.ovpnCredentialsRequired)
    } else if (!credentialsRequired) {
        Text(
            s.ovpnCertificateOnlyHint,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }

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

    SectionHeader(s.useProxyLabel)
    LumenDropdown(
        label = s.useProxyLabel,
        options = OPENVPN_PROXY_TYPES,
        selected = draft.ovpnProxyType,
        onSelected = { onChange(draft.copy(ovpnProxyType = it)) },
        optionLabel = { openVpnProxyLabel(it) }
    )
    Spacer(Modifier.height(4.dp))
    if (draft.ovpnProxyType.isNotBlank()) {
        Row(Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = draft.ovpnProxyServer,
                onValueChange = { onChange(draft.copy(ovpnProxyServer = it)) },
                label = { Text(s.proxyServerLabel) },
                singleLine = true,
                modifier = Modifier.weight(2f).padding(vertical = 4.dp)
            )
            Spacer(Modifier.width(6.dp))
            OutlinedTextField(
                value = draft.ovpnProxyPort,
                onValueChange = { onChange(draft.copy(ovpnProxyPort = it.filter { c -> c.isDigit() })) },
                label = { Text(s.portLabel) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.weight(1f).padding(vertical = 4.dp)
            )
        }
        if (!isObfsProxy(draft.ovpnProxyType)) {
            Field(s.proxyUsernameLabel, draft.ovpnProxyUsername) {
                onChange(draft.copy(ovpnProxyUsername = it))
            }
            Field(s.proxyPasswordLabel, draft.ovpnProxyPassword) {
                onChange(draft.copy(ovpnProxyPassword = it))
            }
        } else {
            Text(
                s.obfsProxyHint,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }

}

/** Drops every obfuscation parameter and turns the node back into plain WireGuard. */
private fun plainWireGuard(d: NodeDraft): NodeDraft = d.copy(
    protocol = "wireguard",
    jc = "", jmin = "", jmax = "",
    s1 = "", s2 = "", s3 = "", s4 = "",
    h1 = "", h2 = "", h3 = "", h4 = "",
    i1 = "", i2 = "", i3 = "", i4 = "", i5 = "",
    j1 = "", j2 = "", j3 = "", itime = ""
)

/** Amnezia defaults (Jc/Jmin/Jmax + S1/S2), same values as desktop Lumen and wgtunnel. */
private fun amneziaCompatible(d: NodeDraft): NodeDraft = d.copy(
    protocol = "awg",
    jc = d.jc.ifBlank { "4" },
    jmin = d.jmin.ifBlank { "40" },
    jmax = d.jmax.ifBlank { "70" },
    s1 = d.s1.ifBlank { "0" },
    s2 = d.s2.ifBlank { "0" }
)
