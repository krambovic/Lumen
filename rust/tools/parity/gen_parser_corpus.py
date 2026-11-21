"""Build the link-parser parity fixture for the Rust test suite.

Inputs come from two places:

* ``--recorded``: the JSONL written by ``record_parser_inputs.py`` while the
  whole Python test suite runs (every text the Python tests parse);
* ``synthetic_inputs()``: generated links and documents covering every scheme,
  transport, security mode and container format the Python parser accepts.

Each input is replayed through the Python parser and the results (nodes
without their random ``id``/``created_at``, errors, ``validate_node_outbound``
and ``normalize_node_outbound``) are written to
``rust/crates/lumen-core/tests/fixtures/parser_parity.json``.

Run from the repository root::

    python rust/tools/parity/gen_parser_corpus.py --recorded inputs.jsonl
"""

from __future__ import annotations

import argparse
import base64
import copy
import itertools
import json
import sys
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "windows"))

from xray_fluent import link_parser  # noqa: E402
from xray_fluent.models import Node  # noqa: E402

FIXTURE = ROOT / "rust" / "crates" / "lumen-core" / "tests" / "fixtures" / "parser_parity.json"
MAX_INPUT_CHARS = 64_000

UUID = "11111111-2222-3333-4444-555555555555"
PBK = "Z84J2IelR9ch3k8VtlVhhs5ycBUlXA7wHBWcBrjqnAw"
WG_PRIV = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk="
WG_PUB = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg="
WG_PSK = "FpCyhws9cxwWoV4xELtfJvjJN+zQVRPISllRWgeopVE="


def b64(text: str, *, urlsafe: bool = False, pad: bool = True) -> str:
    raw = text.encode("utf-8")
    data = base64.urlsafe_b64encode(raw) if urlsafe else base64.b64encode(raw)
    out = data.decode("ascii")
    return out if pad else out.rstrip("=")


def query(params: dict[str, str]) -> str:
    return "&".join(f"{key}={quote(str(value), safe='')}" for key, value in params.items())


TRANSPORTS: dict[str, dict[str, str]] = {
    "tcp": {"type": "tcp"},
    "raw": {"type": "raw"},
    "tcp-http": {"type": "tcp", "headerType": "http", "host": "cdn.example", "path": "/a"},
    "ws": {"type": "ws", "host": "cdn.example", "path": "/ws?ed=2048"},
    "ws-ed": {"type": "ws", "host": "cdn.example", "path": "/ws", "ed": "2560"},
    "grpc": {"type": "grpc", "serviceName": "svc", "mode": "multi", "authority": "auth.example"},
    "grpc-gun": {"type": "grpc", "serviceName": "svc", "mode": "gun"},
    "h2": {"type": "h2", "host": "h.example", "path": "/h2"},
    "http": {"type": "http", "host": "h.example", "path": "/h"},
    "httpupgrade": {"type": "httpupgrade", "host": "u.example", "path": "/up"},
    "xhttp": {"type": "xhttp", "host": "x.example", "path": "/x", "mode": "auto"},
    "xhttp-packet": {"type": "xhttp", "path": "/x", "mode": "packet-up",
                     "extra": json.dumps({"xPaddingBytes": "100-1000", "noGRPCHeader": True})},
    "xhttp-stream": {"type": "xhttp", "path": "/x", "mode": "stream-one"},
    "splithttp": {"type": "splithttp", "path": "/s", "host": "s.example"},
    "kcp": {"type": "kcp", "seed": "secret", "headerType": "wechat-video"},
    "quic": {"type": "quic", "quicSecurity": "aes-128-gcm", "key": "k", "headerType": "srtp"},
    "unknown": {"type": "meek"},
}

SECURITIES: dict[str, dict[str, str]] = {
    "none": {"security": "none"},
    "tls": {"security": "tls", "sni": "sni.example", "fp": "chrome", "alpn": "h2,http/1.1"},
    "tls-insecure": {"security": "tls", "sni": "sni.example", "allowInsecure": "1"},
    "tls-pin": {"security": "tls", "sni": "sni.example", "pcs": "AA:BB", "vcn": "a.example,b.example"},
    "reality": {"security": "reality", "sni": "www.microsoft.com", "pbk": PBK, "sid": "6ba85179e30d4fc2",
                "spx": "/", "fp": "firefox"},
    "reality-nofp": {"security": "reality", "sni": "www.microsoft.com", "pbk": PBK, "sid": "ab"},
}


