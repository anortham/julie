//! T12 — in-process boundary tripwire (Phase 3d.2b-ii).
//!
//! History: adapter module, `http_client.rs`, and `julie-adapter` binary deleted
//! in 3d.1. The daemon HTTP-server runtime (`app/**`, `http_transport`, `transport`,
//! `mcp_session`, `token_file`, `singleton`, `fd_limit`, `shutdown_event`) and the
//! `WorkspacePool`/`WatcherPool` were deleted in 3d.2b-ii. 3d.3 deletes the
//! pid/discovery runtime surface and the retired search-compare data surface.
//! Two guarantees:
//!
//!   1. **No-args path serves in-process.** `src/main.rs`'s `None =>` arm calls
//!      `run_in_process_server` and NEVER `run_adapter` / `DaemonLauncher`. The
//!      old fork-daemon-and-bridge-stdio path is gone from the default entry.
//!   2. **The 3d.2b-ii deletions actually happened, and 3d.3 deleted surfaces are gone.**
//!      The daemon HTTP-server runtime + pool files MUST be gone; the pid/discovery
//!      runtime surface MUST be gone; the search-compare data surface MUST be gone.

use std::fs;
use std::path::Path;

/// Strip a single-line `//` comment so doc/comment mentions of `run_adapter`
/// (which legitimately appear in the cutover's explanatory comments) do not trip
/// the guard. Only the code portion before the first `//` is inspected.
fn code_part(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Guarantee 1: the no-args (`None =>`) arm of `main.rs` serves in-process and
/// does not touch the adapter/daemon-launch path.
#[test]
fn no_args_main_serves_in_process_not_adapter() {
    let main_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let content = fs::read_to_string(&main_rs).expect("read src/main.rs");

    // Collect code-only occurrences (comment-stripped) with line numbers.
    let mut in_process_line: Option<usize> = None;
    let mut none_arm_line: Option<usize> = None;
    let mut run_adapter_hits: Vec<usize> = Vec::new();
    let mut daemon_launcher_hits: Vec<usize> = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let code = code_part(line);
        let lineno = idx + 1;
        if code.contains("run_in_process_server") && in_process_line.is_none() {
            in_process_line = Some(lineno);
        }
        // The last match arm. `None =>` opens the no-args path.
        if code.contains("None =>") && none_arm_line.is_none() {
            none_arm_line = Some(lineno);
        }
        if code.contains("run_adapter") {
            run_adapter_hits.push(lineno);
        }
        if code.contains("DaemonLauncher") {
            daemon_launcher_hits.push(lineno);
        }
    }

    // The cutover must call the in-process server.
    let in_process_line = in_process_line.expect(
        "src/main.rs must call `run_in_process_server` — the no-args cutover (T10) is missing",
    );
    let none_arm_line =
        none_arm_line.expect("src/main.rs must still have a `None =>` (no-args) arm");

    // It must be wired into the no-args arm, not a helper: the call appears after
    // the `None =>` token (the None arm is the last match arm in main()).
    assert!(
        in_process_line > none_arm_line,
        "`run_in_process_server` (line {in_process_line}) must be inside the `None =>` \
         no-args arm (line {none_arm_line}); found it before the arm"
    );

    // The old adapter path must be fully gone from main.rs (it was only ever
    // called from the None arm). Comment mentions are stripped, so any hit here
    // is a real code reference = the cutover regressed.
    assert!(
        run_adapter_hits.is_empty(),
        "src/main.rs must NOT call `run_adapter` after the cutover — found code \
         reference(s) at line(s) {run_adapter_hits:?}. The no-args path serves \
         in-process now."
    );
    assert!(
        daemon_launcher_hits.is_empty(),
        "src/main.rs must NOT reference `DaemonLauncher` after the cutover — found \
         code reference(s) at line(s) {daemon_launcher_hits:?}."
    );
}

/// Guarantee 2a: the daemon HTTP-server runtime + pool files deleted in 3d.2b-ii
/// are actually gone. If a re-introduction or a missed deletion leaves any of these
/// present, this fails — the deletion contract is enforced, not just assumed.
#[test]
fn daemon_http_runtime_files_are_deleted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let deleted_in_3d2b_ii: &[&str] = &[
        // daemon HTTP-server runtime
        "src/daemon/app.rs",
        "src/daemon/http_transport.rs",
        "src/daemon/transport.rs",
        "src/daemon/mcp_session.rs",
        "src/daemon/token_file.rs",
        "src/daemon/singleton.rs",
        "src/daemon/fd_limit.rs",
        "src/daemon/shutdown_event.rs",
        // workspace/watcher pools
        "src/daemon/workspace_pool.rs",
        "src/daemon/watcher_pool.rs",
    ];

    let still_present: Vec<&str> = deleted_in_3d2b_ii
        .iter()
        .copied()
        .filter(|rel| root.join(rel).exists())
        .collect();

    assert!(
        still_present.is_empty(),
        "Phase 3d.2b-ii deletes the daemon HTTP-server runtime + pool files; these \
         MUST NOT exist. Still present: {still_present:?}"
    );
}

/// Guarantee 2b: the pid/discovery runtime surface deleted in 3d.3 Task 2 is
/// actually gone.
#[test]
fn pid_and_discovery_runtime_surface_deleted_in_3d3_task2() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let deleted_in_3d3_task2: &[&str] = &["src/daemon/pid.rs"];

    let still_present: Vec<&str> = deleted_in_3d3_task2
        .iter()
        .copied()
        .filter(|rel| root.join(rel).exists())
        .collect();

    assert!(
        still_present.is_empty(),
        "3d.3 Task 2 deletes the pid-file runtime; these files MUST NOT exist. \
         Still present: {still_present:?}"
    );

    let discovery_rs = fs::read_to_string(root.join("src/daemon/discovery.rs"))
        .expect("read src/daemon/discovery.rs");
    for symbol in ["DiscoveryRecord", "DiscoveryState", "DiscoveryFile"] {
        assert!(
            !discovery_rs.contains(symbol),
            "3d.3 Task 2 deletes the discovery.json reader/writer surface; \
             src/daemon/discovery.rs must not contain `{symbol}`"
        );
    }
}

/// Guarantee 2c: the retired search-compare data surface deleted in 3d.3 Task 5
/// is actually gone.
#[test]
fn search_compare_data_surface_deleted_in_3d3_task5() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let deleted_in_3d3_task5: &[&str] = &["src/daemon/database/search_compare.rs"];

    let still_present: Vec<&str> = deleted_in_3d3_task5
        .iter()
        .copied()
        .filter(|rel| root.join(rel).exists())
        .collect();

    assert!(
        still_present.is_empty(),
        "3d.3 Task 5 deletes the retired search-compare data surface; these files \
         MUST NOT exist. Still present: {still_present:?}"
    );
}
