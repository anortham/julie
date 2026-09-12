#!/usr/bin/env python3
"""Require the exact four release archives and their checksum files."""

import sys
import hashlib
from pathlib import Path


directory, version = Path(sys.argv[1]), sys.argv[2]
archives = [
    f"julie-v{version}-aarch64-apple-darwin.tar.gz",
    f"julie-v{version}-x86_64-apple-darwin.tar.gz",
    f"julie-v{version}-x86_64-unknown-linux-gnu.tar.gz",
    f"julie-v{version}-x86_64-pc-windows-msvc.zip",
]
expected = set(archives + [f"{name}.sha256" for name in archives])
actual = {path.name for path in directory.iterdir() if path.is_file()}
if actual != expected:
    raise SystemExit(f"incomplete public asset set: expected {sorted(expected)}, got {sorted(actual)}")
for archive in archives:
    checksum = (directory / f"{archive}.sha256").read_text().split()
    if len(checksum) < 2 or checksum[1] != archive or checksum[0] != hashlib.sha256((directory / archive).read_bytes()).hexdigest():
        raise SystemExit(f"public archive checksum mismatch: {archive}")
