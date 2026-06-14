"""Country detection from node name/server and flag icon generation."""

from __future__ import annotations

import json
import re
import socket
import urllib.request

from .http_utils import urlopen as _urlopen

from PyQt6.QtCore import QRectF, QThread, Qt, pyqtSignal
from PyQt6.QtGui import QColor, QIcon, QPainter, QPainterPath, QPixmap

# ── Flag stripe data ────────────────────────────────────────────
# (orientation, [colors])  h = horizontal top→bottom, v = vertical left→right

_W, _H = 20, 14

_STRIPES: dict[str, tuple[str, list[str]]] = {
    # Horizontal tri-stripes
    "RU": ("h", ["#FFFFFF", "#0039A6", "#D52B1E"]),
    "DE": ("h", ["#000000", "#DD0000", "#FFCC00"]),
    "NL": ("h", ["#AE1C28", "#FFFFFF", "#21468B"]),
    "LU": ("h", ["#ED2939", "#FFFFFF", "#00A1DE"]),
    "AT": ("h", ["#ED2939", "#FFFFFF", "#ED2939"]),
    "HU": ("h", ["#CE2939", "#FFFFFF", "#477050"]),
    "BG": ("h", ["#FFFFFF", "#00966E", "#D62612"]),
    "LT": ("h", ["#FDB913", "#006A44", "#C1272D"]),
    "EE": ("h", ["#0072CE", "#000000", "#FFFFFF"]),
    "LV": ("h", ["#9E3039", "#FFFFFF", "#9E3039"]),
    "HR": ("h", ["#FF0000", "#FFFFFF", "#171796"]),
    "RS": ("h", ["#C7363C", "#0C4076", "#FFFFFF"]),
    "SI": ("h", ["#FFFFFF", "#003DA5", "#ED1C24"]),
    "AM": ("h", ["#D90012", "#0033A0", "#F2A800"]),
    "AZ": ("h", ["#00B5E2", "#DD0000", "#00B532"]),
    "CO": ("h", ["#FCD116", "#003893", "#CE1126"]),
    "AR": ("h", ["#74ACDF", "#FFFFFF", "#74ACDF"]),
    "IN": ("h", ["#FF9933", "#FFFFFF", "#138808"]),
    "EG": ("h", ["#CE1126", "#FFFFFF", "#000000"]),
    "IR": ("h", ["#239F40", "#FFFFFF", "#DA0000"]),
    "BO": ("h", ["#D52B1E", "#F9E300", "#007934"]),
    "ET": ("h", ["#009A44", "#FCDD09", "#DA121A"]),
    "YE": ("h", ["#CE1126", "#FFFFFF", "#000000"]),
    "IQ": ("h", ["#CE1126", "#FFFFFF", "#000000"]),
    "TJ": ("h", ["#CC0000", "#FFFFFF", "#006600"]),
    "GH": ("h", ["#CF0921", "#FCD116", "#006B3F"]),
    # Horizontal bi-stripes
    "UA": ("h", ["#005BBB", "#FFD500"]),
    "PL": ("h", ["#FFFFFF", "#DC143C"]),
    "ID": ("h", ["#FF0000", "#FFFFFF"]),
    "MC": ("h", ["#CE1126", "#FFFFFF"]),
    "SG": ("h", ["#EF3340", "#FFFFFF"]),
    # Vertical tri-stripes
    "FR": ("v", ["#002395", "#FFFFFF", "#ED2939"]),
    "IT": ("v", ["#009246", "#FFFFFF", "#CE2B37"]),
    "IE": ("v", ["#009A49", "#FFFFFF", "#FF7900"]),
    "BE": ("v", ["#000000", "#FAE042", "#ED2939"]),
    "RO": ("v", ["#002B7F", "#FCD116", "#CE1126"]),
    "MD": ("v", ["#003DA5", "#FCD116", "#CC0000"]),
    "MX": ("v", ["#006847", "#FFFFFF", "#CE1126"]),
    "NG": ("v", ["#008751", "#FFFFFF", "#008751"]),
    "CI": ("v", ["#F77F00", "#FFFFFF", "#009E60"]),
    "PE": ("v", ["#D91023", "#FFFFFF", "#D91023"]),
    "CA": ("v", ["#FF0000", "#FFFFFF", "#FF0000"]),
    "PT": ("v", ["#006600", "#FF0000", "#FF0000"]),
    # Cross flags
    "SE": ("nordic", ["#006AA7", "#FECC02"]),
    "NO": ("nordic", ["#EF2B2D", "#FFFFFF", "#002868"]),
    "FI": ("nordic", ["#FFFFFF", "#003580"]),
    "DK": ("nordic", ["#C60C30", "#FFFFFF"]),
    "IS": ("nordic", ["#003897", "#FFFFFF", "#D72828"]),
    "CH": ("cross", ["#FF0000", "#FFFFFF"]),
    "GR": ("h", ["#0D5EAF", "#FFFFFF", "#0D5EAF"]),
    "GE": ("cross", ["#FFFFFF", "#FF0000"]),
    # Complex flags simplified
    "US": ("h", ["#3C3B6E", "#FFFFFF", "#B22234"]),
    "GB": ("h", ["#012169", "#FFFFFF", "#C8102E"]),
    "JP": ("h", ["#FFFFFF", "#BC002D", "#FFFFFF"]),
    "CN": ("h", ["#DE2910", "#DE2910", "#FFDE00"]),
    "TW": ("h", ["#000095", "#FE0000", "#FE0000"]),
    "HK": ("h", ["#DE2910", "#FFFFFF", "#DE2910"]),
    "KR": ("h", ["#FFFFFF", "#CD2E3A", "#003478"]),
    "TR": ("h", ["#E30A17", "#FFFFFF", "#E30A17"]),
    "IL": ("h", ["#FFFFFF", "#0038B8", "#FFFFFF"]),
    "BR": ("h", ["#009C3B", "#FFDF00", "#009C3B"]),
    "AU": ("h", ["#012169", "#FFFFFF", "#012169"]),
    "NZ": ("h", ["#00247D", "#CC142B", "#00247D"]),
    "ZA": ("h", ["#007749", "#FFB81C", "#DE3831"]),
    "KE": ("h", ["#000000", "#BB0000", "#006600"]),
    "KZ": ("h", ["#00AFCA", "#FFD700", "#00AFCA"]),
    "UZ": ("h", ["#1EB53A", "#FFFFFF", "#0099B5"]),
    "VN": ("h", ["#DA251D", "#FFCD00", "#DA251D"]),
    "SA": ("h", ["#006C35", "#FFFFFF", "#006C35"]),
    "AE": ("h", ["#00732F", "#FFFFFF", "#000000"]),
    "QA": ("h", ["#8A1538", "#FFFFFF", "#8A1538"]),
    "MY": ("h", ["#010066", "#CC0001", "#FFFFFF"]),
    "TH": ("h", ["#A51931", "#F4F5F8", "#2D2A4A"]),
    "PH": ("h", ["#0038A8", "#FFFFFF", "#CE1126"]),
    "MM": ("h", ["#FECB00", "#34B233", "#EA2839"]),
    "BD": ("h", ["#006A4E", "#F42A41", "#006A4E"]),
    "PK": ("h", ["#01411C", "#FFFFFF", "#01411C"]),
    "CL": ("h", ["#FFFFFF", "#0039A6", "#D52B1E"]),
    "PA": ("h", ["#FFFFFF", "#DA121A", "#003DA5"]),
    "CU": ("h", ["#002A8F", "#FFFFFF", "#CF142B"]),
    "ES": ("h", ["#AA151B", "#F1BF00", "#AA151B"]),
    "CZ": ("h", ["#FFFFFF", "#11457E", "#D7141A"]),
    "SK": ("h", ["#FFFFFF", "#0B4EA2", "#EE1C25"]),
    "BA": ("h", ["#002395", "#FECB00", "#002395"]),
    "AL": ("h", ["#E41E20", "#000000", "#E41E20"]),
    "MK": ("h", ["#D20000", "#FFE600", "#D20000"]),
    "ME": ("h", ["#C40308", "#D4AF37", "#C40308"]),
    "MA": ("h", ["#C1272D", "#006233", "#C1272D"]),
    "TN": ("h", ["#E70013", "#FFFFFF", "#E70013"]),
    "DZ": ("h", ["#006233", "#FFFFFF", "#D21034"]),
}

