import json

import pytest

from xray_fluent.secret_scrubber import scrub_text


REQUEST_ID = "7a8a8892-f533-4c0e-a8ce-6158056ee5ad"
SECRET_ID = "11111111-2222-4333-8444-555555555555"


@pytest.mark.parametrize("key", ["request_id", "trace_id", "correlation_id"])
@pytest.mark.parametrize("value", [REQUEST_ID, REQUEST_ID.replace("-", "")])
def test_labelled_technical_ids_survive_repeated_scrubbing(key, value):
    for source in (f'[app] {key}="{value}", uuid={SECRET_ID}',
                   json.dumps({key: value, "uuid": SECRET_ID, "password": "private-password"})):
        for _ in range(3):
            source = scrub_text(source)
            assert value in source
            assert SECRET_ID not in source
            assert "private-password" not in source


@pytest.mark.parametrize("source", [
    SECRET_ID, f"node_id={SECRET_ID}", f"install_id={SECRET_ID}",
    f'password="request_id={SECRET_ID}"',
    f"vless://{SECRET_ID}@private.example?trace_id={SECRET_ID}",
    json.dumps({"password": {"request_id": SECRET_ID}}),
    json.dumps({"request_id": f"private-{SECRET_ID}"}),
    f"private_request_id={SECRET_ID}",
])
def test_connection_and_unlabelled_ids_remain_hidden(source):
    assert SECRET_ID not in scrub_text(source)
