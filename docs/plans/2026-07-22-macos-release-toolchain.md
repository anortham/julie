# macOS Release Toolchain Stabilization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make Julie's local and release-CI Rust builds use one official toolchain and preserve the macOS 11 deployment target without linker object-version warnings.

**Architecture:** Pull the reproducible-build slice of roadmap Phase 4 forward as a release prerequisite. Pin the official rustup toolchain in the repository, make native build scripts inherit the macOS 11 target, align release CI, and put Homebrew's rustup proxies ahead of Homebrew's host-targeted Rust on this machine; repository-wide rustfmt normalization remains in Phase 4.

**Tech Stack:** Rust 1.97.0 via rustup, Cargo configuration, GitHub Actions, zsh, lld.

**Architecture Quality:** The caller-facing interface is the build environment: repository toolchain metadata, Cargo's build-script environment, release CI, and the local shell. Risk is medium because the wrong compiler can produce objects newer than Julie's supported deployment target; no product runtime interface changes.

## Global Constraints

- Keep Julie's minimum macOS deployment target at `11.0`.
- Do not suppress linker messages globally.
- Pin the official rustup toolchain to `1.97.0`; do not use Homebrew Rust 1.97.1 for Julie builds on this machine.
- Preserve the existing lld linker configuration.
- Keep Linux and Windows build behavior unchanged apart from selecting the same Rust version.
- Defer the existing repository-wide rustfmt normalization to the remaining Phase 4 work.
- Follow TDD: add the failing build-contract test before changing repository configuration.

---

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/DEVELOPMENT.md`, `.cargo/config.toml`, `.github/workflows/release.yml`, and the approved roadmap design.

**Worker red/green scope:** `cargo nextest run -p xtask --test toolchain_contract_tests toolchain_contract_pins_release_build_inputs`.

**Worker ceiling:** The worker runs only the exact contract test once red and once green. The lead owns compilation, formatting diagnostics, shell validation, and the release build.

**Worker gate invariant:** The repository pin, Cargo deployment target, release workflow toolchain, and development guidance agree on Rust `1.97.0` and macOS `11.0`.

**Lead affected-change scope:** `cargo xtask test changed` after the repository and machine configuration changes are reviewed.

**Branch gate:** `cargo xtask test dev` once for the completed batch.

**Replay/metric evidence:** A clean `cargo build --release --bin julie-server --bin julie-embedding-host` under the rustup proxy must finish without `linker stderr` or `newer than target minimum`; build duration is report-only.

**Escalation triggers:** Any Linux or Windows workflow semantic change beyond the Rust version, any deployment target above `11.0`, or any warning that remains under the official toolchain blocks completion.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-macos-release-toolchain-verification.md`. Evidence is reusable only at the exact recorded HEAD and scope.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Pin and prove the release toolchain | None - serial | Create `rust-toolchain.toml`, `xtask/tests/toolchain_contract_tests.rs`, and `docs/plans/2026-07-22-macos-release-toolchain-verification.md`; modify `.cargo/config.toml`, `.github/workflows/release.yml`, `README.md`, `docs/DEVELOPMENT.md`, and `/Users/murphy/.zprofile` | Not applicable - single task. | Not applicable - single task. |

### Task 1: Pin and prove the release toolchain

**Files:**
- Create: `rust-toolchain.toml`
- Create: `xtask/tests/toolchain_contract_tests.rs`
- Create: `docs/plans/2026-07-22-macos-release-toolchain-verification.md`
- Modify: `.cargo/config.toml:1-6`
- Modify: `.github/workflows/release.yml:55-82`
- Modify: `README.md:523-531`
- Modify: `docs/DEVELOPMENT.md:1-58`
- Modify: `/Users/murphy/.zprofile:1-6`

**Interfaces:**
- Consumes: Cargo's `[env]` configuration, rustup's `rust-toolchain.toml` selection, `dtolnay/rust-toolchain@<version>`, and Homebrew rustup proxy binaries at `/opt/homebrew/opt/rustup/bin`.
- Produces: bare `cargo` and `rustc` commands in Julie selecting official Rust `1.97.0`, native macOS objects targeting `11.0`, and release CI using the same Rust version for every matrix target.

**Contract inputs:** The live failure uses `/opt/homebrew/Cellar/rust/1.97.1` whose `compiler_builtins` objects declare macOS `26.0`; the official rustup `1.97.0` objects declare macOS `11.0`. The control build completed warning-free when both `cargo` and `rustc` came from the rustup toolchain and `MACOSX_DEPLOYMENT_TARGET=11.0` was present. Implement against the official [rustup toolchain-file contract](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file), [Cargo `[env]` contract](https://doc.rust-lang.org/cargo/reference/config.html#env), and [`dtolnay/rust-toolchain` version/target inputs](https://github.com/dtolnay/rust-toolchain#inputs).

**File ownership:** Create `rust-toolchain.toml`, `xtask/tests/toolchain_contract_tests.rs`, and `docs/plans/2026-07-22-macos-release-toolchain-verification.md`; modify `.cargo/config.toml`, `.github/workflows/release.yml`, `README.md`, `docs/DEVELOPMENT.md`, and `/Users/murphy/.zprofile`

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

**What to build:** Add a minimal rustup toolchain file for `1.97.0` with `rustfmt` and `clippy`, and set `MACOSX_DEPLOYMENT_TARGET = "11.0"` in Cargo without overriding an explicit caller-provided value. Pin release CI to `dtolnay/rust-toolchain@1.97.0`, document the rustup requirement and Homebrew collision, and put `/opt/homebrew/opt/rustup/bin` ahead of Homebrew Rust in this machine's login-shell path.

**Approach:** Start with one repository contract test that reads all shipped configuration surfaces and fails against the current state. Keep lld enabled; the fix is selecting compatible compiler artifacts and targeting native C dependencies correctly. Validate the shell in a fresh login process before the release build so the build cannot accidentally reuse Homebrew `cargo` or `rustc`.

**Acceptance criteria:**
- [x] The exact contract test fails before configuration changes and passes after them.
- [x] `zsh -lic 'command -v cargo; command -v rustc; cargo --version; rustc --version'` resolves both tools through `/opt/homebrew/opt/rustup/bin` and reports `1.97.0` inside the Julie worktree.
- [x] `cargo build --release --bin julie-server --bin julie-embedding-host` passes with no object-version linker warning.
- [x] `vtool -show-build target/release/julie-server` reports minimum macOS `11.0`.
- [x] Release CI uses Rust `1.97.0` for all four matrix targets and keeps the existing target list and build command.
- [x] The verification ledger records the warning-free release evidence at the exact commit.
- [x] Worker-scope verification passes and the change is either committed by the worker or handed to the lead per commit mode.
