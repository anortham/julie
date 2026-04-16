from __future__ import annotations

from types import SimpleNamespace
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from sidecar.runtime import (
    build_runtime,
    _patch_directml_inference_mode,
    _sanitize_texts,
)


def _torch_stub(*, cuda: bool = False, mps: bool = False, mps_callable: bool = True):
    if mps_callable:
        mps_obj = SimpleNamespace(is_available=lambda: mps)
    else:
        mps_obj = SimpleNamespace(is_available=True)
    return SimpleNamespace(
        cuda=SimpleNamespace(is_available=lambda: cuda),
        backends=SimpleNamespace(mps=mps_obj),
    )


def _dml_stub(*, available: bool = True):
    """Stub for torch_directml module."""
    return SimpleNamespace(
        is_available=lambda: available,
        device=lambda: "privateuseone:0",
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


def test_device_selection_prefers_directml_over_cpu() -> None:
    captured_device = []

    def _capture_factory(**kwargs):
        captured_device.append(kwargs["device"])
        return _GoodModel()

    rt = build_runtime(
        model_factory=_capture_factory,
        torch_module=_torch_stub(cuda=False, mps=False),
        dml_module=_dml_stub(available=True),
    )
    assert captured_device == ["privateuseone:0"]
    assert rt.device == "directml:0"
    assert rt.metadata()["device"] == "directml:0"


def test_device_selection_normalizes_directml_label() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=False),
        dml_module=_dml_stub(available=True),
    )
    assert rt.device.startswith("directml")


def test_device_selection_prefers_cuda_over_directml() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=True, mps=False),
        dml_module=_dml_stub(available=True),
    )
    assert rt.device == "cuda"


def test_device_selection_falls_back_when_directml_unavailable() -> None:
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=False),
        dml_module=_dml_stub(available=False),
    )
    assert rt.device == "cpu"


def test_device_selection_skips_directml_when_not_installed() -> None:
    """When dml_module is not provided and torch_directml is not installed,
    device selection should fall through to MPS/CPU without error."""
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(cuda=False, mps=False),
        # no dml_module — simulates torch_directml not installed
    )
    assert rt.device == "cpu"


def test_directml_inference_mode_patch_replaces_with_no_grad() -> None:
    """DirectML crashes with torch.inference_mode(). The patch should
    replace it with torch.no_grad() while preserving the original."""
    sentinel = object()
    no_grad_sentinel = object()
    fake_torch = SimpleNamespace(
        inference_mode=sentinel,
        no_grad=lambda: no_grad_sentinel,
        enable_grad=lambda: None,
    )

    _patch_directml_inference_mode(fake_torch)

    assert fake_torch._original_inference_mode is sentinel
    assert fake_torch.inference_mode is not sentinel
    # Calling the patched version should return no_grad's result
    assert fake_torch.inference_mode() is no_grad_sentinel


def test_directml_inference_mode_patch_is_idempotent() -> None:
    """Patching twice should not overwrite the original reference."""
    original = object()
    fake_torch = SimpleNamespace(
        inference_mode=original,
        no_grad=lambda: None,
        enable_grad=lambda: None,
    )

    _patch_directml_inference_mode(fake_torch)
    first_patched = fake_torch.inference_mode

    _patch_directml_inference_mode(fake_torch)

    assert fake_torch._original_inference_mode is original
    assert fake_torch.inference_mode is first_patched


def test_directml_patch_is_active_when_model_factory_runs() -> None:
    """When DirectML is selected, the inference_mode patch should be
    applied before the model factory is called."""
    patch_was_active_at_factory_call = []

    def checking_factory(**_kwargs):
        patch_was_active_at_factory_call.append(
            hasattr(_torch, "_original_inference_mode")
        )
        return _GoodModel()

    _torch = _torch_stub(cuda=False, mps=False)
    # Add inference_mode so the patch has something to replace
    _torch.inference_mode = lambda mode=True: None
    _torch.no_grad = lambda: None
    _torch.enable_grad = lambda: None

    rt = build_runtime(
        model_factory=checking_factory,
        torch_module=_torch,
        dml_module=_dml_stub(available=True),
    )
    assert patch_was_active_at_factory_call == [True]
    assert hasattr(_torch, "_original_inference_mode")


def test_build_runtime_surfaces_fallback_in_metadata(monkeypatch: pytest.MonkeyPatch) -> None:
    class _FallbackModel:
        def __init__(self, *, device: str) -> None:
            self.device = device
            self.calls = 0

        def get_sentence_embedding_dimension(self) -> int:
            return 384

        def encode(self, texts, **_kwargs):
            self.calls += 1
            if self.device != "cpu":
                raise RuntimeError("DirectML probe failed")
            return [[0.01] * 384 for _ in texts]

    created_devices: list[str] = []

    class _SentenceTransformersModule:
        def SentenceTransformer(
            self, model_id, *, device, trust_remote_code, model_kwargs=None
        ):
            created_devices.append(device)
            return _FallbackModel(device=device)

    from sidecar import runtime as runtime_module

    monkeypatch.setattr(
        runtime_module,
        "_import_module",
        lambda name: _SentenceTransformersModule()
        if name == "sentence_transformers"
        else (_ for _ in ()).throw(AssertionError(f"unexpected import: {name}")),
    )

    rt = build_runtime(
        torch_module=_torch_stub(cuda=False, mps=False),
        dml_module=_dml_stub(available=True),
    )

    metadata = rt.metadata()

    assert created_devices == ["privateuseone:0", "cpu"]
    assert metadata["capabilities"] == {
        "cpu": {"available": True},
        "cuda": {"available": False},
        "directml": {"available": True},
        "mps": {"available": False},
    }
    assert metadata["load_policy"] == {
        "requested_device_backend": "directml:0",
        "resolved_device_backend": "cpu",
        "accelerated": False,
        "degraded_reason": "probe encode failed on directml:0, fell back to CPU",
    }
    assert metadata["resolved_backend"] == "sidecar"
    assert metadata["accelerated"] is False
    assert metadata["degraded_reason"] == "probe encode failed on directml:0, fell back to CPU"


