from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime


_ANSI_RE = re.compile(r"(?:\x1b|\u009b)\[[0-?]*[ -/]*[@-~]")
_CONTROL_RE = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_SINGBOX_PREFIX_RE = re.compile(
    r"^(?:[+-]\d{4}\s+)?\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\s+"
)
_TRACE_ID_RE = re.compile(r"^\s*\[\d+\]\s*(?:\d+(?:\.\d+)?(?:ms|s)\s*)?")
_PORT_RE = re.compile(r"(?:порт|port)\s+(\d{2,5})", re.IGNORECASE)


@dataclass(frozen=True, slots=True)
class LogEntry:
    timestamp: str
    level: str
    source: str
    message: str
    details: str
    action_id: str = ""
    action_label: str = ""
    search_text: str = ""


def clean_log_text(value: str) -> str:
    text = _ANSI_RE.sub("", str(value or ""))
    text = text.replace("\ufffd", "").replace("\u001b", "")
    text = _CONTROL_RE.sub("", text)
    return " ".join(text.strip().split())


def _split_source(text: str) -> tuple[str, str]:
    match = re.match(r"^\[([^\]]+)]\s*(.*)$", text)
    if not match:
        return "Приложение", text
    tag = match.group(1).strip().lower()
    body = match.group(2).strip()
    if tag in {"xray", "xray-error"}:
        return "Xray", body
    if tag in {"singbox", "sing-box", "singbox-error"}:
        return "sing-box", body
    if tag in {"tun", "tun-host-swap"}:
        return "TUN", body
    if tag in {"proxy", "system-proxy"}:
        return "Прокси", body
    if tag in {"zapret", "zapret-error"}:
        return "Zapret", body
    if tag in {"discord-proxy", "droute"}:
        return "Discord Voice", body
    return tag, body


def classify_log_level(text: str) -> str:
    low = text.lower()
    exit_match = re.search(r"(?:code|код(?:ом)?)\s+(-?\d+)", low)
    if exit_match:
        if int(exit_match.group(1)) != 0 and "expected" not in low:
            return "error"
        return "success"
    if "[error]" in low or "[critical]" in low or "[fatal]" in low or "[panic]" in low:
        return "error"
    if "[warning]" in low or "[warn]" in low:
        return "warning"
    if any(token in low for token in ("connection:", "handshake", "dial tcp", "unexpected http response status", "unexpected response status")):
        return "warning"
    if re.search(r"\b(error|critical|fatal|panic)\b", low) and not "common/errors" in low:
        return "error"
    if re.search(r"\b(warning|warn)\b", low):
        return "warning"
    if any(token in low for token in ("failed", "не удалось")):
        return "error"
    if any(token in low for token in ("deprecated", "timeout", "timed out", "предупреж")):
        return "warning"
    if "ошибк" in low:
        return "error"
    if any(token in low for token in ("ready", "started", "connected", "успеш", "готов", "подключено", "updated")):
        return "success"
    return "info"


def _humanize(body: str, level: str) -> tuple[str, str, str]:
    low = body.lower()
    port_match = _PORT_RE.search(body)
    if port_match and any(token in low for token in ("занят", "in use", "already", "bind")):
        port = port_match.group(1)
        return (
            f"Локальный порт {port} уже используется другой программой.",
            f"Закройте конфликтующее приложение или нажмите «Сменить порт». {body}",
            f"change-port:{port}",
        )
    if "certificate verify failed" in low or "unknown authority" in low:
        return (
            "Не удалось проверить сертификат защищённого соединения.",
            "Проверьте дату Windows, антивирусную HTTPS-проверку и обновления корневых сертификатов. " + body,
            "",
        )
    if "unknown field" in low or "failed to load config" in low or "decode config" in low:
        return (
            "Сетевое ядро не принимает текущий конфиг.",
            "Обновите ядро или сбросьте соответствующий конфиг на шаблон по умолчанию. " + body,
            "",
        )
    if "rule-set" in low and any(token in low for token in ("eof", "failed", "context canceled")):
        return (
            "Не удалось загрузить данные маршрутизации.",
            "Обновите GeoIP/GeoSite во вкладке обновлений и повторите подключение. " + body,
            "",
        )
    if "tls handshake" in low and ("eof" in low or "failed" in low):
        return (
            "Сервер оборвал TLS-соединение.",
            "Проверьте параметры Reality/XHTTP, время Windows и попробуйте другой сервер. " + body,
            "",
        )
    if "tunnel not initialized" in low or "endpoint not initialized" in low:
        return (
            "Не удалось инициализировать подключение WARP/MASQUE.",
            "Проверьте доступ к Cloudflare и параметры импортированного профиля. "
            "Исходная причина обычно указана соседней строкой выше. " + body,
            "",
        )
    core_file_missing = (
        "sing-box path is not configured" in low
        or "xray path is not configured" in low
        or re.search(r"\b(?:xray|sing-box)(?:\.exe)?\s+(?:file\s+)?not found\b", low)
        or re.search(r"\b(?:xray|sing-box)\.exe\b.{0,80}\bno such file\b", low)
    )
    if core_file_missing:
        return (
            "Файл сетевого ядра не найден.",
            "Проверьте путь к ядру в настройках или переустановите Lumen. " + body,
            "",
        )
    if "permission denied" in low or "access is denied" in low or "отказано в доступе" in low:
        return (
            "Windows не разрешила выполнить операцию.",
            "Перезапустите Lumen от имени администратора. " + body,
            "",
        )
    if "no such host" in low or "dns" in low and any(token in low for token in ("failed", "timeout", "deadline")):
        return (
            "DNS не смог определить адрес сайта.",
            "Проверьте подключение, смените режим DNS или переподключите TUN. " + body,
            "",
        )
    if "another instance" in low or "другой экземпляр" in low or "windivert" in low and level == "error":
        return (
            "Драйвер перехвата трафика уже занят.",
            "Закройте другой VPN/Zapret-клиент и повторите запуск. " + body,
            "",
        )
    exit_match = re.search(r"(?:code|код(?:ом)?)\s+(-?\d+)", low)
    abnormal_exit = bool(exit_match and int(exit_match.group(1)) != 0)
    if "exited" in low or "завершился" in low or abnormal_exit:
        return (
            "Сетевое ядро неожиданно остановилось.",
            "Откройте соседние ошибки в журнале; обычно причина указана строкой выше. " + body,
            "",
        )
    return body, body, ""


def parse_log_line(line: str, *, timestamp: datetime | None = None) -> LogEntry:
    cleaned = clean_log_text(line)
    source, body = _split_source(cleaned)
    body = _SINGBOX_PREFIX_RE.sub("", body)
    body = _TRACE_ID_RE.sub("", body).strip()
    level = classify_log_level(cleaned)
    message, details, action_id = _humanize(body or cleaned, level)
    source_text = source or ""
    message_text = message or ""
    details_text = details or ""
    return LogEntry(
        timestamp=(timestamp or datetime.now()).strftime("%H:%M:%S"),
        level=level,
        source=source_text,
        message=message_text,
        details=details_text,
        action_id=action_id,
        action_label="Сменить порт" if action_id else "",
        search_text=f"{source_text} {message_text} {details_text}".lower(),
    )