def vless_links() -> list[str]:
    links = []
    for (tname, tparams), (sname, sparams) in itertools.product(TRANSPORTS.items(), SECURITIES.items()):
        params = {"encryption": "none", **tparams, **sparams}
        links.append(f"vless://{UUID}@v.example:443?{query(params)}#VLESS%20{tname}%20{sname}")
        if sname.startswith("reality") or sname == "tls":
            vision = {**params, "flow": "xtls-rprx-vision"}
            links.append(f"vless://{UUID}@v.example:443?{query(vision)}#vision-{tname}-{sname}")
    links += [
        f"vless://{UUID}@[2001:db8::1]:8443?security=tls&type=ws&path=%2F#ipv6",
        f"vless://{UUID}@v.example:443?flow=xtls-rprx-vision&security=none#vision-plain",
        f"vless://{UUID}@v.example:443?encryption=mlkem768x25519plus.native.0rtt.abc&flow=xtls-rprx-vision&type=grpc#enc",
        f"vless://{UUID}@v.example:443?type=ws&security=tls&packetEncoding=xudp&mux=1#mux",
        f"VLESS://{UUID}@V.EXAMPLE:443?SECURITY=TLS#upper",
        f"vless://{UUID}@v.example?security=tls#noport",
        f"vless://{UUID}@v.example:70000?security=tls#badport",
        f"vless://@v.example:443#nouuid",
        f"vless://{UUID}@v.example:443?security=tls#%F0%9F%87%A9%F0%9F%87%AA%20Germany%20%D0%91%D0%B5%D1%80%D0%BB%D0%B8%D0%BD",
        f"vless://{UUID}@v.example:443?security=tls&sni=a&sni=b#dup",
        f"  vless://{UUID}@v.example:443?security=tls#spaces  ",
    ]
    return links


def vmess_links() -> list[str]:
    links = []
    nets = ["tcp", "ws", "grpc", "h2", "http", "httpupgrade", "xhttp", "kcp", "quic"]
    for net, tls in itertools.product(nets, ["", "tls", "reality"]):
        payload = {"v": "2", "ps": f"vmess {net} {tls}", "add": "m.example", "port": "443", "id": UUID,
                   "aid": "0", "scy": "auto", "net": net, "type": "none", "host": "h.example",
                   "path": "/p", "tls": tls, "sni": "s.example", "alpn": "h2", "fp": "chrome"}
        if tls == "reality":
            payload["pbk"] = PBK
        links.append("vmess://" + b64(json.dumps(payload)))
    base = {"v": "2", "ps": "v", "add": "m.example", "port": 443, "id": UUID, "aid": 0, "net": "tcp"}
    for variant in (
        {**base, "scy": ""}, {**base, "scy": None}, {**base, "scy": "none"}, {**base, "scy": "zero"},
        {**base, "scy": "chacha20-poly1305"}, {**base, "aid": "64"}, {**base, "port": "abc"},
        {**base, "type": "http", "host": "a.example,b.example"}, {k: v for k, v in base.items() if k != "id"},
        {**base, "ps": "Русский сервер 🇷🇺"},
    ):
        links.append("vmess://" + b64(json.dumps(variant, ensure_ascii=False)))
        links.append("vmess://" + b64(json.dumps(variant, ensure_ascii=False), urlsafe=True, pad=False))
    links.append("vmess://" + base64.b64encode(json.dumps({**base, "ps": "ÿ"}).encode("latin-1")).decode())
    links.append(f"vmess://{UUID}@m.example:443?type=ws&security=tls#url-form")
    links.append("vmess://not-base64!!")
    return links


def trojan_links() -> list[str]:
    links = []
    for (tname, tparams), sname in itertools.product(TRANSPORTS.items(), ["none", "tls", "tls-insecure", "reality"]):
        params = {**tparams, **SECURITIES[sname]}
        links.append(f"trojan://p%40ss@t.example:443?{query(params)}#trojan-{tname}-{sname}")
    links += ["trojan://pw@t.example:443#bare", "trojan://t.example:443#nopass", "trojan://pw@t.example#noport"]
    return links


