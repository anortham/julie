from __future__ import annotations

import gc
from dataclasses import dataclass
from importlib import import_module
from typing import Any, Callable, Sequence

# Dimensions are now model-driven (reported via protocol handshake).
# Previously hardcoded to 384 for BGE-small; removed to support code models
# like CodeRankEmbed (768d) and Jina-code-v2 (768d).
_SUPPORTED_DIMS = frozenset({384, 768, 1024})
DEFAULT_MODEL_ID = "nomic-ai/CodeRankEmbed"


def _import_module(name: str) -> Any:
    try:
        return import_module(name)
    except ModuleNotFoundError as exc:
        raise RuntimeError(f"missing runtime dependency: {name}") from exc


def _select_device(torch_module: Any, dml_module: Any = None) -> str:
    cuda = getattr(torch_module, "cuda", None)
    cuda_probe = getattr(cuda, "is_available", None)
    if callable(cuda_probe) and cuda_probe():
        return "cuda"

    if dml_module is not None:
        dml_probe = getattr(dml_module, "is_available", None)
        if callable(dml_probe) and dml_probe():
            dml_device = getattr(dml_module, "device", None)
            if callable(dml_device):
                return str(dml_device())

    backends = getattr(torch_module, "backends", None)
    mps = getattr(backends, "mps", None)
    mps_probe = getattr(mps, "is_available", None)
    if callable(mps_probe) and mps_probe():
        return "mps"

    return "cpu"


def _patch_directml_inference_mode(torch_module: Any) -> None:
    """Patch torch.inference_mode for DirectML compatibility.

    DirectML throws ``RuntimeError: Cannot set version_counter for
    inference tensor`` when using ``torch.inference_mode()``.  Replacing
    it with ``torch.no_grad()`` avoids the crash while preserving the
    same inference-time semantics.

    Must be called BEFORE importing sentence_transformers (which decorates
    internal functions with inference_mode at import time).

    See: https://github.com/microsoft/DirectML/issues/622
    """
    if hasattr(torch_module, "_original_inference_mode"):
        return  # Already patched — idempotent
    original = getattr(torch_module, "inference_mode", None)
    if original is None:
        return  # Nothing to patch
    torch_module._original_inference_mode = original
    torch_module.inference_mode = (
        lambda mode=True: torch_module.no_grad() if mode else torch_module.enable_grad()
    )


def _patch_directml_rotary_embeddings() -> bool:
    """Patch CodeRankEmbed's rotary embedding for DirectML compatibility.

    The nomic-bert model uses ``einops.repeat`` to expand cos/sin tensors
    for rotary position embeddings.  The resulting tensors have non-contiguous
    memory layout that DirectML's broadcasting kernels reject with
    "The parameter is incorrect".  Adding ``.contiguous()`` after the repeat
    fixes the layout without changing the math.

    Returns True if the patch was applied, False if the module wasn't loaded.
    """
    import sys

    module_key = None
    for key in sys.modules:
        if "modeling_hf_nomic_bert" in key:
            module_key = key
            break
    if module_key is None:
        return False

    mod = sys.modules[module_key]
    original_fn = getattr(mod, "apply_rotary_emb", None)
    if original_fn is None or getattr(original_fn, "_directml_patched", False):
        return False

    from einops import repeat as _repeat

    def _apply_rotary_emb_directml(x, cos, sin, offset=0, interleaved=False):
        import torch

        ro_dim = cos.shape[-1] * 2
        assert ro_dim <= x.shape[-1]
        cos = cos[offset : offset + x.shape[1]]
        sin = sin[offset : offset + x.shape[1]]
        pattern = "... d -> ... 1 (2 d)" if not interleaved else "... d -> ... 1 (d 2)"
        cos = _repeat(cos, pattern).contiguous()
        sin = _repeat(sin, pattern).contiguous()
        x_rot = x[..., :ro_dim]
        x_pass = x[..., ro_dim:]
        x1, x2 = x_rot.chunk(2, dim=-1)
        rotated = torch.cat((-x2, x1), dim=-1)
        return torch.cat([x_rot * cos + rotated * sin, x_pass], dim=-1)

    _apply_rotary_emb_directml._directml_patched = True
    mod.apply_rotary_emb = _apply_rotary_emb_directml
    return True


