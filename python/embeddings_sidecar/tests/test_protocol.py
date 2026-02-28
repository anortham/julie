import io
import json
from pathlib import Path
import sys
from typing import get_type_hints
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from sidecar.protocol import (
    EmbeddingRuntime,
    SIDECAR_PROTOCOL_SCHEMA,
    SIDECAR_PROTOCOL_VERSION,
    dispatch_request,
    handle_embed_batch,
    handle_health,
    run_stdio_loop,
)
from sidecar.runtime import FakeRuntime


def test_health_returns_ready_runtime_metadata() -> None:
    runtime = FakeRuntime(runtime_name="fake-runtime", device="cpu", dims=384)
    out: dict[str, Any] = handle_health(runtime)

    assert out["ready"] is True
    assert out["runtime"] == "fake-runtime"
    assert out["device"] == "cpu"
    assert out["dims"] == 384


def test_embed_batch_preserves_count_and_dims() -> None:
    runtime = FakeRuntime(dims=384)
    out: dict[str, Any] = handle_embed_batch(runtime, ["a", "b", "c"])

    assert out["dims"] == 384
    assert len(out["vectors"]) == 3
    assert all(len(vector) == 384 for vector in out["vectors"])


def test_response_envelope_roundtrips_request_id() -> None:
    runtime = FakeRuntime()
    response = dispatch_request(
        runtime,
        {"id": "req-123", "method": "health", "params": {}},
    )

    assert response["schema"] == SIDECAR_PROTOCOL_SCHEMA
    assert response["version"] == SIDECAR_PROTOCOL_VERSION
    assert response["request_id"] == "req-123"
    assert "result" in response
    assert "error" not in response


def test_unknown_method_returns_structured_error() -> None:
    runtime = FakeRuntime()
    response = dispatch_request(
        runtime, {"request_id": "r1", "method": "nope", "params": {}}
    )

    assert response["request_id"] == "r1"
    assert "result" not in response
    assert response["error"]["code"] == "unknown_method"
    assert "nope" in response["error"]["message"]


def test_embed_batch_rejects_malformed_params_type() -> None:
    runtime = FakeRuntime()
    response = dispatch_request(
        runtime, {"request_id": "r2", "method": "embed_batch", "params": 123}
    )

    assert response["request_id"] == "r2"
    assert response["error"]["code"] == "invalid_request"
    assert "params" in response["error"]["message"]


def test_loop_stops_after_shutdown() -> None:
    runtime = FakeRuntime()
    input_stream = io.StringIO(
        "\n".join(
            [
                json.dumps({"request_id": "h1", "method": "health", "params": {}}),
                json.dumps({"request_id": "s1", "method": "shutdown", "params": {}}),
                json.dumps({"request_id": "h2", "method": "health", "params": {}}),
            ]
        )
        + "\n"
    )
    output_stream = io.StringIO()

    run_stdio_loop(runtime, input_stream, output_stream)
    lines = [line for line in output_stream.getvalue().splitlines() if line.strip()]
    payloads = [json.loads(line) for line in lines]

    assert len(payloads) == 2
    assert payloads[0]["request_id"] == "h1"
    assert payloads[1]["request_id"] == "s1"
    assert payloads[1]["result"]["stopping"] is True


def test_loop_returns_structured_error_for_malformed_json() -> None:
    runtime = FakeRuntime()
    input_stream = io.StringIO(
        "{\n"
        + json.dumps({"request_id": "h1", "method": "health", "params": {}})
        + "\n"
    )
    output_stream = io.StringIO()

    run_stdio_loop(runtime, input_stream, output_stream)
    lines = [line for line in output_stream.getvalue().splitlines() if line.strip()]
    payloads = [json.loads(line) for line in lines]

    assert len(payloads) == 2
    assert payloads[0]["error"]["code"] == "invalid_json"
    assert payloads[1]["request_id"] == "h1"
    assert payloads[1]["result"]["ready"] is True


def test_protocol_handlers_accept_embedding_runtime_protocol_type() -> None:
    health_hints = get_type_hints(handle_health)
    dispatch_hints = get_type_hints(dispatch_request)

    assert health_hints["runtime"] is EmbeddingRuntime
    assert dispatch_hints["runtime"] is EmbeddingRuntime
