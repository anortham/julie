from __future__ import annotations

import sys as _sys
from typing import Any


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


def _probe_backend_capabilities(torch_module: Any, dml_module: Any = None) -> dict[str, object]:
    cuda = getattr(torch_module, "cuda", None)
    cuda_probe = getattr(cuda, "is_available", None)

    mps = getattr(getattr(torch_module, "backends", None), "mps", None)
    mps_probe = getattr(mps, "is_available", None)

    dml_probe = getattr(dml_module, "is_available", None) if dml_module is not None else None

    return {
        "cpu": {"available": True},
        "cuda": {"available": bool(callable(cuda_probe) and cuda_probe())},
        "directml": {"available": bool(callable(dml_probe) and dml_probe())},
        "mps": {"available": bool(callable(mps_probe) and mps_probe())},
    }


def _build_load_policy(
    *,
    requested_device_backend: str,
    resolved_device_backend: str,
    degraded_reason: str | None,
) -> dict[str, object]:
    return {
        "requested_device_backend": requested_device_backend,
        "resolved_device_backend": resolved_device_backend,
        "accelerated": resolved_device_backend != "cpu",
        "degraded_reason": degraded_reason,
    }


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
