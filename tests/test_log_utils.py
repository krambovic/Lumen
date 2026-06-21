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
    assert entry.level == "info"
    assert entry.message == "process stopped with code 0"


def test_nonzero_core_exit_is_humanized() -> None:
    entry = parse_log_line("[singbox] process stopped with code 1")
    assert entry.level == "error"
    assert entry.message == "Сетевое ядро неожиданно остановилось."


def test_log_proxy_filters_by_level_and_search() -> None:
    source = LogModel()
    proxy = LogFilterModel(source)
    source.append_line("[xray] connected")
    source.append_line("[singbox] FATAL failed to start")
    source.append_line("[tun] WARN deprecated option")
    assert proxy.rowCount() == 3
    proxy.setLevelFilter("error")
    assert proxy.rowCount() == 1
    proxy.setLevelFilter("all")
    proxy.setSearchText("deprecated")
    assert proxy.rowCount() == 1
