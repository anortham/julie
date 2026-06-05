//! T12 — in-process boundary tripwire (Phase 3c.3).
//!
//! Pins the 3c/3d boundary so a reviewer can prove the cutover **bypassed** the
//! daemon/adapter rather than **deleting** it (deletion is Phase 3d). Three
//! guarantees, all cheap and source-level:
//!
//!   1. **No-args path serves in-process.** `src/main.rs`'s `None =>` arm calls
//!      `run_in_process_server` and NEVER `run_adapter` / `DaemonLauncher`. The
//!      old fork-daemon-and-bridge-stdio path is gone from the default entry.
//!   2. **The §7-DAG files still exist.** Every file the 3d deletion DAG will
//!      eventually remove (adapter/**, `bin/julie-adapter.rs`, the daemon HTTP
//!      transport, singleton/legacy/pid, search_compare, migration.rs) is still
//!      present on the 3c branch — bypassed, not deleted.
//!   3. **The bypassed code still compiles.** This test lives in the `julie` lib
//!      crate, whose `lib.rs` declares `pub mod adapter;` and `pub mod daemon;`.
//!      If any file under those modules were deleted (while still `mod`-declared)
//!      or stopped compiling, this test would fail to BUILD. The explicit
//!      `run_adapter` / `start_daemon` path references below make that guard
//!      load-bearing for the two entry symbols most likely to be pruned by
//!      mistake.

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

/// Guarantee 3 (compile-time): force the bypassed entry symbols to still resolve.
/// If the adapter module or `run_adapter` is removed before Phase 3d, this fails
/// to COMPILE — a louder, earlier signal than the runtime assertions below.
#[allow(dead_code)]
fn _bypassed_entry_points_still_compile() {
    // `run_adapter` — the old no-args entry, now bypassed by the cutover.
    let _adapter_entry = crate::adapter::run_adapter;
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

/// Guarantee 2: every file the Phase 3d deletion DAG (§7) will remove is still
/// present on the 3c branch. Bypassed, not deleted.
#[test]
fn section7_dag_files_are_bypassed_not_deleted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // The §7 deletion DAG, by concrete path (verified against the tree on the
    // 3c branch). Grouped by the design-doc shorthand for reviewer clarity.
    let section7_files: &[&str] = &[
        // adapter/**
        "src/adapter/mod.rs",
        "src/adapter/forwarder.rs",
        "src/adapter/http_stdio.rs",
        "src/adapter/launcher.rs",
        // bin/julie-adapter.rs
        "src/bin/julie-adapter.rs",
        // daemon HTTP transport
        "src/daemon/http_transport.rs",
        "src/daemon/transport.rs",
        "src/daemon/http_client.rs",
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
        "Phase 3c bypasses but MUST NOT delete the §7-DAG daemon/adapter files \
         (deletion is Phase 3d). Missing: {missing:?}"
    );
}
