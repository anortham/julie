from __future__ import annotations

import os

from sidecar.protocol import run_stdio_loop
from sidecar.runtime import DEFAULT_MODEL_ID, build_runtime

MODEL_ID_ENV = "JULIE_EMBEDDING_SIDECAR_MODEL_ID"
BATCH_SIZE_ENV = "JULIE_EMBEDDING_SIDECAR_BATCH_SIZE"


def _runtime_config_from_env() -> tuple[str, int]:
    model_id = (
        os.environ.get(MODEL_ID_ENV, DEFAULT_MODEL_ID).strip() or DEFAULT_MODEL_ID
    )
    batch_value = os.environ.get(BATCH_SIZE_ENV, "32").strip()
    try:
        batch_size = int(batch_value)
    except ValueError as exc:
        raise ValueError(
            f"{BATCH_SIZE_ENV} must be a positive integer, got '{batch_value}'"
        ) from exc
    if batch_size <= 0:
        raise ValueError(
            f"{BATCH_SIZE_ENV} must be a positive integer, got {batch_size}"
        )
    return model_id, batch_size


def main() -> int:
    model_id, batch_size = _runtime_config_from_env()
    runtime = build_runtime(model_id=model_id, batch_size=batch_size)
    run_stdio_loop(runtime)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
