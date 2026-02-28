from __future__ import annotations

import json
import sys
from typing import Any, Protocol, Sequence, TextIO


class EmbeddingRuntime(Protocol):
    ready: bool
    dims: int

    def metadata(self) -> dict[str, object]: ...

    def embed_query(self, text: str) -> list[float]: ...

    def embed_batch(self, texts: Sequence[str]) -> list[list[float]]: ...


SIDECAR_PROTOCOL_SCHEMA = "julie.embedding.sidecar"
SIDECAR_PROTOCOL_VERSION = 1


def _success_response(request_id: str, result: object) -> dict[str, object]:
    return {
        "schema": SIDECAR_PROTOCOL_SCHEMA,
        "version": SIDECAR_PROTOCOL_VERSION,
        "request_id": request_id,
        "result": result,
    }


def _error_response(request_id: str, code: str, message: str) -> dict[str, object]:
    return {
        "schema": SIDECAR_PROTOCOL_SCHEMA,
        "version": SIDECAR_PROTOCOL_VERSION,
        "request_id": request_id,
        "error": {
            "code": code,
            "message": message,
        },
    }


def _extract_request_id(request: dict[str, Any]) -> tuple[str, str | None]:
    if "request_id" in request:
        value = request["request_id"]
        if isinstance(value, str):
            return value, None
        return "", "request_id must be a string"

    if "id" in request:
        value = request["id"]
        if isinstance(value, str):
            return value, None
        return "", "id must be a string"

    return "", None


def handle_health(runtime: EmbeddingRuntime) -> dict[str, object]:
    data = runtime.metadata()
    data["ready"] = runtime.ready
    return data


def handle_embed_query(runtime: EmbeddingRuntime, text: str) -> dict[str, object]:
    return {
        "dims": runtime.dims,
        "vector": runtime.embed_query(text),
    }


def handle_embed_batch(
    runtime: EmbeddingRuntime, texts: list[str]
) -> dict[str, object]:
    return {
        "dims": runtime.dims,
        "vectors": runtime.embed_batch(texts),
    }


def dispatch_request(
    runtime: EmbeddingRuntime, request: dict[str, Any]
) -> dict[str, Any]:
    if not isinstance(request, dict):
        return _error_response("", "invalid_request", "request must be an object")

    request_id, request_id_error = _extract_request_id(request)
    if request_id_error is not None:
        return _error_response(request_id, "invalid_request", request_id_error)

    method = request.get("method")
    if not isinstance(method, str):
        return _error_response(request_id, "invalid_request", "method must be a string")

    params_raw = request.get("params", {})
    if not isinstance(params_raw, dict):
        return _error_response(
            request_id, "invalid_request", "params must be an object"
        )
    params = params_raw

    if method == "health":
        return _success_response(request_id, handle_health(runtime))

    if method == "embed_query":
        text = params.get("text")
        if not isinstance(text, str):
            return _error_response(
                request_id,
                "invalid_request",
                "embed_query params.text must be a string",
            )
        return _success_response(request_id, handle_embed_query(runtime, text))

    if method == "embed_batch":
        texts = params.get("texts")
        if not isinstance(texts, list):
            return _error_response(
                request_id,
                "invalid_request",
                "embed_batch params.texts must be an array",
            )
        if not all(isinstance(value, str) for value in texts):
            return _error_response(
                request_id,
                "invalid_request",
                "embed_batch params.texts must contain only strings",
            )
        return _success_response(request_id, handle_embed_batch(runtime, texts))

    if method == "shutdown":
        return _success_response(request_id, {"stopping": True})

    return _error_response(request_id, "unknown_method", f"unknown method: {method}")


def run_stdio_loop(
    runtime: EmbeddingRuntime,
    in_stream: TextIO | None = None,
    out_stream: TextIO | None = None,
) -> None:
    reader = in_stream or sys.stdin
    writer = out_stream or sys.stdout

    for line in reader:
        stripped = line.strip()
        if not stripped:
            continue

        request: Any = None

        try:
            request = json.loads(stripped)
            response = dispatch_request(runtime, request)
        except json.JSONDecodeError as exc:
            response = _error_response("", "invalid_json", f"invalid json: {exc.msg}")

        writer.write(json.dumps(response, separators=(",", ":")) + "\n")
        writer.flush()

        if (
            isinstance(request, dict)
            and request.get("method") == "shutdown"
            and "result" in response
            and isinstance(response["result"], dict)
            and response["result"].get("stopping") is True
        ):
            break
