# Testing Speed Optimization

**Date:** 2026-05-08
**Status:** Draft

## Problem

AI agents spend ~95% of implementation time running tests and ~5% doing actual engineering. Even a single-test-by-name `cargo nextest run --lib <test>` takes 30-60 seconds due to dependency compilation overhead.

## Root Cause

`[profile.dev.package."*"]` with `opt-level = 2` forces **all dependencies** to compile with optimization level 2 in debug mode. This means tantivy, rusqlite, tree-sitter C parsers, axum, tokio, and every other dep gets re-optimized on every compilation — even for a single unit test.

With debug dogfooding using `--release`, the optimization for the dev profile serves no purpose in test workflows.

## Changes

### Step 1: Remove `opt-level=2` from dev deps (Cargo.toml)

**File:** `Cargo.toml`
**Action:** Remove lines 141-142 (`[profile.dev.package."*"]` section)

```diff
 [profile.dev]
 opt-level = 0
 debug = true

-[profile.dev.package."*"]
-opt-level = 2
```

**Effect:**
| Scenario | Before | After |
|---|---|---|
| Cold `cargo nextest run --no-run --lib` | ~5-10 min | ~1-3 min |
| Incremental single test | ~30-60s | ~5-15s |
| `cargo check` | ~5-15s | ~2-5s |
| `cargo build` (debug) | ~2-5 min | ~30-60s |

**Risk:** None. Debug builds are only used for CLI tool testing (not live MCP sessions). Release builds are unaffected.

### Step 2: Add nano test tier (test_tiers.toml)

**File:** `xtask/test_tiers.toml`
**Action:** Add a nano tier pointing to the two fastest, most general buckets.

```diff
 [tiers]
+nano = ["core-database", "core-fast"]
 smoke = ["cli", "core-database", "core-embeddings", "tools-get-context"]
```

No code changes needed — the runner dynamically resolves tiers from the manifest via `manifest.tiers.get()`.

**Effect:** `cargo xtask test nano` runs ~25s expected (core-database: 5s + core-fast: 20s). Useful as a "did I break anything obvious?" pre-check.

### Step 3: Update AGENTS.md (agent guidelines)

**File:** `AGENTS.md`

**3a. Quick Reference** — Add `cargo check`, `cargo xtask test nano`:

```diff
-cargo build                    # Debug build (fast iteration)
+cargo check                    # Type-check only (fastest compilation, no binary)
+cargo build                    # Debug build
-cargo nextest run --lib <test_name>  # Default: narrowest test first (seconds)
+cargo nextest run --lib <test_name>  # Default: narrowest test first
+cargo xtask test nano          # Minimal regression check (~25s)
```

**3b. Canonical Test Tiers** — Add nano row:

```diff
+| **Nano** | `cargo xtask test nano` | Fastest buckets for quick sanity | Ultra-tight loop after compilation fix |
```

**3c. Fast Feedback Loop** — New section after Default Workflow:

```markdown
### 🔥 Fast Feedback Loop (Edit → Verify)

With the opt-level=2 removal, compilation is now fast enough for a tight
edit-test loop. Follow this flow during implementation:

1. **`cargo check`** — Type-checks only, no codegen. Use FIRST after any code
   change to catch compilation errors in ~2-5 seconds.
2. **`cargo nextest run --lib <exact_test_name>`** — After cargo check passes,
   run the specific test. This now takes ~5-15 seconds for incremental rebuilds.
3. **Batch before broader testing** — Make 3-5 edits before running
   `cargo xtask test changed` or `cargo xtask test dev`.
4. **`cargo xtask test nano`** — For a quick "did I break anything?" sanity
   check (~25s) between batches, without running the full dev tier.
```

**3d. sccache recommendation** — Add to Development section:

```markdown
### sccache (Optional, Recommended)

Install [sccache](https://github.com/mozilla/sccache) to cache compiled
artifacts across branches and clean builds:

```bash
brew install sccache
```

Then either set the environment variable:
```bash
export RUSTC_WRAPPER=sccache
```

Or add to `.cargo/config.toml`:
```toml
[build]
rustc-wrapper = "sccache"
```

This helps most when switching branches (common for AI agents) and on clean
builds after `cargo clean`.
```

### Step 4: sccache setup

**File:** `.cargo/config.toml` (optional user action)

Add the `[build]` section:
```toml
[build]
rustc-wrapper = "sccache"
```

This is intentionally documented in AGENTS.md rather than auto-applied — sccache
requires separate installation and not all developers will want it.

## Acceptance Criteria

- [ ] `cargo nextest run --lib some_test` takes ≤15s for incremental rebuilds
- [ ] `cargo xtask test nano` runs and reports PASS for the two buckets
- [ ] `cargo xtask test list` shows nano tier in its output
- [ ] `cargo check` catches compilation errors before test runs
- [ ] AGENTS.md reflects the new workflow (cargo check + nano tier + sccache)
- [ ] `cargo build --release` is completely unaffected
- [ ] `cargo build` (debug) still produces a working binary for CLI tool testing

## Verification

```bash
cargo check                                            # Fast type-check
cargo nextest run --lib tests::core::database           # Test compilation speed
cargo xtask test nano                                  # New tier works
cargo xtask test list | grep -i nano                   # Listed in tiers
cargo build --release                                  # Unaffected
```
