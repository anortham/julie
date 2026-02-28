from __future__ import annotations

import importlib
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

main_module = importlib.import_module("sidecar.main")


def test_main_builds_runtime_with_defaults(monkeypatch: pytest.MonkeyPatch) -> None:
    captured: dict[str, object] = {}
    fake_runtime = object()

    def _build_runtime(*, model_id: str, batch_size: int):
        captured["model_id"] = model_id
        captured["batch_size"] = batch_size
        return fake_runtime

    monkeypatch.setattr(main_module, "build_runtime", _build_runtime)
    monkeypatch.setattr(
        main_module,
        "run_stdio_loop",
        lambda runtime: captured.setdefault("runtime", runtime),
    )

    exit_code = main_module.main()

    assert exit_code == 0
    assert captured["model_id"] == "BAAI/bge-small-en-v1.5"
    assert captured["batch_size"] == 32
    assert captured["runtime"] is fake_runtime


def test_main_builds_runtime_with_env_overrides(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, object] = {}
    fake_runtime = object()

    def _build_runtime(*, model_id: str, batch_size: int):
        captured["model_id"] = model_id
        captured["batch_size"] = batch_size
        return fake_runtime

    monkeypatch.setattr(main_module, "build_runtime", _build_runtime)
    monkeypatch.setattr(
        main_module,
        "run_stdio_loop",
        lambda runtime: captured.setdefault("runtime", runtime),
    )
    monkeypatch.setenv("JULIE_EMBEDDING_SIDECAR_MODEL_ID", "intfloat/e5-small-v2")
    monkeypatch.setenv("JULIE_EMBEDDING_SIDECAR_BATCH_SIZE", "64")

    exit_code = main_module.main()

    assert exit_code == 0
    assert captured["model_id"] == "intfloat/e5-small-v2"
    assert captured["batch_size"] == 64
    assert captured["runtime"] is fake_runtime


def test_main_rejects_invalid_batch_size(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JULIE_EMBEDDING_SIDECAR_BATCH_SIZE", "not-a-number")

    with pytest.raises(ValueError, match="JULIE_EMBEDDING_SIDECAR_BATCH_SIZE"):
        main_module.main()