SS_METHODS = [
    "aes-128-gcm", "aes-256-gcm", "chacha20-ietf-poly1305", "xchacha20-ietf-poly1305", "AEAD_CHACHA20_POLY1305",
    "2022-blake3-aes-128-gcm", "2022-blake3-aes-256-gcm", "2022-blake3-chacha20-poly1305", "rc4-md5",
    "aes-256-cfb", "chacha20", "none", "plain", "made-up-cipher",
]


def ss_links() -> list[str]:
    links = []
    for method in SS_METHODS:
        password = base64.b64encode(b"k" * 16).decode() if "2022" in method else "secret"
        links.append(f"ss://{b64(f'{method}:{password}', urlsafe=True, pad=False)}@s.example:8388#ss-{method}")
        links.append(f"ss://{quote(method)}:{quote(password, safe='')}@s.example:8388#plain-{method}")
        links.append("ss://" + b64(f"{method}:{password}@s.example:8388") + f"#legacy-{method}")
    key32 = base64.b64encode(b"k" * 32).decode()
    links += [
        f"ss://{b64('2022-blake3-aes-128-gcm:' + key32)}@s.example:8388#ss2022-wrong-size",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin={quote('obfs-local;obfs=http;obfs-host=a.example')}#obfs",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin={quote('simple-obfs;obfs=tls')}#simple-obfs",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin={quote('v2ray-plugin;mode=websocket;tls;host=a.example;path=/v;mux=true')}#v2ray",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin={quote('v2ray-plugin;mux=0')}#v2ray-mux0",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin={quote('shadow-tls;host=a.example;password=x;version=3')}#shadowtls",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?plugin=unknown-plugin#unknown-plugin",
        f"ss://{b64('aes-128-gcm:pw')}@s.example:8388?uot=1#uot",
        f"ss://{b64('aes-128-gcm:pw')}@[2001:db8::2]:8388#ipv6",
        f"ss://{b64('aes-128-gcm:')}@s.example:8388#empty-password",
        "ss://garbage@s.example:8388#garbage",
        "ss://" + b64("aes-128-gcm:pw@s.example:8388"),
    ]
    return links


def simple_proxy_links() -> list[str]:
    links = []
    for scheme in ("socks", "socks5", "http", "https"):
        links += [
            f"{scheme}://p.example:1080#{scheme}-noauth",
            f"{scheme}://user:pa%3Ass@p.example:1080#{scheme}-auth",
            f"{scheme}://{b64('user:pass')}@p.example:1080#{scheme}-b64auth",
            f"{scheme}://p.example#{scheme}-noport",
            f"{scheme}://[::1]:1080#{scheme}-ipv6",
        ]
    links += ["https://p.example:443?sni=s.example&insecure=1#https-sni"]
    return links


def quic_family_links() -> list[str]:
    return [
        "hysteria://h.example:443?auth=secret&upmbps=50&downmbps=100&obfs=xplus&obfsParam=key&protocol=udp&peer=sni.example&insecure=1&alpn=h3#hy1",
        "hy://h.example:443?auth=secret#hy1-short",
        "hysteria://h.example:443?protocol=faketcp#hy1-faketcp",
        "hysteria2://pass@h.example:443?sni=s.example&obfs=salamander&obfs-password=o&insecure=1#hy2",
        "hy2://pass@h.example:443/?pinSHA256=AA%3ABB&mport=20000-30000#hy2-mport",
        "hysteria2://h.example:443?auth=pw#hy2-auth-param",
        "hysteria2://pass@h.example#hy2-noport",
        "hysteria2://pass@[2001:db8::3]:443#hy2-ipv6",
        "tuic://uuid:pw@t.example:443?congestion_control=cubic&udp_relay_mode=quic&alpn=h3,spdy/3.1&sni=s.example&allow_insecure=1&disable_sni=1#tuic",
        "tuic://uuid:pw@t.example:443?congestion-control=new_reno&reduce_rtt=1#tuic-dash",
        "tuic://uuid@t.example:443#tuic-nopass",
        "naive+https://u:p@n.example:443?padding=1#naive-https",
        "naive+quic://u:p@n.example:443#naive-quic",
        "naive://u:p@n.example:443#naive-plain",
        "quic://u:p@n.example:443#quic",
        "naive+https://n.example#naive-noauth",
        "anytls://pw@a.example:443?sni=a.example&fp=chrome#anytls",
        "anytls://pw@a.example#anytls-noport",
        "mieru://u:p@m.example:1234?transport=UDP&multiplexing=MULTIPLEXING_HIGH#mieru-udp",
        "mierus://u:p@m.example:1234-1240?profile=default#mierus",
        "mieru://m.example:1234#mieru-noauth",
        "masque://token@mq.example:443?sni=mq.example&insecure=1#masque",
        "masque://mq.example:443#masque-notoken",
    ]


