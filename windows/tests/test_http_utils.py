import ssl

from xray_fluent.http_utils import _make_ssl_context


def test_ssl_context_keeps_certificate_verification_enabled() -> None:
    context = _make_ssl_context()
    assert context.check_hostname is True
    assert context.verify_mode == ssl.CERT_REQUIRED