# ── Icon cache & rendering ──────────────────────────────────────
_icon_cache: dict[str, QIcon] = {}


def get_flag_icon(code: str) -> QIcon | None:
    if not code:
        return None
    code = code.upper()
    cached = _icon_cache.get(code)
    if cached is not None:
        return cached
    pm = _draw_flag(code)
    icon = QIcon(pm)
    _icon_cache[code] = icon
    return icon


def get_flag_emoji(code: str) -> str:
    """Return a regional-indicator flag emoji for an ISO 3166-1 alpha-2 code."""
    code = str(code or "").strip().upper()
    if len(code) != 2 or not code.isalpha():
        return ""
    return "".join(chr(0x1F1E6 + ord(ch) - ord("A")) for ch in code)


def _draw_flag(code: str) -> QPixmap:
    pm = QPixmap(_W, _H)
    pm.fill(QColor(0, 0, 0, 0))

    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)

    path = QPainterPath()
    path.addRoundedRect(QRectF(0, 0, _W, _H), 2, 2)
    p.setClipPath(path)

    data = _STRIPES.get(code)
    if data:
        orient, colors = data
        if orient == "h":
            sh = _H / len(colors)
            for i, c in enumerate(colors):
                p.fillRect(QRectF(0, i * sh, _W, sh + 0.5), QColor(c))
        elif orient == "v":
            sw = _W / len(colors)
            for i, c in enumerate(colors):
                p.fillRect(QRectF(i * sw, 0, sw + 0.5, _H), QColor(c))
        elif orient == "nordic":
            p.fillRect(QRectF(0, 0, _W, _H), QColor(colors[0]))
            outer = QColor(colors[1])
            inner = QColor(colors[2]) if len(colors) > 2 else outer
            p.fillRect(QRectF(0, 5, _W, 4), outer)
            p.fillRect(QRectF(6, 0, 4, _H), outer)
            if len(colors) > 2:
                p.fillRect(QRectF(0, 6, _W, 2), inner)
                p.fillRect(QRectF(7, 0, 2, _H), inner)
        elif orient == "cross":
            p.fillRect(QRectF(0, 0, _W, _H), QColor(colors[0]))
            cross = QColor(colors[1])
            p.fillRect(QRectF(0, 5, _W, 4), cross)
            p.fillRect(QRectF(8, 0, 4, _H), cross)
    else:
        p.fillRect(QRectF(0, 0, _W, _H), QColor("#B0BEC5"))

    p.setClipping(False)
    p.setPen(QColor(0, 0, 0, 30))
    p.setBrush(Qt.BrushStyle.NoBrush)
    p.drawRoundedRect(QRectF(0.5, 0.5, _W - 1, _H - 1), 2, 2)
    p.end()
    return pm