def _normalize_device_telemetry(device: str) -> str:
    if device.startswith("privateuseone"):
        return device.replace("privateuseone", "directml", 1)
    return device


def _detect_gpu_vram_bytes(telemetry_device: str, torch_module: Any) -> int | None:
    """Detect total GPU VRAM in bytes (platform-specific).

    Returns None if detection fails — caller should fall back to a safe default.
    """
    import sys as _sys

    if telemetry_device == "cpu":
        return None

    if telemetry_device == "cuda":
        try:
            return torch_module.cuda.get_device_properties(0).total_memory
        except Exception:
            return None

    if telemetry_device == "mps":
        try:
            # recommended_max_working_set_size returns total available GPU memory,
            # not just what's currently allocated (driver_allocated_memory is wrong
            # here since it returns near-zero on a fresh process).
            return torch_module.mps.recommended_max_working_set_size() or None
        except Exception:
            return None

    # DirectML / other — use WMI on Windows.
    # IMPORTANT: Win32_VideoController.AdapterRAM is a uint32, so it wraps at
    # 4 GB. We also try AdapterDACType-adjacent fields and qwMemorySize, but
    # the most reliable path is nvidia-smi for NVIDIA GPUs.
    if _sys.platform == "win32":
        try:
            import subprocess

            # nvidia-smi is the most reliable source for NVIDIA GPUs
            result = subprocess.run(
                ["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0 and result.stdout.strip():
                # Output is in MiB, one line per GPU
                max_mib = max(int(line.strip()) for line in result.stdout.strip().splitlines() if line.strip())
                return max_mib * 1_048_576  # MiB to bytes
        except Exception:
            pass

        # Fallback: WMI (works for non-NVIDIA, but AdapterRAM wraps at 4 GB)
        try:
            import wmi  # type: ignore[import-untyped]

            w = wmi.WMI()
            max_vram = 0
            for gpu in w.Win32_VideoController():
                vram = getattr(gpu, "AdapterRAM", None)
                if vram:
                    max_vram = max(max_vram, int(vram))
            # AdapterRAM wraps at 4GB; treat suspiciously small values as wrapped
            return max_vram if max_vram >= 2_147_483_648 else None  # >= 2GB
        except Exception:
            return None

    return None


def _calculate_batch_size_from_vram(vram_bytes: int) -> int:
    """Compute GPU batch size from VRAM using Miller's DirectML-safe formula.

    Formula: batch_size = (VRAM_GB / 6.0) * 30, clamped to [16, 128].

    Validated on:
    - 6GB A1000: batch_size=30 → stable (50 caused OOM crash)
    - 8GB consumer GPUs: batch_size=40 → stable
    - 16GB+ workstation GPUs: batch_size=80 → fast and stable
    """
    vram_gb = vram_bytes / 1_073_741_824.0
    calculated = int((vram_gb / 6.0) * 30.0)
    return max(16, min(128, calculated))


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
        sanitized = _sanitize_texts(texts)
        raw_vectors = self._encode_with_fallback(sanitized)
        # DirectML has no explicit cache clearing (unlike torch.cuda.empty_cache).
        # Force a GC pass to release any stale GPU tensor references that
        # Python's refcount alone may not catch (cycles, weak refs, etc.).
        # Without this, DirectML leaks GPU memory across many forward passes
        # until the driver crashes.
        gc.collect()
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


_DEFAULT_BATCH_SIZE = 32


def build_runtime(
    *,
    model_id: str = DEFAULT_MODEL_ID,
    batch_size: int | None = None,
    model_factory: Callable[..., Any] | None = None,
    torch_module: Any | None = None,
    dml_module: Any | None = None,
) -> SentenceTransformerRuntime:
    torch = torch_module if torch_module is not None else _import_module("torch")

    if dml_module is None:
        try:
            dml_module = import_module("torch_directml")
        except ModuleNotFoundError:
            pass

    backend_device = _select_device(torch, dml_module)
    telemetry_device = _normalize_device_telemetry(backend_device)

    # Auto-detect GPU batch size when caller didn't specify one.
    if batch_size is None:
        vram = _detect_gpu_vram_bytes(telemetry_device, torch)
        if vram is not None:
            batch_size = _calculate_batch_size_from_vram(vram)
            import sys
            vram_gb = vram / 1_073_741_824.0
            print(
                f"GPU VRAM: {vram_gb:.1f} GB → batch_size={batch_size}",
                file=sys.stderr,
            )
        else:
            batch_size = _DEFAULT_BATCH_SIZE

    if not isinstance(batch_size, int) or batch_size <= 0:
        raise ValueError("batch_size must be a positive integer")

    # DirectML crashes with torch.inference_mode() — patch before importing
    # sentence_transformers which uses it at import time in decorators.
    if telemetry_device not in ("cuda", "mps", "cpu"):
        _patch_directml_inference_mode(torch)

    if model_factory is not None:
        model = model_factory(model_id=model_id, device=backend_device)
    else:
        sentence_transformers = _import_module("sentence_transformers")
        model = sentence_transformers.SentenceTransformer(
            model_id, device=backend_device, trust_remote_code=True
        )

    # Probe encode: verify the model can actually produce embeddings on this
    # device. Some models (e.g., CodeRankEmbed with rotary embeddings) load
    # fine on DirectML but fail during inference due to unsupported ops.
    #
    # Recovery chain for GPU failures:
    # 1. Try patching DirectML-incompatible ops (e.g., RoPE .contiguous() fix)
    # 2. If patched GPU still fails, fall back to CPU
    #
    # Skip probe when model_factory is provided (test doubles that control
    # their own failure behavior shouldn't trigger device fallback).
    if model_factory is None and telemetry_device != "cpu":
        import sys as _sys

        try:
            model.encode(["probe"], convert_to_numpy=True)
        except Exception as probe_err:
            # Step 1: Try patching and re-probing on GPU
            patched = _patch_directml_rotary_embeddings()
            if patched:
                print(
                    f"[sidecar] probe failed on {telemetry_device}, "
                    f"applied RoPE .contiguous() patch — retrying",
                    file=_sys.stderr,
                )
                try:
                    model.encode(["probe"], convert_to_numpy=True)
                    print(
                        f"[sidecar] patched probe succeeded on {telemetry_device}",
                        file=_sys.stderr,
                    )
                except Exception as patched_err:
                    probe_err = patched_err  # Use the patched error for fallback
                    patched = False

            # Step 2: If patching didn't help, fall back to CPU
            if not patched:
                print(
                    f"[sidecar] probe encode failed on {telemetry_device}: {probe_err}\n"
                    f"[sidecar] falling back to CPU for model {model_id}",
                    file=_sys.stderr,
                )
                del model
                gc.collect()

                model = sentence_transformers.SentenceTransformer(
                    model_id, device="cpu", trust_remote_code=True
                )
                backend_device = "cpu"
                telemetry_device = "cpu"
                # Reset batch size: GPU-calibrated sizes (80+) cause memory
                # pressure on CPU. Use default unless explicitly overridden.
                if batch_size is None:
                    batch_size = _DEFAULT_BATCH_SIZE
                try:
                    model.encode(["probe"], convert_to_numpy=True)
                except Exception as cpu_err:
                    print(
                        f"[sidecar] FATAL: model {model_id} cannot encode on any device.\n"
                        f"  GPU error: {probe_err}\n"
                        f"  CPU error: {cpu_err}",
                        file=_sys.stderr,
                    )
                    raise

    return SentenceTransformerRuntime(
        model,
        model_id=model_id,
        device=telemetry_device,
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
