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

## Second baseline run

The corrected `c0e3828f827451b01131e4310270b4b39e57e72b` run compiled successfully and ran111/2237 development tests before fail-fast:109passed,2failed. Log: `/home/murphy/.local/share/win-test/logs/20260912T140002Z-revival-retrieval-1419592.log`.

The existing-path CLI fixture assumed `/tmp`; it now uses the platform temp directory. Standalone rebuild failed with Windows error32 because it deleted files before releasing the loaded workspace and reference cache. Rebuild now captures its resolved index path before teardown, stops/releases only the matching workspace/cache, then acquires the existing mutation gate before deleting/reindexing. Capturing the path first preserves rebound standalone storage anchors. Stopping the watcher before taking the gate avoids joining a task blocked on that same gate.

Focused host checks pass: `invalidate_checkout_store_releases_only_matching_workspace_handles`, `test_resolve_workspace_root_with_existing_path`, and `test_run_cli_tool_standalone_workspace_rebuild_reindexes_the_path` (one selected each), plus `cargo check`. The clean-commit NTFS rerun remains the proof for Windows file-handle release.

This repair does not add global request/embedding quiescence or claim full concurrent runtime eviction. Those lifecycle guarantees remain in Plan3. Existing explicit rebuild still fails rather than silently unlinking files that another active reader or embedding job retains.
