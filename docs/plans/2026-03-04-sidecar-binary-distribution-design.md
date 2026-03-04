# Embed Python Sidecar for Binary Distribution

**Date:** 2026-03-04
**Status:** Approved
**Scope:** `src/embeddings/sidecar_supervisor.rs`, `Cargo.toml`

## Problem

`sidecar_root_path()` uses `env!("CARGO_MANIFEST_DIR")` to locate the Python sidecar source at compile time. This hardcodes the build machine's absolute path into the binary, which means:

- **GitHub release binaries** look for a path that doesn't exist on the user's machine
- **`cargo install julie`** breaks because the temporary build directory is cleaned up after install
- Only running from source checkout works today

## Solution: Embed + Extract

Embed the Python sidecar source files into the Rust binary at compile time using `include_dir`. On first use, extract them to the user's cache directory.

### Fallback Chain

`sidecar_root_path()` resolves the sidecar location using this priority:

1. **`JULIE_EMBEDDING_SIDECAR_ROOT` env var** — override, always wins
2. **Adjacent to binary** — `<exe_dir>/python/embeddings_sidecar/pyproject.toml` exists → use it (for packagers: Homebrew, scoop, distro packages)
3. **Source checkout** — `CARGO_MANIFEST_DIR/python/embeddings_sidecar/pyproject.toml` exists → use it (dev mode)
4. **Extract embedded files** — write to cache dir, return that path (distributed binary fallback)

### What Gets Embedded

5 files from `python/embeddings_sidecar/` (excluding `tests/`), ~24KB total:

```
pyproject.toml
sidecar/__init__.py
sidecar/main.py
sidecar/runtime.py
sidecar/protocol.py
```

### Extraction Location

```
<managed_cache_base_dir>/embeddings/sidecar/source/
├── pyproject.toml
├── sidecar/
│   ├── __init__.py
│   ├── main.py
│   ├── runtime.py
│   └── protocol.py
└── .embedded-version
```

This sits alongside the existing venv path (`<managed_cache_base_dir>/embeddings/sidecar/venv/`).

### Version Tracking

A `.embedded-version` file in the extraction dir contains `INSTALL_MARKER_VERSION`. On each call:
- Missing or mismatched → extract all files, write marker
- Matches → skip extraction

This piggybacks on the existing `INSTALL_MARKER_VERSION` constant — bumping it triggers both re-extraction and pip re-install.

### Editable Install Compatibility

The current install uses `pip install --editable .[runtime]`, which creates a `.pth` reference back to the source directory rather than copying files into site-packages. This means:

- The extracted source directory must persist (cache dir is permanent — fine)
- On upgrade, re-extracted code is picked up immediately
- No changes needed to `ensure_sidecar_package_installed()`

### API Change

`sidecar_root_path()` changes from `-> PathBuf` to `-> Result<PathBuf>` to propagate extraction errors cleanly. The only caller (`build_sidecar_launch_config()`) already returns `Result`.

## New Dependency

- `include_dir` crate (compile-time directory embedding)

## What's NOT Changing

- `managed_venv_path()` / `managed_cache_base_dir()` — already work correctly
- `ensure_sidecar_package_installed()` — already path-agnostic
- Sidecar launch and protocol — Python code is identical regardless of source location
- All env var overrides — continue to work as before

## Testing

- **Unit tests:** Fallback chain priority, extraction writes correct files, version marker triggers re-extraction
- **Integration:** Build binary, run without source checkout, verify extraction + path resolution
- **Manual:** Release binary in temp dir, confirm end-to-end embeddings work
