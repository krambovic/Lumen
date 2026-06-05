from __future__ import annotations

from copy import deepcopy
import json
from typing import Any
from urllib.parse import quote

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import QDialog, QFormLayout, QHBoxLayout, QScrollArea, QVBoxLayout, QWidget
from qfluentwidgets import BodyLabel, ComboBox, EditableComboBox, LineEdit, PrimaryPushButton, PushButton, SubtitleLabel, isDarkTheme

from ..models import Node


_FINGERPRINTS = ("", "chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized")
_NETWORKS = ("tcp", "raw", "ws", "grpc", "http", "h2", "xhttp", "kcp", "quic")
_SECURITY = ("none", "tls", "reality")
_FLOWS = ("", "xtls-rprx-vision", "xtls-rprx-vision-udp443")


class NodeEditDialog(QDialog):
    def __init__(self, node: Node, existing_groups: list[str], parent=None):
        super().__init__(parent)
        self._node = node
        self._outbound = deepcopy(node.outbound)

        self.setWindowTitle("Редактирование сервера")
        self.setModal(True)
        self.setMinimumSize(640, 620)
        bg = "#2b2b2b" if isDarkTheme() else "#f3f3f3"
        self.setStyleSheet(f"NodeEditDialog {{ background-color: {bg}; }}")

        root = QVBoxLayout(self)
        root.setContentsMargins(20, 20, 20, 20)
        root.setSpacing(10)

        root.addWidget(SubtitleLabel("Редактирование сервера", self))

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        content = QWidget(scroll)
        form = QFormLayout(content)
        form.setContentsMargins(0, 0, 0, 0)
        form.setHorizontalSpacing(14)
        form.setVerticalSpacing(10)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignLeft)
        scroll.setWidget(content)
        root.addWidget(scroll, 1)

        self.name_edit = LineEdit(self)
        self.name_edit.setText(node.name)
        form.addRow(BodyLabel("Псевдоним", self), self.name_edit)

        self.group_combo = EditableComboBox(self)
        for group in existing_groups:
            self.group_combo.addItem(group)
        self.group_combo.setText(node.group)
        form.addRow(BodyLabel("Группа", self), self.group_combo)

        self.tags_edit = LineEdit(self)
        self.tags_edit.setText(", ".join(node.tags))
        form.addRow(BodyLabel("Теги", self), self.tags_edit)

        self.address_edit = LineEdit(self)
        self.address_edit.setText(node.server)
        form.addRow(BodyLabel("Адрес", self), self.address_edit)

        self.port_edit = LineEdit(self)
        self.port_edit.setText(str(node.port or ""))
        form.addRow(BodyLabel("Порт", self), self.port_edit)

        protocol = str(self._outbound.get("protocol") or node.scheme or "").lower()
        form.addRow(BodyLabel("Протокол", self), BodyLabel(protocol.upper() or "?", self))

        self.uuid_edit = LineEdit(self)
        self.encryption_edit = LineEdit(self)
        self.flow_combo = _combo(self, _FLOWS)
        self.network_combo = _combo(self, _NETWORKS)
        self.raw_header_combo = _combo(self, ("none", "http"))
        self.security_combo = _combo(self, _SECURITY)
        self.sni_edit = LineEdit(self)
        self.fp_combo = _combo(self, _FINGERPRINTS, editable=True)
        self.public_key_edit = LineEdit(self)
        self.short_id_edit = LineEdit(self)
        self.spider_x_edit = LineEdit(self)
        self.pqv_edit = LineEdit(self)
        self.finalmask_edit = LineEdit(self)

        form.addRow(BodyLabel("UUID / id", self), self.uuid_edit)
        form.addRow(BodyLabel("Flow", self), self.flow_combo)
        form.addRow(BodyLabel("Шифрование", self), self.encryption_edit)
        form.addRow(BodyLabel("Транспорт", self), self.network_combo)
        form.addRow(BodyLabel("Raw camouflage", self), self.raw_header_combo)
        form.addRow(BodyLabel("TLS", self), self.security_combo)
        form.addRow(BodyLabel("SNI", self), self.sni_edit)
        form.addRow(BodyLabel("Fingerprint", self), self.fp_combo)
        form.addRow(BodyLabel("Public key", self), self.public_key_edit)
        form.addRow(BodyLabel("ShortId", self), self.short_id_edit)
        form.addRow(BodyLabel("SpiderX", self), self.spider_x_edit)
        form.addRow(BodyLabel("Mldsa65Verify", self), self.pqv_edit)
        form.addRow(BodyLabel("Finalmask", self), self.finalmask_edit)

        self._load_outbound_fields()
        self.security_combo.currentIndexChanged.connect(self._sync_security_fields)
        self.network_combo.currentIndexChanged.connect(self._sync_network_fields)
        self._sync_security_fields()
        self._sync_network_fields()

        row = QHBoxLayout()
        row.addStretch(1)
        self.cancel_btn = PushButton("Отмена", self)
        self.save_btn = PrimaryPushButton("Сохранить", self)
        row.addWidget(self.cancel_btn)
        row.addWidget(self.save_btn)
        root.addLayout(row)

        self.cancel_btn.clicked.connect(self.reject)
        self.save_btn.clicked.connect(self.accept)

    def _load_outbound_fields(self) -> None:
        outbound = self._outbound
        protocol = str(outbound.get("protocol") or "").lower()
        settings = outbound.get("settings") if isinstance(outbound.get("settings"), dict) else {}
        if protocol in {"vless", "vmess"}:
            vnext = _first_dict(settings.get("vnext"))
            user = _first_dict(vnext.get("users"))
            self.uuid_edit.setText(str(user.get("id") or ""))
            self.encryption_edit.setText(str(user.get("encryption") or "none"))
            _set_combo_text(self.flow_combo, str(user.get("flow") or ""))

        stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
        _set_combo_text(self.network_combo, str(stream.get("network") or "tcp"))
        raw = stream.get("rawSettings") if isinstance(stream.get("rawSettings"), dict) else {}
        header = raw.get("header") if isinstance(raw.get("header"), dict) else {}
        _set_combo_text(self.raw_header_combo, str(header.get("type") or "none"))
        _set_combo_text(self.security_combo, str(stream.get("security") or "none"))

        tls = stream.get("tlsSettings") if isinstance(stream.get("tlsSettings"), dict) else {}
        reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
        active_security = str(stream.get("security") or "").lower()
        security_payload = reality if active_security == "reality" else tls
        self.sni_edit.setText(str(security_payload.get("serverName") or ""))
        _set_combo_text(self.fp_combo, str(security_payload.get("fingerprint") or ""))
        self.public_key_edit.setText(str(reality.get("publicKey") or ""))
        self.short_id_edit.setText(str(reality.get("shortId") or ""))
        self.spider_x_edit.setText(str(reality.get("spiderX") or ""))
        self.pqv_edit.setText(str(reality.get("mldsa65Verify") or ""))
        finalmask = stream.get("finalmask")
        if finalmask not in (None, ""):
            if isinstance(finalmask, str):
                self.finalmask_edit.setText(finalmask)
            else:
                self.finalmask_edit.setText(json.dumps(finalmask, ensure_ascii=False, separators=(",", ":")))

    def _sync_security_fields(self) -> None:
        security = self.security_combo.currentText().strip().lower()
        is_tls = security in {"tls", "reality"}
        is_reality = security == "reality"
        for widget in (self.sni_edit, self.fp_combo):
            widget.setEnabled(is_tls)
        for widget in (self.public_key_edit, self.short_id_edit, self.spider_x_edit, self.pqv_edit):
            widget.setEnabled(is_reality)

    def _sync_network_fields(self) -> None:
        self.raw_header_combo.setEnabled(self.network_combo.currentText().strip().lower() == "raw")

    def get_updated_fields(self) -> dict:
        raw_tags = self.tags_edit.text().strip()
        tags = [tag.strip() for tag in raw_tags.split(",") if tag.strip()] if raw_tags else []
        outbound = deepcopy(self._outbound)
        protocol = str(outbound.get("protocol") or self._node.scheme or "").lower()
        server = self.address_edit.text().strip()
        port = _safe_port(self.port_edit.text(), self._node.port)

        settings = outbound.setdefault("settings", {})
        if isinstance(settings, dict) and protocol in {"vless", "vmess"}:
            vnext = _ensure_first_dict(settings, "vnext")
            vnext["address"] = server
            vnext["port"] = port
            user = _ensure_first_dict(vnext, "users")
            user["id"] = self.uuid_edit.text().strip()
            if protocol == "vless":
                user["encryption"] = self.encryption_edit.text().strip() or "none"
                _set_or_remove(user, "flow", self.flow_combo.currentText().strip())

        stream = outbound.setdefault("streamSettings", {})
        if isinstance(stream, dict):
            network = self.network_combo.currentText().strip().lower() or "tcp"
            stream["network"] = network
            security = self.security_combo.currentText().strip().lower() or "none"
            stream["security"] = security
            if network == "raw":
                header_type = self.raw_header_combo.currentText().strip() or "none"
                raw_settings = stream.setdefault("rawSettings", {})
                if isinstance(raw_settings, dict):
                    header = raw_settings.setdefault("header", {})
                    if isinstance(header, dict):
                        header["type"] = header_type
            elif "rawSettings" in stream:
                stream.pop("rawSettings", None)

            if security == "reality":
                stream.pop("tlsSettings", None)
                reality = stream.setdefault("realitySettings", {})
                if isinstance(reality, dict):
                    _set_or_remove(reality, "serverName", self.sni_edit.text().strip())
                    _set_or_remove(reality, "fingerprint", self.fp_combo.currentText().strip())
                    _set_or_remove(reality, "publicKey", self.public_key_edit.text().strip())
                    _set_or_remove(reality, "shortId", self.short_id_edit.text().strip())
                    _set_or_remove(reality, "spiderX", self.spider_x_edit.text().strip())
                    _set_or_remove(reality, "mldsa65Verify", self.pqv_edit.text().strip())
            elif security == "tls":
                stream.pop("realitySettings", None)
                tls = stream.setdefault("tlsSettings", {})
                if isinstance(tls, dict):
                    _set_or_remove(tls, "serverName", self.sni_edit.text().strip())
                    _set_or_remove(tls, "fingerprint", self.fp_combo.currentText().strip())
            else:
                stream.pop("tlsSettings", None)
                stream.pop("realitySettings", None)

            finalmask = self.finalmask_edit.text().strip()
            if finalmask:
                try:
                    stream["finalmask"] = json.loads(finalmask)
                except Exception:
                    stream["finalmask"] = finalmask
            else:
                stream.pop("finalmask", None)

        link = _build_vless_link(self.name_edit.text().strip(), server, port, outbound) if protocol == "vless" else self._node.link
        return {
            "name": self.name_edit.text().strip(),
            "group": self.group_combo.text().strip() or "Default",
            "tags": tags,
            "server": server,
            "port": port,
            "outbound": outbound,
            "link": link,
        }