def wireguard_links() -> list[str]:
    priv, pub = quote(WG_PRIV, safe=""), quote(WG_PUB, safe="")
    common = f"publickey={pub}&address=10.0.0.2%2F32,fd00::2%2F128&mtu=1280"
    return [
        f"wireguard://{priv}@wg.example:51820?{common}#wg",
        f"wg://{priv}@wg.example:51820?{common}&reserved=1,2,3#wg-reserved",
        f"wg://{priv}@162.159.192.1:2408?{common}&reserved=1,2,3#warp-reserved",
        f"warp://{priv}@engage.cloudflareclient.com:2408?{common}#warp",
        f"awg://{priv}@wg.example:51820?{common}&jc=4&jmin=40&jmax=70&s1=0&s2=0&h1=1&h2=2&h3=3&h4=4#awg1",
        f"awg://{priv}@wg.example:51820?{common}&jc=4&jmin=40&jmax=70&s1=15&s2=18&s3=20&s4=8&h1=100-200&h2=300-400&h3=500-600&h4=700-800&i1=%3Cb%200x01%3E#awg2",
        f"awg://{priv}@wg.example:51820?{common}&randomtrailers=true&disablecookies=1#awg31",
        f"wireguard://{priv}@[2001:db8::4]:51820?{common}#wg-ipv6",
        f"wireguard://{priv}@wg.example:51820#wg-nopub",
        f"wireguard://{priv}@wg.example:51820?{common}&presharedkey={quote(WG_PSK, safe='')}&keepalive=25#wg-psk",
    ]


def wg_configs() -> list[str]:
    base_iface = f"[Interface]\nPrivateKey = {WG_PRIV}\nAddress = 10.0.0.2/32, fd00::2/128\nDNS = 1.1.1.1, 10.0.0.1\nMTU = 1420\n"
    peer = f"[Peer]\nPublicKey = {WG_PUB}\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = wg.example:51820\nPersistentKeepalive = 25\n"
    return [
        base_iface + "\n" + peer,
        base_iface + "\n" + peer + f"PresharedKey = {WG_PSK}\n",
        base_iface + "Jc = 4\nJmin = 40\nJmax = 70\nS1 = 0\nS2 = 0\nH1 = 1\nH2 = 2\nH3 = 3\nH4 = 4\n\n" + peer,
        base_iface + "Jc = 4\nJmin = 40\nJmax = 70\nS1 = 15\nS2 = 18\nS3 = 20\nS4 = 8\nH1 = 100-200\nH2 = 300-400\nH3 = 500-600\nH4 = 700-800\nI1 = <b 0x01>\n\n" + peer,
        base_iface + "\n" + peer.replace("wg.example:51820", "2001:db8::5:51820"),
        base_iface + "\n" + peer.replace("wg.example:51820", "[2001:db8::5]:51820"),
        base_iface + "\n" + peer + "\n" + peer.replace("wg.example", "wg2.example"),
        base_iface.replace("\n", "\r\n") + "\r\n" + peer.replace("\n", "\r\n"),
        "[Interface]\nPrivateKey = broken\n\n[Peer]\nEndpoint = wg.example:51820\n",
    ]


