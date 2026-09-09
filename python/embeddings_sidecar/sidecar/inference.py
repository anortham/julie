from __future__ import annotations

import gc
from typing import Any, Sequence

from sidecar.devices import _normalize_device_telemetry

# Dimensions are now model-driven (reported via protocol handshake).
# Previously hardcoded to 384 for BGE-small; removed to support code models
# like CodeRankEmbed (768d) and Jina-code-v2 (768d).
_SUPPORTED_DIMS = frozenset({384, 768, 1024})


def _sanitize_texts(texts: Sequence[Any]) -> list[str]:
    """Ensure every element is a non-empty string the tokenizer can handle.

    This runs on input from 30+ languages — symbol names, signatures, and doc
    comments can contain arbitrary Unicode, control characters, or be empty
    after metadata formatting strips content.  The tokenizer (HuggingFace
    ``tokenizers``) raises ``TypeError`` on non-string or empty input.
    """
    cleaned: list[str] = []
    for text in texts:
        if not isinstance(text, str) or not text.strip():
            cleaned.append("[empty]")
        else:
            # Strip null bytes and other control characters that can
            # confuse the tokenizer's internal Rust encoder.
            safe = text.replace("\x00", "")
            cleaned.append(safe if safe.strip() else "[empty]")
    return cleaned


def _as_vectors(raw: Any) -> list[list[float]]:
    data = raw.tolist() if hasattr(raw, "tolist") else raw
    if not isinstance(data, list):
        raise TypeError("embedding output must be a list-like object")
    if not data:
        return []
    if isinstance(data[0], (int, float)):
        return [[float(value) for value in data]]
    vectors: list[list[float]] = []
    for row in data:
        if hasattr(row, "tolist"):
            row = row.tolist()
        if not isinstance(row, list):
            raise TypeError("embedding output must be a 2D list-like object")
        vectors.append([float(value) for value in row])
    return vectors


