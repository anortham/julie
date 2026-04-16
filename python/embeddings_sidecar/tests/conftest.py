"""Shared test fixtures for the embedding sidecar test suite."""

from dataclasses import dataclass


@dataclass
class FakeRuntime:
    runtime_name: str = "fake-runtime"
    device: str = "cpu"
    dims: int = 384
    ready: bool = True
    resolved_backend: str = "sidecar"
    accelerated: bool = False
    degraded_reason: str | None = None
    requested_device_backend: str = "cpu"
    resolved_device_backend: str = "cpu"
    cpu_available: bool = True
    cuda_available: bool = False
    directml_available: bool = False
    mps_available: bool = False

    def metadata(self) -> dict[str, object]:
        return {
            "runtime": self.runtime_name,
            "device": self.device,
            "dims": self.dims,
            "resolved_backend": self.resolved_backend,
            "accelerated": self.accelerated,
            "degraded_reason": self.degraded_reason,
            "capabilities": {
                "cpu": {"available": self.cpu_available},
                "cuda": {"available": self.cuda_available},
                "directml": {"available": self.directml_available},
                "mps": {"available": self.mps_available},
            },
            "load_policy": {
                "requested_device_backend": self.requested_device_backend,
                "resolved_device_backend": self.resolved_device_backend,
                "accelerated": self.accelerated,
                "degraded_reason": self.degraded_reason,
            },
        }

    def embed_query(self, text: str) -> list[float]:
        return self._vector_for_text(text)

    def embed_batch(self, texts: list[str]) -> list[list[float]]:
        return [self._vector_for_text(text) for text in texts]

    def _vector_for_text(self, text: str) -> list[float]:
        seed = sum(text.encode("utf-8"))
        return [((seed + idx) % 997) / 997.0 for idx in range(self.dims)]
