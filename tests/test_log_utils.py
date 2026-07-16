from __future__ import annotations

from xray_fluent.log_utils import clean_log_text, parse_log_line
from xray_fluent.qml_app.bridge.log_model import LogFilterModel, LogModel


def test_singbox_ansi_and_trace_prefix_are_removed() -> None:
    entry = parse_log_line(
        "[singbox] \x1b[31m+0300 2026-06-22 12:00:00 ERROR\x1b[0m [2503566513] 7ms dns: exchange failed: timeout"
    )
    assert entry.source == "sing-box"
    assert entry.level == "error"
    assert "\x1b" not in entry.message
    assert "2503566513" not in entry.message
    assert "DNS" in entry.message


def test_port_conflict_gets_human_message_and_action() -> None:
    entry = parse_log_line("[xray-error] SOCKS порт 10808 уже занят процессом clash.exe (PID 42).")
    assert entry.level == "error"
    assert entry.message == "Локальный порт 10808 уже используется другой программой."
    assert entry.action_id == "change-port:10808"
    assert entry.action_label == "Сменить порт"


def test_control_characters_are_removed() -> None:
    assert clean_log_text("abc\x00\x07 def") == "abc def"


def test_expected_core_exit_is_not_reported_as_error() -> None:
    entry = parse_log_line("[xray] process stopped with code 0")
    assert entry.level == "success"
    assert entry.message == "process stopped with code 0"


def test_nonzero_core_exit_is_humanized() -> None:
    entry = parse_log_line("[singbox] process stopped with code 1")
    assert entry.level == "error"
    assert entry.message == "Сетевое ядро неожиданно остановилось."


def test_wintun_element_not_found_is_not_misreported_as_missing_core() -> None:
    entry = parse_log_line(
        "[singbox-error] sing-box exited during startup: FATAL start inbound/tun: "
        "configure tun interface: set ipv6 address: Element not found."
    )

    assert entry.message == "Сетевое ядро неожиданно остановилось."
    assert "Файл сетевого ядра не найден" not in entry.message


def test_uninitialized_warp_endpoint_has_specific_message() -> None:
    entry = parse_log_line(
        "[singbox] ERROR router: process DNS packet: endpoint not initialized"
    )

    assert entry.message == "Не удалось инициализировать подключение WARP/MASQUE."


def test_log_proxy_filters_by_level_and_search() -> None:
    source = LogModel()
    proxy = LogFilterModel(source)
    source.append_line("[xray] connected")
    source.append_line("[singbox] FATAL failed to start")
    source.append_line("[tun] WARN deprecated option")
    source.append_line("[core] config loaded")
    assert proxy.rowCount() == 4
    proxy.setLevelFilter("error")
    assert proxy.rowCount() == 1
    proxy.setLevelFilter("all")
    proxy.setSearchText("deprecated")
    assert proxy.rowCount() == 1
    proxy.setSearchText("")
    proxy.setLevelFilter("success")
    assert proxy.rowCount() == 1
    proxy.setLevelFilter("info")
    assert proxy.rowCount() == 1


def test_zapret_log_levels() -> None:
    from xray_fluent.log_utils import classify_log_level
    assert classify_log_level("[zapret] ERROR: windivert: access denied") == "error"
    assert classify_log_level("[zapret] WARN: deprecated option") == "warning"
    assert classify_log_level("[zapret] Перезапуск текущего пресета") == "info"


def test_collect_network_context() -> None:
    from xray_fluent.diagnostics import collect_network_context
    net = collect_network_context()
    assert isinstance(net, dict)
    assert "ipv4_internet" in net
    assert "ipv6_internet" in net
    assert "system_dns" in net
    assert isinstance(net["system_dns"], list)
    assert "proxy_info" in net
    assert isinstance(net["proxy_info"], dict)
    assert "env_proxies" in net["proxy_info"]
    assert "connected_adapters" in net
    assert isinstance(net["connected_adapters"], list)


def test_connection_errors_are_warnings() -> None:
    from xray_fluent.log_utils import classify_log_level
    msg1 = "[singbox] ERROR [4263401339 226ms] connection: open connection to 142.251.9.188:5228 using outbound/trojan[proxy]: unexpected HTTP response status: 502"
    msg2 = "dial tcp 127.0.0.1:10808: connectex: A connection attempt failed..."
    msg3 = "tls: handshake failed"
    assert classify_log_level(msg1) == "warning"
    assert classify_log_level(msg2) == "warning"
    assert classify_log_level(msg3) == "warning"


def test_expected_core_exit_with_nonzero_code_is_not_error() -> None:
    from xray_fluent.log_utils import classify_log_level
    assert classify_log_level("[singbox] process stopped with code 1 (expected)") == "success"
    assert classify_log_level("[xray] process stopped with code -1 (expected)") == "success"


def test_diagnostic_filter_ignores_connection_errors() -> None:
    import logging
    from xray_fluent.logging_setup import _DiagnosticFilter
    f = _DiagnosticFilter()
    rec1 = logging.LogRecord("xray_fluent", logging.WARNING, "", 0, "unexpected HTTP response status: 502", (), None)
    rec2 = logging.LogRecord("xray_fluent", logging.WARNING, "", 0, "Normal application warning", (), None)
    assert f.filter(rec1) is False
    assert f.filter(rec2) is True



