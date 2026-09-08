"""All archives, paths, state and transport in these tests are synthetic."""
import io
import json
from types import SimpleNamespace
import zipfile

import pytest

from xray_fluent import diagnostics as diag, diagnostics_uploader as uploader


def _archive(sections, entries):
    output = io.BytesIO()
    with zipfile.ZipFile(output, 'w') as archive:
        archive.comment = b'private-comment'
        archive.writestr('meta.json', json.dumps({'sections': sections, 'hostname': 'private-host'}))
        for name, value in entries.items():
            archive.writestr(name, value)
    return output.getvalue()


def test_zip_is_rebuilt_from_selected_allowlist():
    data = _archive(['app'], {'logs/app.log': 'Bearer private-bearer',
        'logs/core.log': 'unselected', 'state.json': 'private-state',
        '../escape.txt': 'private-path', 'logs/arbitrary.log': 'private-extra'})
    with zipfile.ZipFile(io.BytesIO(diag.sanitize_diagnostic_zip(data))) as archive:
        assert set(archive.namelist()) == {'meta.json', 'logs/app.log'}
        assert archive.comment == b''
        assert b'private-' not in b''.join(archive.read(name) for name in archive.namelist())


@pytest.mark.parametrize('flags,expected', [({}, set()), ({'app': 'true'}, set()),
    ({'app': True}, {'logs/app.log'}), ({'recent': True}, {'recent_logs.txt'}),
    ({'state': True}, {'state_redacted.json'})])
def test_export_include_flags_do_not_expand_access(tmp_path, monkeypatch, flags, expected):
    log_dir = tmp_path / 'logs'
    log_dir.mkdir()
    for name in ('app.log', 'core.log', 'faulthandler.log', 'arbitrary.log'):
        (log_dir / name).write_text('token=private-file', encoding='utf-8')
    monkeypatch.setattr(diag, 'LOG_DIR', log_dir)
    monkeypatch.setattr(diag, 'collect_network_context', lambda: pytest.fail('network not selected'))
    state = SimpleNamespace(to_dict=lambda: {'settings': {'token': 'private-setting'},
                                             'nodes': [{'name': 'private-node'}]})
    path = diag.export_diagnostics(tmp_path / 'out.zip', state, ['Bearer private-recent'], flags)
    with zipfile.ZipFile(path) as archive:
        assert set(archive.namelist()) == expected | {'meta.json'}
        assert b'private-' not in b''.join(archive.read(name) for name in archive.namelist())


def test_old_epoch_cannot_send_after_off_on(monkeypatch):
    monkeypatch.setattr(uploader.urllib.request, 'build_opener', lambda *a: pytest.fail('dispatch'))
    old = uploader.set_uploads_enabled(True)
    uploader.set_uploads_enabled(False)
    current = uploader.set_uploads_enabled(True)
    try:
        assert current != old and not uploader.uploads_allowed(old)
        assert not uploader._send('https://synthetic.invalid', b'{}', 'application/json', 1, epoch=old)
    finally:
        uploader.set_uploads_enabled(False)


def test_delayed_bundle_cannot_rebind_to_new_consent(tmp_path, monkeypatch):
    path = tmp_path / 'old.zip'
    path.write_bytes(_archive([], {}))
    monkeypatch.setattr(uploader, '_send', lambda *a, **k: pytest.fail('old bundle dispatch'))
    old = uploader.set_uploads_enabled(True)
    uploader.mark_bundle_generation(path, old)
    uploader.set_uploads_enabled(False)
    current = uploader.set_uploads_enabled(True)
    try:
        uploader.upload_bundle('https://synthetic.invalid', path, epoch=old)
        uploader.upload_bundle('https://synthetic.invalid', path, epoch=current)
        uploader.upload_bundle('https://synthetic.invalid', path)
        uploader.wait_for_bundle_uploads(1)
        assert not uploader._bundle_threads
    finally:
        uploader.set_uploads_enabled(False)


@pytest.mark.parametrize('cancel_at', [1, 3])
def test_cancelled_export_leaves_existing_file_untouched(tmp_path, monkeypatch, cancel_at):
    monkeypatch.setattr(diag, 'LOG_DIR', tmp_path / 'logs')
    path = tmp_path / 'existing.zip'
    path.write_bytes(b'previous synthetic data')
    calls = 0
    def cancelled():
        nonlocal calls
        calls += 1
        return calls >= cancel_at
    with pytest.raises(diag.DiagnosticsCancelled):
        diag.export_diagnostics(path, SimpleNamespace(), [], {}, cancelled=cancelled)
    assert path.read_bytes() == b'previous synthetic data'
    assert not list(tmp_path.glob('.lumen-diagnostic-*'))


def test_duplicate_archive_names_fail_closed():
    output = io.BytesIO(_archive(['app'], {'logs/app.log': 'safe'}))
    with zipfile.ZipFile(output, 'a') as archive:
        with pytest.warns(UserWarning, match='Duplicate name'):
            archive.writestr('logs/app.log', 'private-duplicate')
    with pytest.raises(ValueError, match='members'):
        diag.sanitize_diagnostic_zip(output.getvalue())
