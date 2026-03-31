"""Shared test fixtures for the embedding sidecar test suite."""

from dataclasses import dataclass


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
