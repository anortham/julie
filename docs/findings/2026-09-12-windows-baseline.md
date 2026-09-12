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

## Third baseline run

At `e1ee19f27d47d1a38270fac5dd76a9b425cf9f4a`, binary build passed and the development gate ran316tests:315passed,1failed. The standalone rebuild regression passed on NTFS. Exact emitted log: `/home/murphy/.local/share/win-test/logs/20260912T142540Z-revival-retrieval-1440634.log`.

The failure was a fixture pretending to change Windows' default home by setting `HOME` and `USERPROFILE`; `dirs::home_dir()` uses the Windows shell home lookup. The corrected coverage keeps simulated default-home traversal on Unix, tests explicit `JULIE_HOME` traversal on every platform, and directly verifies recognition of the actual default home without modifying its files. All three exact host tests passed; the next NTFS gate verifies the portable replacements.

The full command had finished in76.1seconds, but PowerShell's process-tree wait remained held by Visual C++ telemetry (`VCTIP.EXE`). Process inventory showed no remaining Cargo, nextest, xtask, or Julie process. Stopping only the identity-checked telemetry process created during this build immediately released the original wrapper with its actual failure status and log. No runner skill or unrelated process was modified.

## Retrieval and paging gate

At `13249765bc1424504cf90a0d847e41ae8e05c4dd`, the Windows binary built successfully. The development tier completed510passing tests before an index-upgrade fixture failed with error32. A concurrent MCP test remained alive past its deadlines and was deliberately terminated after209.966seconds; this is recorded as an abort, not a pass. Log: `/home/murphy/.local/share/win-test/logs/20260912T153447Z-revival-retrieval-1667299.log`.

The upgrade fixture now awaits the existing checkout teardown before dropping its old handler and reopening files. The concurrent fixture used a stop notification that could be lost between waits; it now stores a permit with `notify_one` and bounds the driver join. Concurrency and indexing assertions remain intact.

At clean `cae075a32a23c612940a2e0ac55823afcb33396a`, exact NTFS reruns passed:

- `test_concurrent_mcp_requests_do_not_wedge`:1passed in2.219seconds. Log: `/home/murphy/.local/share/win-test/logs/20260912T154440Z-revival-retrieval-1672274.log`.
- `out_of_date_schema_version_recreates_index_directory_and_reindexes`:1passed in1.749seconds. Log: `/home/murphy/.local/share/win-test/logs/20260912T154644Z-revival-retrieval-1672974.log`.

The concurrent rerun's wrapper again waited on build telemetry after the test process had exited. Process inventory confirmed no Cargo/test process remained before stopping only its identity-checked VCTIP process. The original runner then returned exit0 and its authoritative log. A final Windows full gate is still required after body retrieval lands.

## Final retrieval gate, first attempt

At `80d0da04c0116ea383ddb2d02d20949181dd2690`, the binary built in37.4seconds and the development tier ran613tests:612passed,1failed. Log: `/home/murphy/.local/share/win-test/logs/20260912T162441Z-revival-retrieval-1759484.log`. The sensitive-root request fixture expected `SENSITIVE_ROOT` but received `WORKSPACE_REQUIRED`; its root-path assumption is under correction. No full Windows pass is claimed. After Cargo and all test processes exited, the wrapper was released by stopping only its identity-checked build telemetry process.

At `1a4d804f310887ffee2088083b5089f80225ef9c`, both root-guard tests passed on NTFS. The development tier ran649tests:648passed,1failed. The service architecture budget test did not match its allowlisted path on Windows; the portable-path correction is in progress. Log: `/home/murphy/.local/share/win-test/logs/20260912T163025Z-revival-retrieval-1764942.log`.

At `d13a835d8e58eaa3cbdf15275b340e1dd1a41c03`, the portable architecture-budget check passed. The development tier ran666tests:665passed,1failed. `stale_record_is_removed_and_the_spawn_hook_runs_once` exceeded its one-second wall-clock assertion; diagnosis is in progress. Log: `/home/murphy/.local/share/win-test/logs/20260912T163427Z-revival-retrieval-1768108.log`.

## Service lifecycle diagnostics

At `85eb197164600f47873718f582f9e97409732bf7`, the known-dead PID is checked before HTTP, and the stale-record test additionally proves no connection reaches a bound listener. A narrowed Windows service-group run completed48tests:46passed,2process-fixture failures. Log: `/home/murphy/.local/share/win-test/logs/20260912T163939Z-revival-retrieval-1771784.log`. Because that command did not rebuild the server binary, it is not final executable evidence.

A fresh server build passed (`/home/murphy/.local/share/win-test/logs/20260912T164143Z-revival-retrieval-1772727.log`). The four process tests then ran against that binary:2passed and the same2failed (`/home/murphy/.local/share/win-test/logs/20260912T164241Z-revival-retrieval-1773238.log`). Both observed a one-second-idle service record only after a subprocess exited, leaving a lifecycle race under investigation.

At `9194498be7737fbf37e88bfa80d28f1458c5c673`, the full gate ran617tests:616passed,1intermittent cancellation-test failure. The source-preflight test returned a successful parse instead of cancellation; its synchronization is under correction. Log: `/home/murphy/.local/share/win-test/logs/20260912T164457Z-revival-retrieval-1775550.log`. This run stopped before the corrected process tests; it is not evidence for their Windows outcome.

At `9d8636b79148cab12b3af3ebbe94666f1b57497e`, the service-process and source-preflight cancellation corrections passed on NTFS. The development tier ran1039tests:1038passed,1failed. The lazy semantic-initialization fixture observed zero initialization attempts instead of one; diagnosis is in progress. Log: `/home/murphy/.local/share/win-test/logs/20260912T164953Z-revival-retrieval-1778797.log`.

At `24a4ebc0ada2115339085bade5502c04e24a9736`, the full gate ran220tests:219passed,1intermittent context-metrics failure (`get_context_records_result_count`, expected2, recorded0). Earlier runs had passed this test; diagnosis is in progress rather than treating a retry as a fix. Log: `/home/murphy/.local/share/win-test/logs/20260912T165545Z-revival-retrieval-1781897.log`.

At `2ad900d890284116a88052dfde01a140567b6e40`, all11Windows metrics tests passed in the narrowed group (`/home/murphy/.local/share/win-test/logs/20260912T170555Z-revival-retrieval-1787657.log`). The subsequent full run completed217tests:216passed,1failed at the newly added fixture graph postcondition, before calling deep_dive: graph.symbols was0instead of2 (`/home/murphy/.local/share/win-test/logs/20260912T170702Z-revival-retrieval-1788243.log`). This is positive evidence of a separate setup/index state issue under the full run; investigation continues. The mock-peer correction remains valid and was not used to remove the readiness assertion.
