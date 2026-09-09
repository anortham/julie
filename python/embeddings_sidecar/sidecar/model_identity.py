"""Snapshot hashing of model weights and configuration files."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
from typing import Any

IDENTITY_UNAVAILABLE = "IdentityUnavailable"

# Files defining the model architecture, tokenization, and weights
_CONFIG_FILENAMES = frozenset({
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "modules.json",
    "sentence_bert_config.json",
    "special_tokens_map.json",
    "vocab.json",
    "vocab.txt",
    "merges.txt",
})

_WEIGHTS_EXTENSIONS = frozenset({
    ".safetensors",
    ".bin",
    ".pt",
    ".onnx",
    ".ckpt",
    ".gguf",
})

SnapshotSignature = tuple[tuple[str, int, int], ...]
_CACHE: dict[str, tuple[SnapshotSignature, str]] = {}


def is_valid_digest(digest: str) -> bool:
    """Return True if digest is a 64-character lowercase hexadecimal string."""
    return len(digest) == 64 and all(c in "0123456789abcdef" for c in digest)


def find_snapshot_files(snapshot_dir: Path) -> tuple[list[Path], list[Path]]:
    """Locate weights files and configuration files in snapshot_dir."""
    weights: list[Path] = []
    configs: list[Path] = []

    if not snapshot_dir.is_dir():
        return weights, configs

    for root, dirs, files in os.walk(snapshot_dir):
        # Skip hidden directories and pycache
        dirs[:] = [d for d in dirs if not d.startswith(".") and d != "__pycache__"]
        for f in files:
            p = Path(root) / f
            ext = p.suffix.lower()
            name = p.name.lower()
            if ext in _WEIGHTS_EXTENSIONS:
                weights.append(p)
            elif name in _CONFIG_FILENAMES or (ext == ".json" and "config" in name):
                configs.append(p)

    return weights, configs


def compute_snapshot_fingerprint(snapshot_dir: str | Path) -> str:
    """Compute SHA-256 fingerprint over all weights and configuration files.

    Returns IDENTITY_UNAVAILABLE if required files (weights or configs) are missing.
    Results are cached based on a composite signature of file paths, sizes, and mtimes.
    """
    path = Path(snapshot_dir).resolve()
    if not path.is_dir():
        return IDENTITY_UNAVAILABLE

    weights, configs = find_snapshot_files(path)
    cache_key = path.as_posix()
    if not weights or not configs:
        _CACHE.pop(cache_key, None)
        return IDENTITY_UNAVAILABLE

    all_files = sorted(
        set(weights + configs),
        key=lambda p: p.relative_to(path).as_posix(),
    )

    try:
        file_entries: list[tuple[Path, str, int]] = []
        sig_entries: list[tuple[str, int, int]] = []
        for file_path in all_files:
            rel_posix = file_path.relative_to(path).as_posix()
            stat_res = file_path.stat()
            file_entries.append((file_path, rel_posix, stat_res.st_size))
            sig_entries.append((rel_posix, stat_res.st_size, stat_res.st_mtime_ns))
    except OSError:
        return IDENTITY_UNAVAILABLE

    snapshot_sig: SnapshotSignature = tuple(sig_entries)
    if cache_key in _CACHE:
        cached_sig, cached_hash = _CACHE[cache_key]
        if cached_sig == snapshot_sig:
            return cached_hash

    try:
        hasher = hashlib.sha256()
        for file_path, rel_posix, file_size in file_entries:
            rel_path_bytes = rel_posix.encode("utf-8")
            # Feed relative path and file length header
            hasher.update(rel_path_bytes)
            hasher.update(b"\x00")
            hasher.update(file_size.to_bytes(8, byteorder="big"))

            # Stream file content in chunks
            with file_path.open("rb") as f:
                while chunk := f.read(65536):
                    hasher.update(chunk)
    except OSError:
        return IDENTITY_UNAVAILABLE

    digest = hasher.hexdigest().lower()
    _CACHE[cache_key] = (snapshot_sig, digest)
    return digest


def get_model_identity(
    snapshot_dir: str | Path | None,
    model_id: str,
    dims: int,
) -> dict[str, Any]:
    """Build a structured model identity dictionary for health metadata."""
    if snapshot_dir is None:
        digest = IDENTITY_UNAVAILABLE
    else:
        digest = compute_snapshot_fingerprint(snapshot_dir)

    return {
        "model_id": model_id,
        "model_sha256": digest if is_valid_digest(digest) else IDENTITY_UNAVAILABLE,
        "dims": dims,
        "pooling": "cls",
        "normalization": "l2",
        "instruction_policy_version": 1,
    }