def clash_documents() -> list[str]:
    proxies = [
        {"name": "c-vless", "type": "vless", "server": "c.example", "port": 443, "uuid": UUID, "tls": True,
         "network": "ws", "ws-opts": {"path": "/ws", "headers": {"Host": "h.example"}}, "servername": "s.example"},
        {"name": "c-vless-reality", "type": "vless", "server": "c.example", "port": 443, "uuid": UUID, "tls": True,
         "flow": "xtls-rprx-vision", "reality-opts": {"public-key": PBK, "short-id": "ab"},
         "client-fingerprint": "chrome", "servername": "www.microsoft.com"},
        {"name": "c-vless-grpc", "type": "vless", "server": "c.example", "port": 443, "uuid": UUID, "tls": True,
         "network": "grpc", "grpc-opts": {"grpc-service-name": "svc"}},
        {"name": "c-vmess", "type": "vmess", "server": "c.example", "port": 443, "uuid": UUID, "alterId": 0,
         "cipher": "auto", "network": "h2", "h2-opts": {"host": ["h.example"], "path": "/h2"}, "tls": True},
        {"name": "c-vmess-null", "type": "vmess", "server": "c.example", "port": 443, "uuid": UUID, "cipher": None},
        {"name": "c-trojan", "type": "trojan", "server": "c.example", "port": 443, "password": "pw", "sni": "s.example",
         "skip-cert-verify": True},
        {"name": "c-ss", "type": "ss", "server": "c.example", "port": 8388, "cipher": "aes-128-gcm", "password": "pw",
         "plugin": "obfs", "plugin-opts": {"mode": "tls", "host": "a.example"}},
        {"name": "c-ss-v2ray", "type": "ss", "server": "c.example", "port": 8388, "cipher": "chacha20-ietf-poly1305",
         "password": "pw", "plugin": "v2ray-plugin", "plugin-opts": {"mode": "websocket", "tls": True, "mux": True}},
        {"name": "c-ss-udp-over-tcp", "type": "ss", "server": "c.example", "port": 8388, "cipher": "aes-128-gcm",
         "password": "pw", "udp-over-tcp": True},
        {"name": "c-socks", "type": "socks5", "server": "c.example", "port": 1080, "username": "u", "password": "p"},
        {"name": "c-http", "type": "http", "server": "c.example", "port": 8080, "tls": True},
        {"name": "c-hy2", "type": "hysteria2", "server": "c.example", "port": 443, "password": "pw",
         "obfs": "salamander", "obfs-password": "o", "sni": "s.example", "ports": "20000-30000"},
        {"name": "c-hy1", "type": "hysteria", "server": "c.example", "port": 443, "auth-str": "pw", "up": "50 Mbps",
         "down": "100 Mbps", "obfs": "x"},
        {"name": "c-tuic", "type": "tuic", "server": "c.example", "port": 443, "uuid": UUID, "password": "pw",
         "congestion-controller": "bbr", "udp-relay-mode": "native", "alpn": ["h3"]},
        {"name": "c-wg", "type": "wireguard", "server": "c.example", "port": 51820, "private-key": WG_PRIV,
         "public-key": WG_PUB, "ip": "10.0.0.2", "ipv6": "fd00::2", "mtu": 1280, "udp": True},
        {"name": "c-warp", "type": "wireguard", "server": "162.159.192.1", "port": 2408, "private-key": WG_PRIV,
         "public-key": WG_PUB, "ip": "172.16.0.2", "reserved": [1, 2, 3]},
        {"name": "c-awg", "type": "wireguard", "server": "c.example", "port": 51820, "private-key": WG_PRIV,
         "public-key": WG_PUB, "ip": "10.0.0.2", "amnezia-wg-option": {"jc": 4, "jmin": 40, "jmax": 70, "s1": 0,
                                                                         "s2": 0, "h1": 1, "h2": 2, "h3": 3, "h4": 4}},
        {"name": "c-anytls", "type": "anytls", "server": "c.example", "port": 443, "password": "pw"},
        {"name": "c-mieru", "type": "mieru", "server": "c.example", "port": 1234, "transport": "TCP",
         "username": "u", "password": "p"},
        {"name": "c-masque", "type": "masque", "server": "c.example", "port": 443, "private-key": WG_PRIV,
         "public-key": WG_PUB, "ip": "172.16.0.2"},
        {"name": "c-unknown", "type": "snell", "server": "c.example", "port": 1},
    ]

    def dump(value, indent=0):
        pad = "  " * indent
        if isinstance(value, dict):
            lines = []
            for key, item in value.items():
                if isinstance(item, (dict, list)) and item:
                    lines.append(f"{pad}{key}:")
                    lines.append(dump(item, indent + 1))
                else:
                    lines.append(f"{pad}{key}: {scalar(item)}")
            return "\n".join(lines)
        lines = []
        for item in value:
            if isinstance(item, dict):
                inner = dump(item, indent + 1).split("\n")
                first = inner[0].strip()
                lines.append(f"{pad}- {first}")
                lines.extend(inner[1:])
            else:
                lines.append(f"{pad}- {scalar(item)}")
        return "\n".join(lines)

    def scalar(item):
        if item is None:
            return "null"
        if item is True:
            return "true"
        if item is False:
            return "false"
        if isinstance(item, (int, float)):
            return str(item)
        if isinstance(item, list):
            return "[" + ", ".join(scalar(x) for x in item) + "]"
        return json.dumps(item, ensure_ascii=False)

    docs = ["proxies:\n" + dump(proxies, 1)]
    for proxy in proxies:
        docs.append("proxies:\n" + dump([proxy], 1))
    groups = {"proxy-groups": [{"name": "auto", "type": "url-test", "proxies": ["c-wg", "c-awg"],
                                "url": "https://www.gstatic.com/generate_204", "interval": 300}]}
    docs.append("proxies:\n" + dump(proxies[14:17], 1) + "\n" + dump(groups))
    docs.append(json.dumps({"proxies": proxies[:5]}))
    docs.append("proxies: []\n")
    docs.append("proxies:\n  - {name: flow, type: trojan, server: f.example, port: 443, password: pw}\n")
    return docs


