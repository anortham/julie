# Julie Rustfmt Normalization Design

**Status:** Approved for implementation planning.

## Purpose

Finish the remaining Julie quality-roadmap work by establishing the repository-pinned Rust 1.97.0 formatter as a clean, reproducible baseline. The current `cargo fmt --check` baseline reports drift in 85 Rust files.

## Architecture Quality

No Architecture Impact. This is a repository-wide mechanical normalization. It must not change runtime behavior, public interfaces, dependencies, toolchain configuration, supported platforms, generated artifacts, fixtures, or non-Rust source files.

## Design

Run the repository's existing `cargo fmt` command once from the clean roadmap worktree under `rust-toolchain.toml`, which pins Rust 1.97.0 and includes `rustfmt`. There is no repository rustfmt override file, so the pinned toolchain's stable defaults are the canonical format.

Keep the formatter output as one coherent mechanical implementation commit. Do not mix manual cleanup, comment rewriting, module restructuring, dependency changes, or opportunistic fixes into that commit. Review the complete diff before committing, with special attention to import and module-declaration reorderings, macro bodies, string literals, and generated-looking files.

The mechanical commit may touch implementation and test files across workspace crates, but file ownership remains with the formatter: every changed Rust file must be reproducible by running `cargo fmt` from the pre-normalization commit. Any diff that cannot be reproduced that way is out of scope and must be removed or split into a separately approved change.

## Verification

- Capture the failing pre-change `cargo fmt --check` baseline and the exact set of affected files.
- Apply `cargo fmt`, then require `cargo fmt --check` and `git diff --check` to pass.
- Confirm no non-Rust files changed in the mechanical commit.
- Review formatter-sensitive reorderings and confirm no public interface, dependency, feature, or platform configuration changed.
- Run `cargo check` and the repository's affected-change gate.
- Run `cargo xtask test dev` and `cargo xtask test full` before closeout because the normalization spans all workspace crates.
- Record every gate with its exact commit SHA in a Phase 4 verification ledger.

## Delivery

The normalization stays on `codex/julie-improvement-roadmap`. It is committed locally after review and verification. No push, merge, publish, deploy, tag, or release occurs without separate explicit approval.

## Acceptance Criteria

- [ ] `cargo fmt --check` passes under the pinned Rust 1.97.0 toolchain.
- [ ] The mechanical commit contains only reproducible rustfmt output in Rust files.
- [ ] No public interface, dependency, feature, platform, or runtime behavior changes are introduced.
- [ ] Compile, affected-change, dev, and full verification gates pass on recorded exact SHAs.
- [ ] The Phase 4 ledger and active Goldfish brief mark the Julie quality roadmap complete.
- [ ] The final task worktree is clean and no external integration action occurs.
