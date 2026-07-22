# macOS Release Toolchain Verification Ledger

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Repository build inputs pin official Rust 1.97.0 and macOS 11.0 | `cargo nextest run -p xtask --test toolchain_contract_tests toolchain_contract_pins_release_build_inputs` | worker-exact | `c0154dba3c3c69a64b1edc3a788855db5904286f` | pass: 1/1 | 2026-07-22T12:39:51Z | no |
| Fresh login shells resolve rustup proxies and the pinned toolchain | `zsh -lic 'cd /Users/murphy/source/julie/.worktrees/julie-improvement-roadmap && command -v cargo && command -v rustc && cargo --version && rustc --version && rustup show active-toolchain'` | machine-shell | `c0154dba3c3c69a64b1edc3a788855db5904286f` | pass: both proxies under `/opt/homebrew/opt/rustup/bin`; Cargo and Rust 1.97.0 | 2026-07-22T12:39:51Z | no |
| Workspace compiles with the pinned toolchain | `cargo check` | lead-compile | `c0154dba3c3c69a64b1edc3a788855db5904286f` | pass | 2026-07-22T12:39:51Z | no |
| Clean release build emits no object-version linker warnings | `cargo clean --release && cargo build --release --bin julie-server --bin julie-embedding-host` | release-build-clean | `c0154dba3c3c69a64b1edc3a788855db5904286f` | pass: 3m03s; no `linker stderr` or `newer than target minimum` | 2026-07-22T12:39:51Z | no |
| Release binary retains the supported macOS deployment target | `vtool -show-build target/release/julie-server` | release-binary-contract | `c0154dba3c3c69a64b1edc3a788855db5904286f` | pass: `minos 11.0` | 2026-07-22T12:39:51Z | no |

## TDD Evidence

- RED at `e3dc1eb9d4eaf9f87a3157d0a63660bae201ce13`: the exact contract test failed because `rust-toolchain.toml` did not exist.
- GREEN at `c0154dba3c3c69a64b1edc3a788855db5904286f`: the exact contract test passed with all repository build inputs aligned.

## Batch Evidence

Before the implementation commit, `cargo xtask test changed` mapped `.cargo/config.toml`, `README.md`, and the contract test to `xtask-runner`; the previously unmapped release workflow and toolchain file triggered the documented `dev` fallback. All 27 selected buckets passed in 269.0s warm. The final combined affected-change and branch gates are recorded in the Phase 2A ledger at the final implementation HEAD.