def json_documents() -> list[str]:
    singbox = {"outbounds": [
        {"type": "vless", "tag": "sb-vless", "server": "j.example", "server_port": 443, "uuid": UUID,
         "flow": "xtls-rprx-vision", "tls": {"enabled": True, "server_name": "s.example",
                                              "reality": {"enabled": True, "public_key": PBK, "short_id": "ab"},
                                              "utls": {"enabled": True, "fingerprint": "chrome"}}},
        {"type": "vmess", "tag": "sb-vmess", "server": "j.example", "server_port": 443, "uuid": UUID,
         "security": "auto", "transport": {"type": "ws", "path": "/ws", "headers": {"Host": "h.example"}}},
        {"type": "trojan", "tag": "sb-trojan", "server": "j.example", "server_port": 443, "password": "pw",
         "tls": {"enabled": True, "insecure": True}},
        {"type": "shadowsocks", "tag": "sb-ss", "server": "j.example", "server_port": 8388,
         "method": "2022-blake3-aes-128-gcm", "password": base64.b64encode(b"k" * 16).decode()},
        {"type": "hysteria2", "tag": "sb-hy2", "server": "j.example", "server_port": 443, "password": "pw"},
        {"type": "tuic", "tag": "sb-tuic", "server": "j.example", "server_port": 443, "uuid": UUID, "password": "pw"},
        {"type": "wireguard", "tag": "sb-wg-legacy", "server": "j.example", "server_port": 51820,
         "local_address": ["10.0.0.2/32"], "private_key": WG_PRIV, "peer_public_key": WG_PUB},
        {"type": "urltest", "tag": "auto", "outbounds": ["sb-vless", "sb-vmess"]},
        {"type": "direct", "tag": "direct"}, {"type": "block", "tag": "block"},
    ], "endpoints": [
        {"type": "wireguard", "tag": "sb-wg", "address": ["10.0.0.2/32"], "private_key": WG_PRIV,
         "peers": [{"address": "j.example", "port": 51820, "public_key": WG_PUB, "allowed_ips": ["0.0.0.0/0"]}]},
    ]}
    xray = {"outbounds": [
        {"protocol": "vless", "tag": "proxy", "settings": {"vnext": [{"address": "x.example", "port": 443,
                                                                    "users": [{"id": UUID, "encryption": "none",
                                                                               "flow": "xtls-rprx-vision"}]}]},
         "streamSettings": {"network": "tcp", "security": "reality",
                            "realitySettings": {"serverName": "s.example", "publicKey": PBK, "shortId": "ab",
                                                "fingerprint": "chrome"}}},
        {"protocol": "freedom", "tag": "direct"},
    ]}
    xray_xhttp = {"outbounds": [
        {"protocol": "vless", "tag": "proxy", "settings": {"vnext": [{"address": "x.example", "port": 443,
                                                                    "users": [{"id": UUID, "encryption": "none"}]}]},
         "streamSettings": {"network": "xhttp", "security": "tls", "tlsSettings": {"serverName": "s.example"},
                            "xhttpSettings": {"path": "/x", "mode": "auto",
                                              "extra": {"downloadSettings": {"address": "d.example", "port": 443,
                                                                             "network": "xhttp",
                                                                             "xhttpSettings": {"path": "/d"}}}}}},
    ]}
    sip008 = {"version": 1, "servers": [
        {"id": "1", "remarks": "sip1", "server": "s.example", "server_port": 8388, "password": "pw",
         "method": "aes-256-gcm"},
        {"id": "2", "remarks": "sip2", "server": "s.example", "server_port": 8389, "password": "pw",
         "method": "chacha20-ietf-poly1305", "plugin": "obfs-local", "plugin_opts": "obfs=http"},
    ]}
    naive = {"listen": "socks://127.0.0.1:1080", "proxy": "https://u:p@n.example"}
    docs = [json.dumps(singbox), json.dumps(xray), json.dumps(xray_xhttp), json.dumps(sip008), json.dumps(naive),
            json.dumps([xray, xray]), json.dumps([singbox["outbounds"][0]]), json.dumps(singbox["outbounds"][0]),
            json.dumps(xray["outbounds"][0]), json.dumps({"remarks": "named", **xray}),
            json.dumps({"outbounds": []}), "{not json", "[1, 2, 3]", json.dumps({"servers": []})]
    for outbound in singbox["outbounds"][:7] + singbox["endpoints"]:
        docs.append(json.dumps(outbound))
    return docs


