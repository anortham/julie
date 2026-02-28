from __future__ import annotations

from types import SimpleNamespace
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from sidecar.runtime import build_runtime


def _torch_stub(*, cuda: bool = False, mps: bool = False, mps_callable: bool = True):
    if mps_callable:
        mps_obj = SimpleNamespace(is_available=lambda: mps)
    else:
        mps_obj = SimpleNamespace(is_available=True)
    return SimpleNamespace(
        cuda=SimpleNamespace(is_available=lambda: cuda),
        backends=SimpleNamespace(mps=mps_obj),
    )


class _GoodModel:
    def get_sentence_embedding_dimension(self) -> int:
        return 384

    def encode(self, texts, **_kwargs):
        return [[0.01] * 384 for _ in texts]


class _BadOutputModel:
    def get_sentence_embedding_dimension(self) -> int:
        return 384

    def encode(self, texts, **_kwargs):
        return [[0.01] * 383 for _ in texts]


class _CountMismatchModel:
    def get_sentence_embedding_dimension(self) -> int:
        return 384

    def encode(self, _texts, **_kwargs):
        return [[0.01] * 384]


def test_runtime_reports_384_dimensions() -> None:
    rt = build_runtime(
        model_id="BAAI/bge-small-en-v1.5",
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(),
    )
    assert rt.dimensions == 384


def test_runtime_embed_batch_matches_input_count_and_vector_size() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(),
    )
    vectors = rt.embed_batch(["foo", "bar", "baz"])
    assert len(vectors) == 3
    assert all(len(v) == 384 for v in vectors)


def test_dimension_guard_rejects_non_384_outputs() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _BadOutputModel(),
        torch_module=_torch_stub(),
    )

    with pytest.raises(ValueError, match="384"):
        rt.embed_batch(["foo"])


def test_embed_query_returns_384_length_vector() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(),
    )

    vector = rt.embed_query("hello")
    assert len(vector) == 384


def test_embed_batch_rejects_output_count_mismatch() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _CountMismatchModel(),
        torch_module=_torch_stub(),
    )

    with pytest.raises(ValueError, match="count mismatch"):
        rt.embed_batch(["a", "b"])


def test_device_selection_prefers_cuda_over_mps_over_cpu() -> None:
    cuda_rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=True, mps=True),
    )
    assert cuda_rt.device == "cuda"

    mps_rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=True),
    )
    assert mps_rt.device == "mps"

    cpu_rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=False),
    )
    assert cpu_rt.device == "cpu"


def test_device_selection_handles_non_callable_mps_probe() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=False, mps_callable=False),
    )
    assert rt.device == "cpu"


def test_build_runtime_requires_positive_batch_size() -> None:
    with pytest.raises(ValueError, match="batch_size"):
        build_runtime(
            model_factory=lambda **_kwargs: _GoodModel(),
            torch_module=_torch_stub(),
            batch_size=0,
        )


def test_missing_torch_dependency_error_is_clear(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from sidecar import runtime as runtime_module

    def _raise(name: str):
        raise ModuleNotFoundError(name)

    monkeypatch.setattr(runtime_module, "import_module", _raise)

    with pytest.raises(RuntimeError, match="missing runtime dependency: torch"):
        build_runtime()