# =========================================================================
# Text sanitization — defensive coding for 30+ language inputs
# =========================================================================


def test_sanitize_replaces_empty_strings() -> None:
    result = _sanitize_texts(["hello", "", "world"])
    assert result == ["hello", "[empty]", "world"]


def test_sanitize_replaces_whitespace_only() -> None:
    result = _sanitize_texts(["ok", "   ", "\t\n"])
    assert result == ["ok", "[empty]", "[empty]"]


def test_sanitize_strips_null_bytes() -> None:
    result = _sanitize_texts(["he\x00llo", "\x00"])
    assert result[0] == "hello"
    assert result[1] == "[empty]"


def test_sanitize_replaces_non_string_values() -> None:
    result = _sanitize_texts(["ok", None, 42, ["nested"]])  # type: ignore[list-item]
    assert result == ["ok", "[empty]", "[empty]", "[empty]"]


def test_sanitize_preserves_unicode() -> None:
    texts = ["処理データ", "café", "Ñoño", "🚀 rocket"]
    result = _sanitize_texts(texts)
    assert result == texts


# =========================================================================
# Binary-search fallback encoding
# =========================================================================


def test_embed_batch_binary_search_fallback() -> None:
    """If model.encode raises on a batch, embed_batch should binary-search
    to isolate the bad text and encode the rest normally."""
    call_count = [0]

    class FlakeyModel:
        def get_sentence_embedding_dimension(self) -> int:
            return 384

        def encode(self, texts, **kwargs):
            call_count[0] += 1
            if len(texts) > 1:
                raise TypeError("TextEncodeInput must be ...")
            import numpy as np

            return np.random.rand(1, 384).astype(np.float32)

    rt = build_runtime(
        model_factory=lambda **_kwargs: FlakeyModel(),
        torch_module=_torch_stub(cuda=False),
    )
    vectors = rt.embed_batch(["a", "b", "c"])
    assert len(vectors) == 3
    assert all(len(v) == 384 for v in vectors)
    # Binary search: batch(3) FAIL → [a] OK + [b,c] FAIL → [b] OK + [c] OK
    assert call_count[0] == 5


def test_embed_batch_zero_vector_for_unencodable() -> None:
    """A single text that always fails should produce a zero vector."""

    class AlwaysFailModel:
        def get_sentence_embedding_dimension(self) -> int:
            return 384

        def encode(self, texts, **kwargs):
            raise TypeError("bad input")

    rt = build_runtime(
        model_factory=lambda **_kwargs: AlwaysFailModel(),
        torch_module=_torch_stub(cuda=False),
    )
    vectors = rt.embed_batch(["bad text"])
    assert len(vectors) == 1
    assert len(vectors[0]) == 384
    assert all(v == 0.0 for v in vectors[0])


# =========================================================================
# GPU memory management
# =========================================================================


def test_embed_batch_clears_mps_device_cache() -> None:
    """embed_batch should release MPS cached buffers after encoding.

    Without torch.mps.empty_cache(), PyTorch's MPS caching allocator
    grows to ~6 GB for a 500 MB model on Apple Silicon.
    """
    cache_cleared = [False]

    torch_mod = _torch_stub(mps=True)
    # torch.mps.empty_cache() lives at torch.mps, not torch.backends.mps
    torch_mod.mps = SimpleNamespace(
        empty_cache=lambda: cache_cleared.__setitem__(0, True),
    )

    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=torch_mod,
    )
    rt.embed_batch(["hello"])
    assert cache_cleared[0], "torch.mps.empty_cache() was not called"


def test_embed_batch_clears_cuda_device_cache() -> None:
    """embed_batch should release CUDA cached buffers after encoding."""
    cache_cleared = [False]

    torch_mod = _torch_stub(cuda=True)
    torch_mod.cuda.empty_cache = lambda: cache_cleared.__setitem__(0, True)

    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=torch_mod,
    )
    rt.embed_batch(["hello"])
    assert cache_cleared[0], "torch.cuda.empty_cache() was not called"


def test_embed_batch_no_crash_on_cpu_without_cache() -> None:
    """CPU device should not attempt GPU cache clearing."""
    rt = build_runtime(
        model_factory=lambda **_kwargs: _GoodModel(),
        torch_module=_torch_stub(),
    )
    vectors = rt.embed_batch(["hello"])
    assert len(vectors) == 1
    assert len(vectors[0]) == 384