def containers(links: list[str]) -> list[str]:
    sample = "\n".join(links[:12])
    return [
        b64(sample), b64(sample, urlsafe=True, pad=False), b64(sample.replace("\n", "\r\n")),
        "﻿" + sample, sample.replace("\n", "\r\n"), sample.replace("\n", " "),
        "# comment\n" + sample + "\n\n// another\n", sample + "\nnot-a-link\nftp://x.example\n",
        b64(b64(sample)), "\n".join(links[:3]) + "\n" + b64("\n".join(links[3:6])),
        "", "   ", "vless://", "://", "unknown://x", "https://example.com/sub?token=1",
    ]


def synthetic_inputs() -> list[tuple[str, str]]:
    links = (vless_links() + vmess_links() + trojan_links() + ss_links() + simple_proxy_links()
             + quic_family_links() + wireguard_links())
    items: list[tuple[str, str]] = []
    for link in links:
        items.append(("parse_single", link))
        items.append(("parse_links_text", link))
    for doc in wg_configs() + clash_documents() + json_documents() + containers(links):
        items.append(("parse_links_text", doc))
        items.append(("parse_single", doc))
    return items


def node_payload(node: Node) -> dict:
    data = node.to_dict()
    data.pop("id", None)
    data.pop("created_at", None)
    validate_copy = copy.deepcopy(node)
    try:
        data["_validate"] = link_parser.validate_node_outbound(validate_copy)
    except Exception as exc:  # noqa: BLE001 - recorded as data
        data["_validate"] = f"!{type(exc).__name__}: {exc}"
    normalized = copy.deepcopy(node)
    try:
        changed = link_parser.normalize_node_outbound(normalized)
        data["_normalized"] = {"changed": bool(changed), "outbound": normalized.outbound}
    except Exception as exc:  # noqa: BLE001
        data["_normalized"] = {"error": f"{type(exc).__name__}: {exc}"}
    return data


def replay(fn: str, text: str) -> dict:
    if fn == "parse_single":
        try:
            node = link_parser.parse_single(text)
        except link_parser.LinkParseError as exc:
            return {"error": str(exc)}
        except Exception as exc:  # noqa: BLE001
            return {"error": str(exc), "python_exception": type(exc).__name__}
        return {"node": node_payload(node)}
    nodes, errors = link_parser.parse_links_text(text)
    return {"nodes": [node_payload(node) for node in nodes], "errors": list(errors)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--recorded", type=Path)
    args = parser.parse_args()
    items: list[tuple[str, str, str]] = []
    if args.recorded:
        for line in args.recorded.read_text(encoding="utf-8").splitlines():
            entry = json.loads(line)
            items.append(("recorded", entry["fn"], entry["text"]))
    items += [("synthetic", fn, text) for fn, text in synthetic_inputs()]
    seen: set[tuple[str, str]] = set()
    cases = []
    skipped = 0
    for origin, fn, text in items:
        if (fn, text) in seen:
            continue
        seen.add((fn, text))
        if len(text) > MAX_INPUT_CHARS:
            skipped += 1
            continue
        result = replay(fn, text)
        # Nodes embed parse-time details that must not depend on the run.
        cases.append({"origin": origin, "fn": fn, "input": text, "expected": result})
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(json.dumps({"cases": cases}, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(cases)} cases ({skipped} oversized inputs skipped) -> {FIXTURE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