# ── Country detection ───────────────────────────────────────────

def detect_country(name: str, server: str) -> str:
    return (
        _detect_emoji(name)
        or _detect_name(name)
        or _detect_code(name)
        or _detect_server(server)
        or ""
    )


def _detect_emoji(text: str) -> str:
    codepoints = [ord(c) for c in text]
    for i in range(len(codepoints) - 1):
        a, b = codepoints[i], codepoints[i + 1]
        if 0x1F1E6 <= a <= 0x1F1FF and 0x1F1E6 <= b <= 0x1F1FF:
            return chr(a - 0x1F1E6 + ord("A")) + chr(b - 0x1F1E6 + ord("A"))
    return ""


# Sorted longest-first so multi-word names match before shorter ones
_NAMES: list[tuple[str, str]] = sorted(
    [
        # English country names
        ("united states", "US"), ("usa", "US"), ("america", "US"),
        ("russia", "RU"), ("germany", "DE"), ("france", "FR"),
        ("united kingdom", "GB"), ("great britain", "GB"), ("england", "GB"),
        ("japan", "JP"), ("china", "CN"), ("south korea", "KR"), ("korea", "KR"),
        ("taiwan", "TW"), ("hong kong", "HK"), ("singapore", "SG"),
        ("netherlands", "NL"), ("holland", "NL"),
        ("italy", "IT"), ("spain", "ES"), ("portugal", "PT"),
        ("ireland", "IE"), ("belgium", "BE"), ("austria", "AT"),
        ("switzerland", "CH"), ("poland", "PL"), ("czech", "CZ"), ("czechia", "CZ"),
        ("slovakia", "SK"), ("hungary", "HU"), ("romania", "RO"), ("bulgaria", "BG"),
        ("croatia", "HR"), ("serbia", "RS"), ("greece", "GR"), ("turkey", "TR"),
        ("turkiye", "TR"), ("ukraine", "UA"), ("moldova", "MD"), ("georgia", "GE"),
        ("armenia", "AM"), ("azerbaijan", "AZ"), ("kazakhstan", "KZ"),
        ("uzbekistan", "UZ"), ("sweden", "SE"), ("norway", "NO"),
        ("finland", "FI"), ("denmark", "DK"), ("iceland", "IS"),
        ("estonia", "EE"), ("latvia", "LV"), ("lithuania", "LT"),
        ("luxembourg", "LU"), ("albania", "AL"), ("north macedonia", "MK"),
        ("bosnia", "BA"), ("montenegro", "ME"), ("slovenia", "SI"),
        ("israel", "IL"), ("india", "IN"), ("thailand", "TH"),
        ("vietnam", "VN"), ("indonesia", "ID"), ("malaysia", "MY"),
        ("philippines", "PH"), ("brazil", "BR"), ("argentina", "AR"),
        ("chile", "CL"), ("colombia", "CO"), ("mexico", "MX"),
        ("canada", "CA"), ("australia", "AU"), ("new zealand", "NZ"),
        ("south africa", "ZA"), ("nigeria", "NG"), ("kenya", "KE"),
        ("egypt", "EG"), ("uae", "AE"), ("emirates", "AE"),
        ("saudi arabia", "SA"), ("panama", "PA"), ("iran", "IR"),
        ("iraq", "IQ"), ("pakistan", "PK"), ("bangladesh", "BD"),
        ("cambodia", "KH"), ("myanmar", "MM"), ("mongolia", "MN"),
        # Russian country names
        ("россия", "RU"), ("германия", "DE"), ("франция", "FR"),
        ("великобритания", "GB"), ("англия", "GB"), ("япония", "JP"),
        ("китай", "CN"), ("корея", "KR"), ("тайвань", "TW"),
        ("гонконг", "HK"), ("сингапур", "SG"), ("нидерланды", "NL"),
        ("голландия", "NL"), ("италия", "IT"), ("испания", "ES"),
        ("португалия", "PT"), ("ирландия", "IE"), ("бельгия", "BE"),
        ("австрия", "AT"), ("швейцария", "CH"), ("польша", "PL"),
        ("чехия", "CZ"), ("словакия", "SK"), ("венгрия", "HU"),
        ("румыния", "RO"), ("болгария", "BG"), ("хорватия", "HR"),
        ("сербия", "RS"), ("греция", "GR"), ("турция", "TR"),
        ("украина", "UA"), ("молдова", "MD"), ("молдавия", "MD"),
        ("грузия", "GE"), ("армения", "AM"), ("азербайджан", "AZ"),
        ("казахстан", "KZ"), ("узбекистан", "UZ"), ("швеция", "SE"),
        ("норвегия", "NO"), ("финляндия", "FI"), ("дания", "DK"),
        ("исландия", "IS"), ("эстония", "EE"), ("латвия", "LV"),
        ("литва", "LT"), ("люксембург", "LU"), ("албания", "AL"),
        ("македония", "MK"), ("босния", "BA"), ("черногория", "ME"),
        ("словения", "SI"), ("израиль", "IL"), ("индия", "IN"),
        ("таиланд", "TH"), ("вьетнам", "VN"), ("индонезия", "ID"),
        ("малайзия", "MY"), ("филиппины", "PH"), ("бразилия", "BR"),
        ("аргентина", "AR"), ("чили", "CL"), ("колумбия", "CO"),
        ("мексика", "MX"), ("канада", "CA"), ("австралия", "AU"),
        ("новая зеландия", "NZ"), ("юар", "ZA"), ("нигерия", "NG"),
        ("кения", "KE"), ("египет", "EG"), ("оаэ", "AE"), ("эмираты", "AE"),
        ("саудовская аравия", "SA"), ("панама", "PA"), ("иран", "IR"),
        ("ирак", "IQ"), ("пакистан", "PK"), ("монголия", "MN"),
        # Major cities
        ("moscow", "RU"), ("saint petersburg", "RU"), ("novosibirsk", "RU"),
        ("москва", "RU"), ("питер", "RU"), ("спб", "RU"), ("петербург", "RU"),
        ("new york", "US"), ("los angeles", "US"), ("chicago", "US"),
        ("san francisco", "US"), ("seattle", "US"), ("dallas", "US"),
        ("miami", "US"), ("atlanta", "US"), ("washington", "US"),
        ("silicon valley", "US"), ("ashburn", "US"), ("phoenix", "US"),
        ("las vegas", "US"), ("denver", "US"), ("boston", "US"),
        ("berlin", "DE"), ("frankfurt", "DE"), ("munich", "DE"), ("hamburg", "DE"),
        ("dusseldorf", "DE"), ("nuremberg", "DE"),
        ("paris", "FR"), ("marseille", "FR"), ("lyon", "FR"),
        ("london", "GB"), ("manchester", "GB"), ("edinburgh", "GB"),
        ("tokyo", "JP"), ("osaka", "JP"),
        ("beijing", "CN"), ("shanghai", "CN"), ("guangzhou", "CN"), ("shenzhen", "CN"),
        ("seoul", "KR"), ("busan", "KR"),
        ("taipei", "TW"),
        ("amsterdam", "NL"), ("rotterdam", "NL"),
        ("rome", "IT"), ("milan", "IT"),
        ("madrid", "ES"), ("barcelona", "ES"),
        ("lisbon", "PT"), ("dublin", "IE"), ("brussels", "BE"),
        ("vienna", "AT"), ("wien", "AT"),
        ("zurich", "CH"), ("geneva", "CH"), ("bern", "CH"),
        ("warsaw", "PL"), ("krakow", "PL"),
        ("prague", "CZ"), ("budapest", "HU"), ("bucharest", "RO"),
        ("sofia", "BG"), ("zagreb", "HR"), ("belgrade", "RS"),
        ("athens", "GR"),
        ("kyiv", "UA"), ("kiev", "UA"), ("odessa", "UA"),
        ("киев", "UA"), ("одесса", "UA"),
        ("tbilisi", "GE"), ("yerevan", "AM"), ("baku", "AZ"),
        ("astana", "KZ"), ("almaty", "KZ"), ("tashkent", "UZ"),
        ("stockholm", "SE"), ("oslo", "NO"), ("helsinki", "FI"),
        ("copenhagen", "DK"), ("reykjavik", "IS"),
        ("tallinn", "EE"), ("riga", "LV"), ("vilnius", "LT"),
        ("istanbul", "TR"), ("ankara", "TR"),
        ("tel aviv", "IL"), ("jerusalem", "IL"),
        ("dubai", "AE"), ("abu dhabi", "AE"),
        ("riyadh", "SA"), ("jeddah", "SA"),
        ("cairo", "EG"), ("johannesburg", "ZA"), ("cape town", "ZA"),
        ("lagos", "NG"), ("nairobi", "KE"),
        ("sao paulo", "BR"), ("rio de janeiro", "BR"),
        ("buenos aires", "AR"), ("santiago", "CL"), ("bogota", "CO"),
        ("mexico city", "MX"),
        ("toronto", "CA"), ("montreal", "CA"), ("vancouver", "CA"),
        ("sydney", "AU"), ("melbourne", "AU"), ("perth", "AU"),
        ("auckland", "NZ"),
        ("mumbai", "IN"), ("delhi", "IN"), ("bangalore", "IN"),
        ("bangkok", "TH"),
        ("ho chi minh", "VN"), ("hanoi", "VN"),
        ("jakarta", "ID"), ("manila", "PH"), ("kuala lumpur", "MY"),
    ],
    key=lambda x: -len(x[0]),
)

