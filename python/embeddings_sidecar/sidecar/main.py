from __future__ import annotations

import os
import sys

from sidecar.protocol import run_stdio_loop
from sidecar.runtime import DEFAULT_MODEL_ID, build_runtime

MODEL_ID_ENV = "JULIE_EMBEDDING_SIDECAR_MODEL_ID"
BATCH_SIZE_ENV = "JULIE_EMBEDDING_SIDECAR_BATCH_SIZE"


def _runtime_config_from_env() -> tuple[str, int | None]:
    model_id = (
        os.environ.get(MODEL_ID_ENV, DEFAULT_MODEL_ID).strip() or DEFAULT_MODEL_ID
    )
    raw = os.environ.get(BATCH_SIZE_ENV, "").strip()
    if not raw:
        # No explicit override → let build_runtime auto-detect from VRAM
        return model_id, None
    try:
        batch_size = int(raw)
    except ValueError as exc:
        raise ValueError(
            f"{BATCH_SIZE_ENV} must be a positive integer, got '{raw}'"
        ) from exc
    if batch_size <= 0:
        raise ValueError(
            f"{BATCH_SIZE_ENV} must be a positive integer, got {batch_size}"
        )
    return model_id, batch_size


def main() -> int:
    model_id, batch_size = _runtime_config_from_env()

    # Redirect stdout → stderr at the OS file-descriptor level during model
    # loading.  C extensions (safetensors / tqdm) write progress bars directly
    # to fd 1, bypassing Python's sys.stdout, so a Python-level redirect is
    # not sufficient.
    saved_stdout_fd = os.dup(1)
    os.dup2(2, 1)  # fd 1 now points to stderr
    try:
        runtime = build_runtime(model_id=model_id, batch_size=batch_size)
    finally:
        os.dup2(saved_stdout_fd, 1)  # restore fd 1 → real stdout pipe
        os.close(saved_stdout_fd)
        sys.stdout = open(1, "w", closefd=False)  # reconnect Python sys.stdout

    run_stdio_loop(runtime)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