def _combo(parent: QWidget, values: tuple[str, ...], *, editable: bool = False) -> ComboBox:
    combo = EditableComboBox(parent) if editable else ComboBox(parent)
    for value in values:
        combo.addItem(value)
    return combo


def _set_combo_text(combo: ComboBox, value: str) -> None:
    value = str(value or "")
    index = combo.findText(value)
    if index >= 0:
        combo.setCurrentIndex(index)
    elif hasattr(combo, "setText"):
        combo.setText(value)


def _first_dict(value: Any) -> dict[str, Any]:
    if isinstance(value, list) and value and isinstance(value[0], dict):
        return value[0]
    return {}


def _ensure_first_dict(parent: dict[str, Any], key: str) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, list):
        value = []
        parent[key] = value
    if not value or not isinstance(value[0], dict):
        value.insert(0, {})
    return value[0]


def _set_or_remove(parent: dict[str, Any], key: str, value: str) -> None:
    if value:
        parent[key] = value
    else:
        parent.pop(key, None)


def _safe_port(value: str, fallback: int) -> int:
    try:
        port = int(str(value).strip())
    except ValueError:
        return int(fallback or 0)
    return max(0, min(65535, port))


def _build_vless_link(name: str, server: str, port: int, outbound: dict[str, Any]) -> str:
    settings = outbound.get("settings") if isinstance(outbound.get("settings"), dict) else {}
    vnext = _first_dict(settings.get("vnext"))
    user = _first_dict(vnext.get("users"))
    user_id = str(user.get("id") or "")
    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    params: dict[str, str] = {
        "encryption": str(user.get("encryption") or "none"),
        "type": "tcp" if str(stream.get("network") or "tcp").lower() == "raw" else str(stream.get("network") or "tcp"),
    }
    _set_param(params, "flow", str(user.get("flow") or ""))
    security = str(stream.get("security") or "none").lower()
    _set_param(params, "security", security if security != "none" else "")
    tls = stream.get("tlsSettings") if isinstance(stream.get("tlsSettings"), dict) else {}
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    payload = reality if security == "reality" else tls
    _set_param(params, "sni", str(payload.get("serverName") or ""))
    _set_param(params, "fp", str(payload.get("fingerprint") or ""))
    network = str(stream.get("network") or "tcp").lower()
    if network == "xhttp":
        xhttp = stream.get("xhttpSettings") if isinstance(stream.get("xhttpSettings"), dict) else {}
        _set_param(params, "path", str(xhttp.get("path") or ""))
        _set_param(params, "host", str(xhttp.get("host") or ""))
        _set_param(params, "mode", str(xhttp.get("mode") or ""))
        extra = xhttp.get("extra")
        if extra not in (None, ""):
            _set_param(
                params,
                "extra",
                extra if isinstance(extra, str) else json.dumps(extra, ensure_ascii=False, separators=(",", ":")),
            )
    if security == "reality":
        _set_param(params, "pbk", str(reality.get("publicKey") or ""))
        _set_param(params, "sid", str(reality.get("shortId") or ""))
        _set_param(params, "spx", str(reality.get("spiderX") or ""))
        _set_param(params, "pqv", str(reality.get("mldsa65Verify") or ""))
    finalmask = stream.get("finalmask")
    if finalmask not in (None, ""):
        _set_param(params, "fm", finalmask if isinstance(finalmask, str) else json.dumps(finalmask, ensure_ascii=False, separators=(",", ":")))

    query = "&".join(f"{quote(key)}={quote(value, safe='')}" for key, value in params.items() if value != "")
    fragment = quote(name, safe="")
    return f"vless://{quote(user_id, safe='')}@{server}:{port}?{query}#{fragment}"


def _set_param(params: dict[str, str], key: str, value: str) -> None:
    value = str(value or "")
    if value:
        params[key] = value
