from __future__ import annotations

from copy import deepcopy
from typing import Any


def _clean_fragment_value(value: str, fallback: str) -> str:
    cleaned = str(value or "").strip()
    return cleaned or fallback


def build_fragment_mask(
    *,
    packets: str = "tlshello",
    length: str = "50-100",
    delay: str = "10-20",
) -> dict[str, Any]:
    return {
        "type": "fragment",
        "settings": {
            "packets": _clean_fragment_value(packets, "tlshello"),
            "length": _clean_fragment_value(length, "50-100"),
            "delay": _clean_fragment_value(delay, "10-20"),
        },
    }


def build_noise_mask() -> dict[str, Any]:
    return {
        "type": "noise",
        "settings": {
            "length": "10-20",
            "delay": "10-16",
        },
    }


def build_finalmask(
    *,
    packets: str = "tlshello",
    length: str = "50-100",
    delay: str = "10-20",
    tail_fragment: bool = False,
) -> dict[str, Any]:
    tcp = [build_fragment_mask(packets=packets, length=length, delay=delay)]
    if tail_fragment:
        tcp.append(build_fragment_mask(packets="1-1", length="1-3", delay="0-0"))
    return {"tcp": tcp, "udp": [build_noise_mask()]}


def ensure_stream_finalmask(
    stream_settings: dict[str, Any],
    *,
    packets: str = "tlshello",
    length: str = "50-100",
    delay: str = "10-20",
    tail_fragment: bool = False,
) -> bool:
    changed = False
    finalmask = stream_settings.get("finalmask")
    if not isinstance(finalmask, dict):
        finalmask = {}
        stream_settings["finalmask"] = finalmask
        changed = True

    tcp = finalmask.get("tcp")
    if not isinstance(tcp, list) or not tcp:
        finalmask["tcp"] = build_finalmask(
            packets=packets,
            length=length,
            delay=delay,
            tail_fragment=tail_fragment,
        )["tcp"]
        changed = True

    udp = finalmask.get("udp")
    if not isinstance(udp, list) or not udp:
        finalmask["udp"] = [build_noise_mask()]
        changed = True

    return changed


def apply_xray_outbound_fragment(
    payload: dict[str, Any],
    *,
    packets: str = "tlshello",
    length: str = "50-100",
    delay: str = "10-20",
    tail_fragment: bool = False,
) -> int:
    """Add Xray finalmask to TLS-like proxy outbounds, like v2rayN EnableFragment."""
    patched = 0
    outbounds = payload.get("outbounds")
    if not isinstance(outbounds, list):
        return 0
    for outbound in outbounds:
        if not isinstance(outbound, dict):
            continue
        stream_settings = outbound.get("streamSettings")
        if not isinstance(stream_settings, dict):
            continue
        if not str(stream_settings.get("security") or "").strip():
            continue
        sockopt = stream_settings.get("sockopt")
        if isinstance(sockopt, dict) and str(sockopt.get("dialerProxy") or "").strip():
            continue
        if ensure_stream_finalmask(
            stream_settings,
            packets=packets,
            length=length,
            delay=delay,
            tail_fragment=tail_fragment,
        ):
            patched += 1
    return patched


def apply_xray_final_fragment(
    payload: dict[str, Any],
    *,
    tag_prefix: str = "proxy",
    packets: str = "tlshello",
    length: str = "50-100",
    delay: str = "10-20",
    tail_fragment: bool = False,
) -> int:
    """Insert a v2rayN-style freedom finalmask wrapper before proxy outbounds.

    Xray sees this as:
      inbound -> outbound tag `proxy` (freedom + finalmask)
              -> dialerProxy `fragment-proxy` (the original proxy outbound)

    That fragments the target site's TLS handshake without hardcoding any domain.
    """
    outbounds = payload.get("outbounds")
    if not isinstance(outbounds, list):
        return 0

    existing_tags = {
        str(outbound.get("tag") or "")
        for outbound in outbounds
        if isinstance(outbound, dict)
    }
    patched = 0

    index = 0
    while index < len(outbounds):
        outbound = outbounds[index]
        if not isinstance(outbound, dict):
            index += 1
            continue

        original_tag = str(outbound.get("tag") or "").strip()
        if not original_tag.startswith(tag_prefix):
            index += 1
            continue
        if original_tag.startswith("fragment-"):
            index += 1
            continue

        stream_settings = outbound.get("streamSettings")
        sockopt = stream_settings.get("sockopt") if isinstance(stream_settings, dict) else None
        if isinstance(sockopt, dict) and str(sockopt.get("dialerProxy") or "").startswith("fragment-"):
            index += 1
            continue

        after_tag = f"fragment-{original_tag}"
        if after_tag in existing_tags:
            index += 1
            continue

        wrapper = {
            "tag": original_tag,
            "protocol": "freedom",
            "streamSettings": {
                "finalmask": deepcopy(
                    build_finalmask(
                        packets=packets,
                        length=length,
                        delay=delay,
                        tail_fragment=tail_fragment,
                    )
                ),
                "sockopt": {
                    "dialerProxy": after_tag,
                },
            },
        }
        outbound["tag"] = after_tag
        outbounds.insert(index, wrapper)
        existing_tags.add(after_tag)
        patched += 1
        index += 2

    return patched
