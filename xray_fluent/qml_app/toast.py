"""Modern Windows toast notifications via WinRT (PowerShell bridge)"""
from __future__ import annotations

import base64
import subprocess
import sys

AUMID = "Lumen.LumenKVN"


def _xml_escape(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _ps_quote(text: str) -> str:
    return text.replace("'", "''")


def show_toast(title: str, message: str, aumid: str = AUMID) -> bool:
    """Raise a modern Windows toast. Returns True if the launch was dispatched"""
    if sys.platform != "win32":
        return False
    try:
        xml = (
            "<toast activationType='protocol' launch='lumen-kvn:show'>"
            "<visual><binding template='ToastGeneric'>"
            f"<text>{_xml_escape(title)}</text>"
            f"<text>{_xml_escape(message)}</text>"
            "</binding></visual></toast>"
        )
        script = "\n".join(
            [
                "$ErrorActionPreference='Stop'",
                "[void][Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]",
                "[void][Windows.UI.Notifications.ToastNotification,Windows.UI.Notifications,ContentType=WindowsRuntime]",
                "[void][Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom,ContentType=WindowsRuntime]",
                "$doc=New-Object Windows.Data.Xml.Dom.XmlDocument",
                f"$doc.LoadXml('{_ps_quote(xml)}')",
                "$toast=New-Object Windows.UI.Notifications.ToastNotification -ArgumentList $doc",
                f"[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{_ps_quote(aumid)}').Show($toast)",
            ]
        )
        encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
        subprocess.Popen(
            [
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                encoded,
            ],
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL,
        )
        return True
    except Exception:
        return False
