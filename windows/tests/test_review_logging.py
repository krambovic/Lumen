"""Synthetic privacy and bounded-worker regression coverage."""
import json
import logging
import sys
import threading

import pytest

from xray_fluent import secret_scrubber as scrubber
from xray_fluent.logging_setup import _AsyncFileHandler, _HumanFormatter, _JsonLinesFormatter


@pytest.mark.parametrize("text", [
    'Bearer sentinel-private-value', 'basic c2VudGluZWwtcHJpdmF0ZS12YWx1ZQ==',
    'Authorization: Bearer sentinel-private-value',
    r'payload={\"password\":\"sentinel-private-value\\\"tail\"}',
    r'payload={"\u0074oken":"sentinel-private-value',
    'payload={"password":"sentinel-private-value',
    r'vless\://sentinel-private-value@synthetic.invalid',
])
def test_auth_and_malformed_fragments_are_scrubbed(text):
    result = scrubber.scrub_text(text)
    assert 'sentinel-private-value' not in result
    assert 'c2VudGluZWwtcHJpdmF0ZS12YWx1ZQ==' not in result


def _record(message):
    return logging.LogRecord('xray_fluent.app', logging.WARNING, __file__, 1, message, (), None)


def test_streaming_pem_hides_separate_lines_and_resumes():
    stream = scrubber.StreamingSecretScrubber()
    assert 'before' in stream.scrub('before -----BEGIN PRIVATE KEY-----')
    assert 'secret-body' not in stream.scrub('secret-body')
    stream.scrub('-----BEGIN RSA PRIVATE KEY-----')
    stream.scrub('-----END RSA PRIVATE KEY-----')
    assert 'still-secret' not in stream.scrub('still-secret')
    assert 'after' in stream.scrub('-----END PRIVATE KEY----- after')
    assert stream.scrub('normal diagnostic') == 'normal diagnostic'


def test_records_hide_pem_before_repeated_sink_processing(monkeypatch):
    monkeypatch.setattr(scrubber, '_RECORD_STREAM', scrubber.StreamingSecretScrubber())
    for line in ('-----BEGIN PRIVATE KEY-----', 'private-body-fragment', '-----END PRIVATE KEY-----'):
        record = _record(line)
        first = scrubber.prepare_record(record)
        second = scrubber.prepare_record(record)
        assert 'private-body-fragment' not in repr((first.__dict__, second.__dict__))
    assert scrubber.prepare_record(_record('resumed')).msg == 'resumed'


def test_unknown_extras_never_stringify_objects():
    class Secret:
        def __str__(self):
            raise AssertionError('must not stringify extra')
    record = _record('ok')
    record.custom = {'nested': Secret(), 'raw': 'private-extra'}
    assert scrubber.prepare_record(record).custom == '[REDACTED FIELD]'


def test_tracebacks_stack_and_json_formatter_are_private():
    record = _record('Bearer message-secret')
    try:
        raise ValueError('Authorization: Bearer exception-secret')
    except ValueError:
        record.exc_info = sys.exc_info()
    record.stack_info = 'password=stack-secret'
    record.filename = 'https://user:filename-secret@synthetic.invalid/x'
    for formatter in (_HumanFormatter('%(message)s'), _JsonLinesFormatter()):
        text = formatter.format(record)
        assert not any(secret in text for secret in
                       ('message-secret', 'exception-secret', 'stack-secret', 'filename-secret'))
    assert record.exc_info is None and record.args == ()
    assert json.loads(_JsonLinesFormatter().format(record))['msg']


class _BlockedSink:
    level = logging.DEBUG
    def __init__(self):
        self.entered, self.release = threading.Event(), threading.Event()
        self.records, self.threads = [], []
    def handle(self, record):
        self.entered.set()
        assert self.release.wait(3)
        self.records.append(record)
        self.threads.append(threading.get_ident())
    def close(self):
        self.closed_on = threading.get_ident()


def test_file_queue_is_bounded_redacted_and_worker_owned():
    sink = _BlockedSink()
    handler = _AsyncFileHandler([sink], capacity=2)
    caller = threading.get_ident()
    try:
        record = _record('Bearer private-queued')
        handler.emit(record)
        assert sink.entered.wait(1)
        record.msg = 'mutated-original'
        handler.emit(_record('second'))
        handler.emit(_record('third'))
        handler.emit(_record('overflow'))
        assert handler.stats == {'accepted': 3, 'completed': 0, 'queued': 2,
                                 'dropped': 1, 'sink_failures': 0}
        assert not handler.flush(0)
        sink.release.set()
        assert handler.flush(2)
        assert len(sink.records) == 3
        assert 'private-queued' not in sink.records[0].msg
        assert sink.records[0].msg != 'mutated-original'
        assert [item.msg for item in sink.records[1:]] == ['second', 'third']
        assert all(ident != caller for ident in sink.threads)
    finally:
        sink.release.set()
        handler.close(timeout=2)
    assert not handler._thread.is_alive() and sink.closed_on != caller


@pytest.mark.parametrize('text', [
    'payload={"authorization":"Bearer "sentinel-quote-tail""}',
    r'payload={"token":\u0022first,sentinel-quote-tail\u0022}',
    r'payload={\"token\":\"first\\\"sentinel-quote-tail\"}',
    'Bearer "sentinel-quote-tail"',
])
def test_malformed_auth_quotes_never_expose_value_tails(text):
    assert 'sentinel-quote-tail' not in scrubber.scrub_text(text)


def test_failed_sink_does_not_starve_other_sinks():
    class Failing:
        level = logging.DEBUG
        def handle(self, record):
            raise OSError('synthetic write error')
        def close(self):
            pass
    healthy = _BlockedSink()
    healthy.release.set()
    handler = _AsyncFileHandler([Failing(), healthy])
    try:
        handler.emit(_record('kept'))
        assert handler.flush(2)
        assert handler.stats['sink_failures'] == 1
        assert healthy.records[0].msg == 'kept'
    finally:
        handler.close(timeout=2)


def test_streaming_scan_precedes_output_bound():
    stream = scrubber.StreamingSecretScrubber()
    first = stream.scrub('x' * 20000 + '-----BEGIN PRIVATE KEY-----')
    assert len(first) <= 16400 and '[TRUNCATED]' in first
    assert 'private-after-truncation' not in stream.scrub('private-after-truncation')
    stream.scrub('-----END PRIVATE KEY-----')
    assert stream.scrub('normal') == 'normal'