_VALID_CODES: frozenset[str] = frozenset(_STRIPES.keys())


def _detect_name(name: str) -> str:
    lower = name.lower()
    for pattern, code in _NAMES:
        if re.search(r"\b" + re.escape(pattern) + r"\b", lower):
            return code
    return ""


def _detect_code(name: str) -> str:
    m = re.match(r"^([A-Za-z]{2})(?:\s*[-_.|#\s]|\d)", name)
    if m:
        cc = m.group(1).upper()
        if cc in _VALID_CODES:
            return cc
    return ""


_COUNTRY_TLDS: frozenset[str] = frozenset({
    "ru", "de", "fr", "jp", "kr", "cn", "ua", "pl", "cz", "nl",
    "se", "no", "fi", "dk", "at", "ch", "it", "es", "pt", "ie",
    "be", "hu", "ro", "bg", "hr", "rs", "gr", "tr", "il", "br",
    "ar", "mx", "ca", "au", "nz", "in", "sg", "my", "th", "vn",
    "id", "ph", "eg", "za", "ng", "ke", "kz", "uz", "ge", "am",
    "az", "ee", "lv", "lt", "lu", "al", "mk", "ba", "sk", "md",
    "tw", "hk", "pk", "bd", "ir", "iq", "sa", "ae", "qa", "us",
    "uk",
})

