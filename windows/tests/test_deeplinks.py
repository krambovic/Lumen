from __future__ import annotations

from urllib.parse import quote

import pytest

from xray_fluent.deeplinks import (
    DeepLinkError,
    decode_instance_message,
    encode_instance_message,
    find_lumen_deep_link,
    parse_lumen_deep_link,
)
from xray_fluent.qml_app import main_qml


def test_parse_canonical_subscription_deep_link() -> None:
    target = "https://example.com/sub?token=secret&client=lumen"
    link = f"lumen://add?url={quote(target, safe='')}&name={quote('My VPN')}"

    request = parse_lumen_deep_link(link)

    assert request is not None
    assert request.url == target
    assert request.name == "My VPN"


@pytest.mark.parametrize(
    "link",
    [
        "lumen://import?url=https%3A%2F%2Fexample.com%2Fsub",
        "lumen://subscribe/https%3A%2F%2Fexample.com%2Fsub",
        "lumen://add/https://example.com/sub",
        "lumen://subscription/add?subscription=happ%3A%2F%2Fcrypt%2Fpayload",
        "lumen:install-config?link=https%3A%2F%2Fexample.com%2Fsub",
    ],
)
def test_parse_supported_deep_link_aliases(link: str) -> None:
    request = parse_lumen_deep_link(link)
    assert request is not None
    assert request.url in {"https://example.com/sub", "happ://crypt/payload"}


def test_bare_lumen_link_only_activates_application() -> None:
    assert parse_lumen_deep_link("lumen://") is None


@pytest.mark.parametrize(
    "link",
    [
        "lumen://add?url=file%3A%2F%2FC%3A%2Fsecret.txt",
        "lumen://add?url=javascript%3Aalert%281%29",
        "lumen://unknown?url=https%3A%2F%2Fexample.com",
    ],
)
def test_rejects_unsafe_or_unknown_deep_links(link: str) -> None:
    with pytest.raises(DeepLinkError):
        parse_lumen_deep_link(link)


def test_single_instance_message_preserves_deep_link() -> None:
    link = "lumen://add?url=https%3A%2F%2Fexample.com%2Fsub"
    payload = encode_instance_message(["Lumen.exe", link])

    assert find_lumen_deep_link(["Lumen.exe", link]) == link
    assert decode_instance_message(payload) == link
    assert decode_instance_message(b"activate") == ""


def test_source_run_registers_deep_links_through_run_qml(monkeypatch) -> None:
    monkeypatch.setattr(main_qml.sys, "frozen", False, raising=False)

    launch = main_qml._protocol_launch_command()

    assert launch is not None
    command, _icon = launch
    assert "run_qml.py" in command
    assert command.endswith('" "%1"')