class SentenceTransformerRuntime:
    runtime_name = "sentence-transformers"

    def __init__(
        self,
        model: Any,
        *,
        model_id: str,
        device: str,
        batch_size: int,
        resolved_backend: str = "sidecar",
        accelerated: bool | None = None,
        degraded_reason: str | None = None,
        capabilities: dict[str, object] | None = None,
        load_policy: dict[str, object] | None = None,
        torch_module: Any = None,
    ) -> None:
        self._model = model
        self._model_id = model_id
        self.device = device
        self._batch_size = batch_size
        self._resolved_backend = resolved_backend
        self._accelerated = (
            _normalize_device_telemetry(device) != "cpu"
            if accelerated is None
            else accelerated
        )
        self._degraded_reason = degraded_reason
        self._capabilities = capabilities or {
            "cpu": {"available": True},
            "cuda": {"available": False},
            "directml": {"available": False},
            "mps": {"available": False},
        }
        self._load_policy = load_policy or {
            "requested_device_backend": _normalize_device_telemetry(device),
            "resolved_device_backend": _normalize_device_telemetry(device),
            "accelerated": self._accelerated,
            "degraded_reason": degraded_reason,
        }
        self._torch = torch_module
        self.ready = True
        self.dims = self._resolve_declared_dims()
        self._guard_dims(self.dims, context="init")
        self._model_identity = None
        try:
            from sidecar.model_identity import get_model_identity
            snapshot_dir = self._resolve_snapshot_dir(model, model_id)
            self._model_identity = get_model_identity(snapshot_dir, model_id, self.dims)
        except Exception:
            self._model_identity = None

    @staticmethod
    def _resolve_snapshot_dir(model: Any, model_id: str) -> Any:
        from pathlib import Path
        try:
            if Path(model_id).is_dir():
                return Path(model_id)
        except Exception:
            pass
        first_module = getattr(model, "_first_module", None)
        if callable(first_module):
            try:
                mod = first_module()
                auto_model = getattr(mod, "auto_model", None)
                name_or_path = getattr(auto_model, "name_or_path", None)
                if name_or_path and Path(name_or_path).is_dir():
                    return Path(name_or_path)
            except Exception:
                pass
        return None

    @property
    def dimensions(self) -> int:
        return self.dims

    def metadata(self) -> dict[str, object]:
        data = {
            "runtime": self.runtime_name,
            "device": self.device,
            "dims": self.dims,
            "model_id": self._model_id,
            "resolved_backend": self._resolved_backend,
            "accelerated": self._accelerated,
            "degraded_reason": self._degraded_reason,
            "capabilities": self._capabilities,
            "load_policy": self._load_policy,
        }
        if self._model_identity:
            data.update(self._model_identity)
        return data

    def _empty_device_cache(self) -> None:
        """Release cached GPU buffers back to the system.

        PyTorch's MPS and CUDA backends use caching allocators that hold
        freed buffers for reuse. Without explicit clearing, MPS memory
        can grow to ~6 GB for a 500 MB model on Apple Silicon.
        """
        if self._torch is None:
            return
        if self.device == "mps":
            mps = getattr(self._torch, "mps", None)
            if mps is not None:
                empty = getattr(mps, "empty_cache", None)
                if callable(empty):
                    empty()
        elif self.device == "cuda":
            cuda = getattr(self._torch, "cuda", None)
            if cuda is not None:
                empty = getattr(cuda, "empty_cache", None)
                if callable(empty):
                    empty()

    def embed_query(self, text: str) -> list[float]:
        vectors = self.embed_batch([text])
        return vectors[0]

    def embed_batch(self, texts: Sequence[str]) -> list[list[float]]:
        if not texts:
            return []
        sanitized = _sanitize_texts(texts)
        raw_vectors = self._encode_with_fallback(sanitized)
        # DirectML has no explicit cache clearing (unlike torch.cuda.empty_cache).
        # Force a GC pass to release any stale GPU tensor references that
        # Python's refcount alone may not catch (cycles, weak refs, etc.).
        # Without this, DirectML leaks GPU memory across many forward passes
        # until the driver crashes.
        gc.collect()
        self._empty_device_cache()
        vectors = _as_vectors(raw_vectors)
        if len(vectors) != len(texts):
            raise ValueError(
                "embedding output count mismatch: "
                f"expected {len(texts)}, got {len(vectors)}"
            )
        for vector in vectors:
            self._guard_dims(len(vector), context="inference")
        return vectors

    def _encode_with_fallback(self, texts: list[str]) -> Any:
        """Encode texts with binary-search fallback for bad inputs.

        Tries the full batch first.  On failure, recursively splits in half
        to isolate the problematic text(s).  For 500 texts with 1 bad text
        this takes ~9 splits (~800ms) instead of 500 individual calls (~25s).
        """
        try:
            return self._model.encode(
                texts,
                batch_size=self._batch_size,
                convert_to_numpy=True,
                normalize_embeddings=True,
                show_progress_bar=False,
            )
        except Exception as exc:
            if len(texts) <= 1:
                # Single unencodable text — log it and return zero vector.
                import sys

                print(
                    f"[sidecar] skipping unencodable text "
                    f"({type(exc).__name__}: {exc}): {texts[0][:120]!r}",
                    file=sys.stderr,
                )
                return [[0.0] * self.dims]

            # Split and try each half — good halves batch-encode normally,
            # only the failing half gets split further.
            mid = len(texts) // 2
            left = _as_vectors(self._encode_with_fallback(texts[:mid]))
            right = _as_vectors(self._encode_with_fallback(texts[mid:]))
            return left + right

    def _resolve_declared_dims(self) -> int:
        getter = getattr(self._model, "get_sentence_embedding_dimension", None)
        if not callable(getter):
            raise ValueError("model does not expose embedding dimension")
        dims = getter()
        if not isinstance(dims, int):
            raise ValueError("model embedding dimension must be an integer")
        return dims

    def _guard_dims(self, dims: int, *, context: str) -> None:
        if dims not in _SUPPORTED_DIMS:
            raise ValueError(
                f"embedding dimensions must be one of {sorted(_SUPPORTED_DIMS)}, "
                f"got {dims} during {context}"
            )
