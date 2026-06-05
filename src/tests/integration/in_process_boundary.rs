//! T12 — in-process boundary tripwire (Phase 3d.1).
//!
//! Updated from 3c.3: adapter module, `http_client.rs`, and `julie-adapter` binary
//! deleted in 3d.1. Remaining §7-DAG daemon server files stay bypassed until 3d.2/3d.3.
//! Two guarantees:
//!
//!   1. **No-args path serves in-process.** `src/main.rs`'s `None =>` arm calls
//!      `run_in_process_server` and NEVER `run_adapter` / `DaemonLauncher`. The
//!      old fork-daemon-and-bridge-stdio path is gone from the default entry.
//!   2. **The remaining §7-DAG files still exist.** The daemon HTTP transport,
//!      singleton/legacy/pid, search_compare, and migration.rs are still present
//!      — bypassed, not deleted (deletion is 3d.2/3d.3).
//!   3. **The bypassed daemon code still compiles.** This test lives in the `julie`
//!      lib crate, whose `lib.rs` declares `pub mod daemon;`. The explicit
//!      `start_daemon` path reference below keeps the daemon CLI entry load-bearing.

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

/// Guarantee 3 (compile-time): force the bypassed daemon entry symbol to still resolve.
/// If `start_daemon` is removed before Phase 3d.2, this fails to COMPILE — a louder,
/// earlier signal than the runtime assertions below.
#[allow(dead_code)]
fn _bypassed_entry_points_still_compile() {
    // The daemon lifecycle entry is still reachable via the `daemon` subcommand.
    let _daemon_entry = crate::daemon::cli::start_daemon;
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
    let none_arm_line = none_arm_line.expect("src/main.rs must still have a `None =>` (no-args) arm");

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

/// Guarantee 2: every file the later 3d sub-PRs (3d.2/3d.3) will remove is still
/// present on the 3d.1 branch. Bypassed, not deleted.
/// (adapter/**, http_client.rs, julie-adapter.rs were deleted in 3d.1.)
#[test]
fn section7_dag_files_are_bypassed_not_deleted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Remaining §7-DAG files after 3d.1 deletions. Grouped by design-doc shorthand.
    let section7_files: &[&str] = &[
        // daemon HTTP transport
        "src/daemon/http_transport.rs",
        "src/daemon/transport.rs",
        // singleton / legacy / pid
        "src/daemon/singleton.rs",
        "src/daemon/legacy_migration.rs",
        "src/daemon/pid.rs",
        // search_compare
        "src/daemon/database/search_compare.rs",
        // migration.rs
        "src/migration.rs",
    ];

    let missing: Vec<&str> = section7_files
        .iter()
        .copied()
        .filter(|rel| !root.join(rel).exists())
        .collect();

    assert!(
        missing.is_empty(),
        "Phase 3d.1 deleted adapter/http_client but MUST NOT delete the remaining \
         §7-DAG daemon server files (deletion is 3d.2/3d.3). Missing: {missing:?}"
    );
}
