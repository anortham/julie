#!/usr/bin/env python3
"""Verify a native Julie release archive before it becomes public."""

import argparse
import hashlib
import json
import os
import stat
import subprocess
import tarfile
import tempfile
import time
import zipfile
from pathlib import Path


def fail(message: str) -> None:
    raise SystemExit(f"release archive verification failed: {message}")


def flat_name(name: str) -> str:
    name = name.removeprefix("./")
    if not name or name.startswith("/") or "/" in name or "\\" in name:
        fail(f"archive entry is not flat: {name}")
    return name


def members(archive: Path) -> set[str]:
    names = set()
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            for item in package.infolist():
                if item.is_dir() or item.filename.startswith("__MACOSX/"):
                    continue
                mode = item.external_attr >> 16
                if stat.S_ISLNK(mode) or (mode and not stat.S_ISREG(mode)):
                    fail(f"archive contains non-regular entry: {item.filename}")
                name = flat_name(item.filename)
                if name in names:
                    fail(f"archive contains duplicate path: {name}")
                names.add(name)
    else:
        with tarfile.open(archive) as package:
            for item in package.getmembers():
                if item.isdir():
                    continue
                if not item.isfile():
                    fail(f"archive contains non-regular entry: {item.name}")
                name = flat_name(item.name)
                if name in names:
                    fail(f"archive contains duplicate path: {name}")
                names.add(name)
    return names


def extract(archive: Path, destination: Path) -> None:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            package.extractall(destination)
    else:
        with tarfile.open(archive) as package:
            package.extractall(destination)


def run(command: list[str], env: dict[str, str], input_text: str | None = None):
    return subprocess.run(command, env=env, input=input_text, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=False, timeout=30)


def initialize_instructions(output: str) -> str | None:
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            fail("packaged stdio server returned malformed initialize JSON")
        if message.get("id") == 1:
            result = message.get("result")
            if isinstance(result, dict) and isinstance(result.get("instructions"), str):
                return result["instructions"]
            return None
    return None


def wait_for_service_record_removal(path: Path, timeout_seconds: float = 1.0) -> None:
    deadline = time.monotonic() + timeout_seconds
    while path.exists():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            fail(f"packaged stdio probe left service record after {timeout_seconds:.1f}s")
        time.sleep(min(0.02, remaining))


def verify_manifest(root: Path, entries: set[str]) -> None:
    try:
        manifest = json.loads((root / "package-manifest.json").read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"malformed sidecar manifest: {error}")
    source, files = manifest.get("source"), manifest.get("files")
    if not isinstance(source, dict) or not isinstance(source.get("files"), list):
        fail("sidecar manifest has no source files list")
    if not isinstance(files, list) or not files:
        fail("sidecar manifest has no files list")
    expected = entries - {"package-manifest.json"}
    paths = set()
    for item in files:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            fail("sidecar manifest has a malformed file entry")
        path = flat_name(item["path"])
        checksum = item.get("sha256")
        if path in paths:
            fail(f"sidecar manifest has duplicate path: {path}")
        if not isinstance(checksum, str) or len(checksum) != 64 or any(char not in "0123456789abcdef" for char in checksum):
            fail(f"manifest file has an invalid checksum: {path}")
        file_path = root / path
        if path not in entries or not file_path.is_file() or file_path.is_symlink():
            fail(f"manifest file missing from archive: {path}")
        if hashlib.sha256(file_path.read_bytes()).hexdigest() != checksum:
            fail(f"manifest checksum mismatch: {path}")
        paths.add(path)
    if paths != expected:
        fail("sidecar manifest does not cover every packaged file")
    for item in source["files"]:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str) or flat_name(item["path"]) not in paths:
            fail("sidecar source manifest has a malformed file entry")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--sidecar-version", required=True)
    parser.add_argument("--sha256", type=Path, required=True)
    parser.add_argument("--windows", action="store_true")
    args = parser.parse_args()

    expected_checksum = args.sha256.read_text().split()[0]
    if hashlib.sha256(args.archive.read_bytes()).hexdigest() != expected_checksum:
        fail("archive checksum does not match its .sha256 file")
    entries = members(args.archive)
    suffix = ".exe" if args.windows else ""
    server, sidecar = f"julie-server{suffix}", f"julie-semantic-sidecar{suffix}"
    required = {server, sidecar, "package-manifest.json", "README.md", "LICENSE"}
    missing = required - entries
    if missing:
        fail(f"archive is missing: {', '.join(sorted(missing))}")

    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        extract(args.archive, root)
        verify_manifest(root, entries)
        env = os.environ | {"JULIE_HOME": str(root / "home"), "JULIE_SESSION_HOOKS": "0"}
        server_path, sidecar_path = root / server, root / sidecar
        server_version = run([str(server_path), "--version"], env)
        if server_version.returncode or server_version.stdout.strip() != f"julie-server {args.version}":
            fail("packaged server version does not match release version")
        sidecar_version = run([str(sidecar_path), "--version"], env)
        if sidecar_version.returncode or sidecar_version.stdout.strip() != f"julie-semantic-sidecar {args.sidecar_version}":
            fail("packaged sidecar identity does not match the pinned release")
        initialize = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "release-qualification", "version": "1"}}}) + "\n"
        try:
            result = run([str(server_path)], env, initialize)
            instructions = initialize_instructions(result.stdout)
            if result.returncode or not instructions or "workspace" not in instructions.lower():
                fail("packaged stdio server did not return workspace instructions")
        finally:
            stop = run([str(server_path), "service", "stop"], env)
        if stop.returncode:
            fail(f"packaged service stop exited {stop.returncode}")
        wait_for_service_record_removal(root / "home" / "service.json")


if __name__ == "__main__":
    main()
