"""Offline, bounded provider presentation. Never apply system settings."""
from __future__ import annotations

import base64
import json
import math
import re
import time
from urllib.parse import urlsplit

EXTRA_PROVIDER_PARAMETERS = (
    "fallback-url", "routing-enable", "custom-tunnel-config", "routing", "autorouting",
    "socks-auth-mode", "socks-auth-user", "socks-auth-password",
    "http-auth-mode", "http-auth-user", "http-auth-password",
    "sub-info-color", "sub-info-text", "sub-info-button-text", "sub-info-button-link",
    "sub-expire", "sub-expire-button-link", "no-limit-enabled", "no-limit-xhttp-enabled",
    "fragmentation-maxsplit", "noises-enable", "noises-type", "noises-packet-type",
    "noises-packet", "noises-delay", "noises-rand", "noises-rand-range",
    "per-app-proxy-enable", "per-app-proxy-list-invert", "per-app-proxy-list-set",
    "subscriptions-expand-now", "proxy-enable", "tun-enable", "tun-mode", "tun-type",
    "exclude-routes-set", "color-profile", "include-all-networks-enable",
    "exclude-local-networks-enable", "exclude-apns-enable", "dont-use-filter",
    "subscription-pin", "manual-block-user-agent", "subscriptions-sort-type",
    "dns-from-json-enable", "user-agent-geo-files", "proxy-ping-timeout", "hide-vpn-icon",
    "subscription-request-timeout", "inbound-http-enable", "xray-tun-enable",
    "xray-tun-mtu", "block-bind-to-tunnel-enable", "subscription-alternative-hwid-enabled",
    "proxy-ping-mode",
)


def text(value, limit=512):
    if value is None:
        return ""
    value = str(value).strip()
    if value.lower().startswith("base64:") and len(value) <= 4096:
        try:
            raw = value.split(":", 1)[1].strip()
            value = base64.b64decode(raw + "=" * (-len(raw) % 4), altchars=b"-_", validate=True).decode("utf-8").strip()
        except (ValueError, UnicodeError):
            return ""
    return value[:limit]


def enabled(value):
    return value is True or str(value).strip().lower() in {"1", "true"}


def link(value):
    value = text(value, 2048)
    if any(ord(c) < 32 or ord(c) == 127 for c in value):
        return ""
    try:
        parsed = urlsplit(value)
        if parsed.username is not None or parsed.password is not None:
            return ""
        if parsed.scheme.lower() in {"https", "http"} and parsed.hostname:
            return value
        if parsed.scheme.lower() == "tg" and parsed.netloc in {"resolve", "join"}:
            return value
    except ValueError:
        pass
    return ""


def color(value, fallback):
    value = text(value, 9)
    return value if re.fullmatch(r"#[0-9a-fA-F]{6}", value) else fallback


def contrast(value):
    value = color(value, "#2563EB").lstrip("#")
    r, g, b = (int(value[i:i + 2], 16) for i in (0, 2, 4))
    return "#111827" if r * 0.299 + g * 0.587 + b * 0.114 > 155 else "#FFFFFF"


def _expiry(info):
    value = info.get("expire") or info.get("expiresAt")
    try:
        number = float(value)
        if number > 32_000_000_000:
            number /= 1000
        return number if math.isfinite(number) and number > 0 else None
    except (TypeError, ValueError, OverflowError):
        try:
            from datetime import datetime, timezone
            parsed = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
            return parsed.replace(tzinfo=parsed.tzinfo or timezone.utc).timestamp()
        except (ValueError, TypeError, OverflowError):
            return None


def _display_value(value, key="", depth=0):
    lowered = str(key).lower()
    if any(part in lowered for part in ("password", "auth-user", "token", "adminhwid")) or lowered in {"hwid", "uid"}:
        return "••••"
    if isinstance(value, dict):
        if depth >= 3:
            return "…"
        return {text(k, 80): _display_value(v, k, depth + 1) for k, v in list(value.items())[:16]}
    if isinstance(value, (list, tuple)):
        return [_display_value(v, key, depth + 1) for v in value[:16]] if depth < 3 else "…"
    return text(value, 160 if depth else 512)


