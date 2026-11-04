# Android Xray sidecar

Xray-core v26.10.31 (MPL-2.0): https://github.com/XTLS/Xray-core/tree/v26.10.31
The APK includes the license at `assets/licenses/xray.txt`.

`tools/fetch_xray.py` downloads the official Android ARM64/x86_64 releases and
checks their published SHA256 digests. `tools/build_xray_arm.py` builds ARMv7 as
an Android API 26 PIE with NDK 28 and Go 1.27.3. The linker flag
`-checklinkname=0` is required by upstream's Android `anet` dependency.

Binary SHA256:

| ABI | SHA256 |
| --- | --- |
| arm64-v8a | ce8f18c44eb40cbd4563433109e0ede36c38b89c2fe71d1e583b4332ebefb622 |
| armeabi-v7a | c7731798d8ef1521eeb9c55300395e2363bf7857b1ea9f6506295c4fc48faefd |
| x86_64 | 894651c1de4e2b9cb51775c13ed168e8c8148be7220d2b5db15a42362c5b3263 |

Hybrid is the default. Ordinary compatible nodes use sing-box alone. Xray
handles VLESS encryption, REALITY ML-DSA verification, finalMask, ECH and other
settings with no faithful sing-box representation, including full-certificate
`pcs` pins. An unsupported sing-box
conversion can also fall back to Xray for compatible Xray outbounds. This is
capability-based selection, not a claim that every server or legacy transport
will work with either core.

Singbox mode refuses settings that would otherwise be silently lost. Xray mode
uses Xray for supported V2Ray protocols and reports unsupported protocols such
as OpenVPN instead of rewriting them. Sing-box retains routing/DNS and the
Android VPN bridge in all modes. Sidecar SOCKS inbounds listen on loopback and
use per-runtime random credentials. The VPN service owns their lifecycle;
process reaping is scoped to each engine type. HTTP ping uses the same choice.

Existing normalized records recover newly supported extension fields from a
matching original share link. Export compares Xray settings directly, so a
sing-box projection cannot silently drop `finalMask`, XHTTP `extra` or `pqv`.
REALITY `publicKey` is normalized to current Xray's `password` spelling without
altering the key. A missing key produces an actionable incomplete-link error.
Pinned endpoint IPs retain the original TLS server name. Global fragmentation,
multiplexing and TCP socket options are also transferred to the sidecar; source
profile fragmentation and mux overrides take precedence.

For WebSocket, an offered ALPN list that includes HTTP/1.1 is narrowed to that
protocol: the RFC 6455 upgrade cannot run as HTTP/2 or HTTP/3. SPKI-only pinning
is not silently dropped in Xray mode. It requires Singbox or an explicit full
certificate `pcs` pin. A failing full certificate pin remains a failure.

The older extended sing-box encryption parser can mistake a padding segment
that happens to decode as Base64URL for a key (for example, decoding to nine
bytes). Hybrid passes the original encryption expression to Xray; it never
disables the profile's encryption to work around that error.
