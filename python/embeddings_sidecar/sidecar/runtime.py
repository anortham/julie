from __future__ import annotations

from dataclasses import dataclass
from importlib import import_module
from typing import Any, Callable, Sequence

EXPECTED_DIMS = 384
DEFAULT_MODEL_ID = "BAAI/bge-small-en-v1.5"


def _import_module(name: str) -> Any:
    try:
        return import_module(name)
    except ModuleNotFoundError as exc:
        raise RuntimeError(f"missing runtime dependency: {name}") from exc


def _select_device(torch_module: Any) -> str:
    cuda = getattr(torch_module, "cuda", None)
    cuda_probe = getattr(cuda, "is_available", None)
    if callable(cuda_probe) and cuda_probe():
        return "cuda"

    backends = getattr(torch_module, "backends", None)
    mps = getattr(backends, "mps", None)
    mps_probe = getattr(mps, "is_available", None)
    if callable(mps_probe) and mps_probe():
        return "mps"

    return "cpu"


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
    ) -> None:
        self._model = model
        self._model_id = model_id
        self.device = device
        self._batch_size = batch_size
        self.ready = True
        self.dims = self._resolve_declared_dims()
        self._guard_dims(self.dims, context="init")

    @property
    def dimensions(self) -> int:
        return self.dims

    def metadata(self) -> dict[str, object]:
        return {
            "runtime": self.runtime_name,
            "device": self.device,
            "dims": self.dims,
            "model_id": self._model_id,
        }

    def embed_query(self, text: str) -> list[float]:
        vectors = self.embed_batch([text])
        return vectors[0]

    def embed_batch(self, texts: Sequence[str]) -> list[list[float]]:
        if not texts:
            return []
        raw_vectors = self._model.encode(
            list(texts),
            batch_size=self._batch_size,
            convert_to_numpy=False,
            normalize_embeddings=True,
            show_progress_bar=False,
        )
        vectors = _as_vectors(raw_vectors)
        if len(vectors) != len(texts):
            raise ValueError(
                "embedding output count mismatch: "
                f"expected {len(texts)}, got {len(vectors)}"
            )
        for vector in vectors:
            self._guard_dims(len(vector), context="inference")
        return vectors

    def _resolve_declared_dims(self) -> int:
        getter = getattr(self._model, "get_sentence_embedding_dimension", None)
        if not callable(getter):
            raise ValueError("model does not expose embedding dimension")
        dims = getter()
        if not isinstance(dims, int):
            raise ValueError("model embedding dimension must be an integer")
        return dims

    def _guard_dims(self, dims: int, *, context: str) -> None:
        if dims != EXPECTED_DIMS:
            raise ValueError(
                f"embedding dimensions must be {EXPECTED_DIMS}, "
                f"got {dims} during {context}"
            )


def build_runtime(
    *,
    model_id: str = DEFAULT_MODEL_ID,
    batch_size: int = 32,
    model_factory: Callable[..., Any] | None = None,
    torch_module: Any | None = None,
) -> SentenceTransformerRuntime:
    if not isinstance(batch_size, int) or batch_size <= 0:
        raise ValueError("batch_size must be a positive integer")

    torch = torch_module if torch_module is not None else _import_module("torch")
    device = _select_device(torch)

    if model_factory is not None:
        model = model_factory(model_id=model_id, device=device)
    else:
        sentence_transformers = _import_module("sentence_transformers")
        model = sentence_transformers.SentenceTransformer(model_id, device=device)

    return SentenceTransformerRuntime(
        model,
        model_id=model_id,
        device=device,
        batch_size=batch_size,
    )


@dataclass
class FakeRuntime:
    runtime_name: str = "fake-runtime"
    device: str = "cpu"
    dims: int = 384
    ready: bool = True

    def metadata(self) -> dict[str, object]:
        return {
            "runtime": self.runtime_name,
            "device": self.device,
            "dims": self.dims,
        }

    def embed_query(self, text: str) -> list[float]:
        return self._vector_for_text(text)

    def embed_batch(self, texts: list[str]) -> list[list[float]]:
        return [self._vector_for_text(text) for text in texts]

    def _vector_for_text(self, text: str) -> list[float]:
        seed = sum(text.encode("utf-8"))
        return [((seed + idx) % 997) / 997.0 for idx in range(self.dims)]
