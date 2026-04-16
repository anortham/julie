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
    data = dict(runtime.metadata())
    data["ready"] = runtime.ready
    _validate_health_metadata(data)
    return data


def _validate_health_metadata(data: dict[str, object]) -> None:
    capabilities = data.get("capabilities")
    if not isinstance(capabilities, dict):
        raise ValueError("health metadata.capabilities must be an object")

    for backend in ("cpu", "cuda", "directml", "mps"):
        backend_capability = capabilities.get(backend)
        if not isinstance(backend_capability, dict):
            raise ValueError(f"health metadata.capabilities.{backend} must be an object")
        available = backend_capability.get("available")
        if not isinstance(available, bool):
            raise ValueError(
                f"health metadata.capabilities.{backend}.available must be a boolean"
            )

    load_policy = data.get("load_policy")
    if not isinstance(load_policy, dict):
        raise ValueError("health metadata.load_policy must be an object")

    requested = load_policy.get("requested_device_backend")
    resolved = load_policy.get("resolved_device_backend")
    if not isinstance(requested, str) or not requested:
        raise ValueError(
            "health metadata.load_policy.requested_device_backend must be a non-empty string"
        )
    if not isinstance(resolved, str) or not resolved:
        raise ValueError(
            "health metadata.load_policy.resolved_device_backend must be a non-empty string"
        )

    load_policy_accelerated = load_policy.get("accelerated")
    if not isinstance(load_policy_accelerated, bool):
        raise ValueError("health metadata.load_policy.accelerated must be a boolean")

    degraded_reason = data.get("degraded_reason")
    load_policy_degraded_reason = load_policy.get("degraded_reason")
    if load_policy_degraded_reason is not None and not isinstance(
        load_policy_degraded_reason, str
    ):
        raise ValueError(
            "health metadata.load_policy.degraded_reason must be a string or null"
        )

    if load_policy_accelerated != data.get("accelerated"):
        raise ValueError(
            "health metadata.load_policy.accelerated must match top-level accelerated"
        )

    if load_policy_degraded_reason != degraded_reason:
        raise ValueError(
            "health metadata.load_policy.degraded_reason must match top-level degraded_reason"
        )

    if requested != resolved and degraded_reason is None:
        raise ValueError(
            "health metadata must include degraded_reason when requested_device_backend differs from resolved_device_backend"
        )


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

    schema = request.get("schema")
    if schema is not None and schema != SIDECAR_PROTOCOL_SCHEMA:
        return _error_response(
            request_id,
            "invalid_request",
            (f"schema mismatch: expected '{SIDECAR_PROTOCOL_SCHEMA}', got {schema!r}"),
        )

    version = request.get("version")
    if version is not None and version != SIDECAR_PROTOCOL_VERSION:
        return _error_response(
            request_id,
            "invalid_request",
            (
                "unsupported version: "
                f"expected {SIDECAR_PROTOCOL_VERSION}, got {version!r}"
            ),
        )

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
        except Exception as exc:
            # Catch ALL exceptions (RuntimeError from DirectML, OOM, etc.)
            # so the protocol loop stays alive and returns a proper error
            # instead of crashing the sidecar silently.
            request_id = ""
            if isinstance(request, dict):
                rid = request.get("request_id") or request.get("id")
                if isinstance(rid, str):
                    request_id = rid
            response = _error_response(
                request_id, "internal_error", f"{type(exc).__name__}: {exc}"
            )

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
