# Windows baseline for retrieval work

Host source baseline: `22391e26882e54b9e4f5bb0995a69369793e067c` (merged Plan1).
Guest checkout: `C:\work\revival-retrieval` on native NTFS.
Control plane: updated `win-test` CLI; no direct SSH/virsh or shared-mount tests.

The guest has Rust/Cargo1.97 MSVC and now nextest0.9.144. Installation used the documented `cargo install cargo-nextest --locked` command ([nextest instructions](https://nexte.st/docs/installation/from-source/)). The installer wrapper lingered after child exit and was terminated; availability was separately verified by the executable version. Its termination is not recorded as a successful installer command.

The first `cargo xtask test full` completed with failure, with an authoritative emitted log at `/home/murphy/.local/share/win-test/logs/20260912T133218Z-revival-retrieval-1392788.log`:

- Julie binary build passed in402.1seconds.
- Workspace test compilation failed in475.7seconds with E0433/E0599: `std::os::unix` and `Permissions::from_mode` in `julie-pipeline/src/tests/native_child.rs`.
- No test-suite pass is claimed for that run.

All four tests in that module already had `#[cfg(unix)]` and use POSIX shell fixtures. Adding a module-level `#![cfg(unix)]` applies the same condition to the shared helper/imports. It removes no previously enabled Windows test. The corrected Windows gate remains pending until a clean commit is synced and tested.

A clean read-only export clone under the task's ignored `.razorback/sdd/revival-retrieval/windows-source/revival-retrieval` permits exact-commit syncing while host workers have unrelated in-flight diffs. All implementation edits remain in the primary task worktree; the export is only verification input. Keeping its basename preserves the warmed guest checkout and target directory.