_TLD_REMAP: dict[str, str] = {"uk": "GB"}


def _detect_server(server: str) -> str:
    if not server:
        return ""
    server = server.lower().strip(".")
    # TLD check
    parts = server.rsplit(".", 1)
    if len(parts) == 2 and len(parts[1]) == 2:
        tld = parts[1]
        if tld in _COUNTRY_TLDS:
            return _TLD_REMAP.get(tld, tld.upper())
    # First label: "de1.server.com" → "de"
    first = server.split(".")[0]
    m = re.match(r"^([a-z]{2})\d", first)
    if m and m.group(1) in _COUNTRY_TLDS:
        return _TLD_REMAP.get(m.group(1), m.group(1).upper())
    return ""


# ── Async IP-based country resolution ───────────────────────────

class CountryResolver(QThread):
    resolved = pyqtSignal(dict)  # {node_id: country_code}

    def __init__(self, nodes: list[tuple[str, str]], parent=None):
        super().__init__(parent)
        self._nodes = nodes  # [(node_id, server_address), ...]

    def run(self) -> None:
        results: dict[str, str] = {}
        ip_map: dict[str, list[str]] = {}  # ip → [node_ids]

        for node_id, server in self._nodes:
            try:
                infos = socket.getaddrinfo(server, None, socket.AF_INET, socket.SOCK_STREAM)
                if infos:
                    ip = infos[0][4][0]
                    ip_map.setdefault(ip, []).append(node_id)
            except Exception:
                pass

        if not ip_map:
            self.resolved.emit(results)
            return

        ips = list(ip_map.keys())
        for i in range(0, len(ips), 100):
            batch = ips[i : i + 100]
            try:
                payload = json.dumps(
                    [{"query": ip, "fields": "countryCode,query"} for ip in batch]
                ).encode()
                req = urllib.request.Request(
                    "http://ip-api.com/batch",
                    data=payload,
                    headers={"Content-Type": "application/json"},
                )
                with _urlopen(req, timeout=10) as resp:
                    data = json.loads(resp.read())
                for item in data:
                    cc = item.get("countryCode", "")
                    ip = item.get("query", "")
                    if cc and ip in ip_map:
                        for nid in ip_map[ip]:
                            results[nid] = cc
            except Exception:
                pass

        self.resolved.emit(results)
