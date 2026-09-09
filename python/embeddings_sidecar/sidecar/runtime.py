from __future__ import annotations

import gc
from importlib import import_module
from typing import Any, Callable

from sidecar.devices import (
    _build_load_policy,
    _calculate_batch_size_from_vram,
    _detect_gpu_vram_bytes,
    _normalize_device_telemetry,
    _patch_directml_inference_mode,
    _patch_directml_rotary_embeddings,
    _probe_backend_capabilities,
    _select_device,
)
from sidecar.inference import (
    _SUPPORTED_DIMS,
    _as_vectors,
    _sanitize_texts,
    SentenceTransformerRuntime,
)

DEFAULT_MODEL_ID = "nomic-ai/CodeRankEmbed"
_DEFAULT_BATCH_SIZE = 32


def _import_module(name: str) -> Any:
    try:
        return import_module(name)
    except ModuleNotFoundError as exc:
        raise RuntimeError(f"missing runtime dependency: {name}") from exc


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
    degraded_reason: str | None = None
    capabilities = _probe_backend_capabilities(torch, dml_module)
    requested_device_backend = telemetry_device
    resolved_device_backend = telemetry_device

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
        # Use float16 on GPU to halve model weight memory.
        # CodeRankEmbed weights are stored as float16 in safetensors;
        # loading without specifying dtype upcasts to float32, doubling memory.
        model_kwargs: dict[str, Any] = {}
        use_half = backend_device in ("mps", "cuda")
        if use_half:
            half_dtype = getattr(torch, "float16", None)
            if half_dtype is not None:
                model_kwargs["torch_dtype"] = half_dtype
        try:
            model = sentence_transformers.SentenceTransformer(
                model_id, device=backend_device, trust_remote_code=True,
                model_kwargs=model_kwargs,
            )
        except Exception:
            if use_half and model_kwargs:
                import sys as _sys
                print(
                    f"[sidecar] float16 model load failed, retrying in float32",
                    file=_sys.stderr,
                )
                model_kwargs.pop("torch_dtype", None)
                model = sentence_transformers.SentenceTransformer(
                    model_id, device=backend_device, trust_remote_code=True,
                    model_kwargs=model_kwargs,
                )
            else:
                raise

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
                degraded_reason = (
                    f"probe encode failed on {telemetry_device}, fell back to CPU"
                )
                resolved_device_backend = "cpu"
                del model
                gc.collect()
                # Release GPU buffers held by the caching allocator
                if telemetry_device == "mps":
                    mps = getattr(torch, "mps", None)
                    if mps is not None and callable(getattr(mps, "empty_cache", None)):
                        mps.empty_cache()
                elif telemetry_device == "cuda":
                    cuda_mod = getattr(torch, "cuda", None)
                    if cuda_mod is not None and callable(getattr(cuda_mod, "empty_cache", None)):
                        cuda_mod.empty_cache()

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
        degraded_reason=degraded_reason,
        capabilities=capabilities,
        load_policy=_build_load_policy(
            requested_device_backend=requested_device_backend,
            resolved_device_backend=resolved_device_backend,
            degraded_reason=degraded_reason,
        ),
        torch_module=torch,
    )