def present_subscription(subscription, *, now=None):
    info = subscription.get("userinfo")
    info = info if isinstance(info, dict) else {}
    premium = info.get("premiumFeatures")
    premium = premium if isinstance(premium, dict) else {}
    settings = info.get("providerSettings")
    settings = settings if isinstance(settings, dict) else {}
    theme = info.get("providerTheme")
    theme = theme if isinstance(theme, dict) else {}
    happ = bool(text(info.get("providerId")))
    incy = enabled(info.get("isPremium")) and not enabled(info.get("deviceLimitExceeded"))
    expires = _expiry(info)
    delta = expires - (time.time() if now is None else now) if expires is not None else None
    banners = []

    def banner(message, button, url, bg, button_bg, kind):
        message = text(message, 200)
        if not message or (kind.startswith("happ-") and message == "0"):
            return
        bg, button_bg = color(bg, "#1E40AF"), color(button_bg, "#2563EB")
        banners.append({"text": message, "button": text(button, 40), "url": link(url),
                        "background": bg, "foreground": contrast(bg),
                        "buttonBackground": button_bg, "buttonForeground": contrast(button_bg), "kind": kind})

    # Happ: expiry takes precedence over its informational banner.
    if happ and enabled(premium.get("sub-expire")) and delta is not None and delta <= 3 * 86400:
        message = "Подписка истекла" if delta <= 0 else "Подписка истекает: " + str(max(0, int(delta // 86400))) + " дн."
        banner(message, "Продлить", premium.get("sub-expire-button-link"), "#991B1B", "#2563EB", "happ-expiry")
    elif happ:
        palette = {"red": "#991B1B", "blue": "#1E40AF", "green": "#166534"}
        banner(premium.get("sub-info-text"), premium.get("sub-info-button-text"), premium.get("sub-info-button-link"),
               palette.get(text(premium.get("sub-info-color")), "#1E40AF"), "#2563EB", "happ-info")

    # INCY: header maintenance overrides the panel, and maintenance beats expiry.
    if incy:
        maintenance = bool(text(info.get("bannerText", settings.get("bannerText")))) and (bool(text(info.get("bannerText"))) or enabled(settings.get("bannerEnabled")))
        if maintenance:
            banner(info.get("bannerText", settings.get("bannerText")), info.get("bannerButtonText", settings.get("bannerButtonText")),
                   info.get("premiumUrl") or info.get("bannerButtonUrl", settings.get("bannerButtonUrl")),
                   info.get("bannerBgColor", settings.get("bannerBgColor")), info.get("bannerButtonColor", settings.get("bannerButtonColor")), "incy-info")
        elif enabled(settings.get("expiryBannerEnabled")) and delta is not None and delta <= 3 * 86400:
            banner(settings.get("expiryBannerText"), settings.get("expiryBannerButtonText"), settings.get("expiryBannerButtonUrl"),
                   settings.get("expiryBannerBgColor"), settings.get("expiryBannerButtonColor"), "incy-expiry")

    links = []
    for label, raw in (
        ("Поддержка", info.get("supportUrl") or settings.get("supportUrl")),
        ("Сайт", info.get("profileUrl") or settings.get("webPageUrl")),
        ("Telegram", info.get("telegramUrl") or settings.get("channelUrl")),
        ("Бот", settings.get("botUrl")), ("Premium", info.get("premiumUrl")),
        ("Объявление", info.get("announcementUrl") or settings.get("announceUrl")),
    ):
        if safe := link(raw):
            if not any(item["url"] == safe for item in links):
                links.append({"label": label, "url": safe})
    email = text(info.get("supportEmail"), 254)
    if re.fullmatch(r"[A-Za-z0-9.!#$%&'*+/=_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", email):
        links.append({"label": "Email", "url": "mailto:" + email})

    features = []
    fields = list(premium.items())
    fields += [("INCY." + str(k), v) for k, v in settings.items()]
    fields += [("INCY.theme." + str(k), v) for k, v in theme.items()]
    for key, value in fields[:256]:
        key = str(key)
        rendered = _display_value(value, key)
        if isinstance(rendered, (dict, list)):
            rendered = json.dumps(rendered, ensure_ascii=False)
        status = "Получено; не применяется автоматически"
        if key.startswith("sub-info-") or key.startswith("sub-expire"):
            status = "Отображается по условиям подписки" if happ else "Требуется Provider ID"
        elif key == "subscription-pin":
            status = "Порядок подписок" if happ else "Требуется Provider ID"
        elif key in {"subscriptions-collapse", "subscriptions-expand-now"}:
            status = "Список серверов всегда развёрнут"
        elif key.startswith("INCY.banner") or key.startswith("INCY.expiryBanner"):
            status = "Отображается по условиям подписки" if incy else "Нет активной Premium-конфигурации"
        elif key.startswith("per-app-") or key in {"include-all-networks-enable", "exclude-local-networks-enable", "exclude-apns-enable", "xray-tun-enable", "xray-tun-mtu", "no-limit-enabled", "no-limit-xhttp-enabled", "color-profile", "proxy-ping-timeout", "block-bind-to-tunnel-enable"}:
            status = "Параметр другой платформы"
        features.append({"key": key, "value": rendered[:512], "status": status})

    return {"description": text(info.get("profileDescription")), "announcement": text(info.get("announcement"), 200),
            "links": links, "banners": banners, "featureRows": features,
            "hideUrl": any(str(info.get(k, "")).strip().lower() in {"true", "1", "yes"} for k in ("hideUrl", "hide_url")),
            "pinned": happ and enabled(premium.get("subscription-pin")),
            "provider": "Happ" if happ else ("INCY" if incy else ""),
            "notice": "Лимит Premium-устройств исчерпан; базовые серверы доступны" if enabled(info.get("deviceLimitExceeded")) else "",
            "hasContent": bool(banners or links or info.get("announcement") or info.get("profileDescription") or features or info.get("total") is not None or expires or info.get("bannerText") or info.get("deviceLimitExceeded")),
            "themeAccent": color(theme.get("accent"), "") if incy and enabled(theme.get("enabled")) else ""}
