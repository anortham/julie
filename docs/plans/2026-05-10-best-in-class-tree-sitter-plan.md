# Best-in-Class Tree-Sitter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Close every open gap in `fixtures/extraction/capabilities.json`, refresh all generated tree-sitter evidence at HEAD, harden `julie-extractors` as a reusable Rust crate, and produce 22-repo real-world evidence with semantic correctness specs — driven by the rubric at `docs/plans/2026-05-10-best-in-class-tree-sitter-rubric.md`.

**Architecture:** Treat this as a correctness program with one architecture lever (Pillar 3 public surface). Sequence: validators that prevent contradictory closures → shared contract regression bar → relationship/pending shape implementations → per-tier language work → crate hardening → real-world evidence regen → doc cleanup → release gates. Every per-language change is TDD: failing test → minimal extractor change → narrow targeted test → commit.

**Tech Stack:** Rust 2021, tree-sitter 0.26.8, `cargo nextest`, `cargo xtask` runners, Tantivy, SQLite via rusqlite, serde/serde_json, tree-sitter parser crates per language, the existing `crates/julie-extractors/` workspace member.

**Architecture Quality:** Approved per the design doc (`docs/plans/2026-05-10-best-in-class-tree-sitter-design.md` §"Architecture Impact Assessment"): one read-only public function `capability_snapshot()`, one public constant `EXTRACTION_CONTRACT_VERSION`, one example binary, doc comments on every existing public item. **`fixtures/extraction/capabilities.json` is the single source of truth and stays at that path** (44 in-repo references already point there per `mcp__julie__fast_search`). The crate reads it via `include_str!("../../../fixtures/extraction/capabilities.json")` from `crates/julie-extractors/src/capability_snapshot.rs`; this works for path-dep and git-dep consumers because cargo serves the whole repo at the actual on-disk location. It does NOT work for `cargo publish` to crates.io, and Pillar 3 has been scoped to "Rust path/git dependency usable" — not crates.io publishable — to match the inherent git deps (`tree-sitter-qmljs`, `tree-sitter-razor`, `tree-sitter-powershell`, `tree-sitter-vb-dotnet`) which have no crates.io equivalents. No module restructure. No breaking refactor — main `julie` crate keeps re-exporting through `src/extractors/mod.rs`. **Architecture risk:** The capability-snapshot data lifetime (`&'static`) commits us to load-once semantics; a reload mechanism is explicitly out of scope. **Worker rule:** if code reality contradicts the approved shape, report a plan mismatch instead of redesigning locally.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` (Quick Reference table, §🚨 RUNNING TESTS, §🔥 Fast Feedback Loop), `docs/TESTING_GUIDE.md`, `docs/TREE_SITTER_QUALITY_BAR.md` (Release Gates table).

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10` for the test the worker just wrote, or `cargo nextest run -p julie-extractors --lib <exact_test_name>` for extractor-crate tests. After `cargo check` passes. This is the default during the local edit loop.

**Worker ceiling:** Workers MUST NOT run `cargo xtask test changed`, `cargo xtask test dev`, or any xtask tier. Workers MUST NOT run `cargo nextest run --lib` without a specific filter. Workers run two test invocations per fix (RED → GREEN); diagnose failures rather than retry. The lead orchestrating session handles regression checks. (See CLAUDE.md "🚨 Subagent & Worker Agent Test Rules".)

**Worker gate invariant:** Each worker-owned test names the exact behavior it proves. Generic "smoke" tests are banned. The targeted regression must fail before the implementation change and pass after, with the failing assertion describing the missing structured-pending field, the wrong relationship endpoint, or the violated invariant — not "function returns something."

**Lead affected-change scope:** `cargo xtask test changed` after a coherent batch of edits. If `changed` falls back to `dev`, accept it (shared infrastructure moved).

**Branch gate:** `cargo xtask test dev` once per phase boundary. Not per edit, not per language.

**Release-gate scope (worktree HEAD):** the full Quality Bar table from `docs/TREE_SITTER_QUALITY_BAR.md`:
- `cargo fmt --check`
- `git diff --check`
- `cargo xtask certify tree-sitter --check`
- `cargo xtask test bucket extractors`
- `cargo xtask test bucket parser-upgrade`
- `cargo xtask test changed`
- `cargo xtask test system`
- `cargo xtask test dogfood`
- `cargo xtask test full`
- `cargo build --release`
- `cargo build --examples -p julie-extractors`
- `cargo test -p julie-extractors --doc`
- `cargo doc -p julie-extractors --no-deps`
- `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate` (Pillar 3 gate; replaces `cargo package --list`)

**Replay/metric evidence:** Real-world correctness specs in §6 are the only metric-style assertions. Each spec entry (named symbol with reference count, parent_id link, identifier span) is a hard gate; the existing `min_files` / `min_language_files` / `min_symbols` count thresholds remain as a secondary safety net. There are no report-only metrics in this plan — every assertion is a hard gate.

**Escalation triggers:** Any of the conditions below escalate the lead from `gpt-5.5 high` (per RAZORBACK.md routing):
- A worker reports the same failing test passes for the wrong reason (e.g., test asserts non-empty array, implementation returns a placeholder).
- The structured-pending shape contract assertion fails for a language whose extractor was not modified in the same batch — indicates shared-contract regression in §2.
- `cargo xtask test full` fails after a phase that should not have touched the failing tier (e.g., system tier fails after a per-language fixture phase).
- The downstream-smoke integration test (Task 5.7) fails to build or run, indicating the crate is no longer usable as a path dependency.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate. Workers MUST NOT loosen a test to make it pass.

**Verification ledger:** Append a row to the table at the bottom of this plan after each phase boundary and after the final release-gate sweep. Columns: invariant, command, scope label, commit SHA, result (Pass/Fail with summary), timestamp (UTC ISO-8601), evidence reused (Yes/No). If the same HEAD already has a passing ledger entry for the required scope, reuse that evidence instead of rerunning the same expensive gate.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` (Model Routing table at the top of the file).

**Strategy tier:** Phase boundaries, sequencing decisions, contract design (§1.1, §1.3, §3.1 SQL pending shape, §5.1–5.3 Pillar 3 API), gap-classification adjudication, plan-mismatch triage, escalations.
- Harness mapping: Claude Code Opus, or `gpt-5.5 high` via Codex when delegated.

**Implementation tier:** Bounded worker tasks from this plan: per-language fixture additions, extractor function migration to a structured-full variant, JSON/TOML relationship implementation, real-world spec authoring per repo, doc-comment additions for the public API audit.
- Harness mapping: Claude Code Sonnet for narrow worker tasks, or `gpt-5.4-mini xhigh` via Codex.

**Mechanical tier:** Doc deletions (Phase 7), `capabilities.json` `evidence` field rewrites (after the schema migration), regenerated cert report commits, ledger-row appends.
- Harness mapping: Claude Code Haiku, or `gpt-5.4-mini low/medium` via Codex.

**Coupled implementation tier:** Cross-file work after the lead has fixed the shared contract — Phase 2 tightening, Phase 4 per-language changes that touch both the extractor and the registry macro.
- Harness mapping: Claude Code Sonnet high, or `gpt-5.3-codex high` via Codex.

**Gate-interpretation reviewer:** Reading the rubric, a failing test, and the diff to decide whether the test or the implementation is wrong. Used when a worker reports a failure they cannot resolve.
- Harness mapping: Claude Code Opus, or `gpt-5.3-codex high` via Codex.

**Escalation tier:** Subtle correctness in pending-shape resolution, the SQL no-pending migration (§3.1), the Pillar 3 build-time data inclusion mechanic (§5.1), repeated worker failure on the same gap.
- Harness mapping: Claude Code Opus, or `gpt-5.5 high/xhigh` via Codex.

**Worker eligibility:** Implementation-tier workers may take any task in Phases 4, 5 (except 5.1–5.3), 6.3, 6.5, 7.1–7.4. They may NOT take Phases 1, 2, 3, 5.1–5.3, 6.4. Coupled-implementation tier may take any task in Phases 3 or 4.

**Escalation triggers:** Same caller-callee pending collision returns despite §2 fix; SQL migration breaks an unrelated language's tests; Pillar 3 packaging fails after fix attempts; >5 escalation files open simultaneously.

**Mechanical exclusion:** Mechanical workers cannot own failing-test gates. Phase 7 mechanical edits ride alongside the failing test that proves the gate, owned by an implementation worker.

**Unsupported harness behavior:** If the harness cannot select models per agent (Cursor IDE-level), record `inherit` in the worker prompt and proceed. Record the limitation in the verification ledger row's evidence column.

---

## File Structure

```
crates/julie-extractors/
├── Cargo.toml                            # No path/data changes needed for capabilities.json (file stays at fixtures/)
├── build.rs                              # ABSENT — we deliberately use include_str!, not a build script
├── examples/
│   └── extract_file.rs                   # CREATE (Phase 5.6)
├── tests/
│   └── downstream_smoke.rs               # CREATE (Phase 5.7): integration test that spawns a tempdir consumer crate
└── src/
    ├── lib.rs                            # MODIFY: doc, capability_snapshot(), EXTRACTION_CONTRACT_VERSION
    ├── registry.rs                       # MODIFY: macro audit, per-language migration
    ├── capability_snapshot.rs            # CREATE (Phase 5.2): typed CapabilitySnapshot + parser; include_str!("../../../fixtures/extraction/capabilities.json")
    ├── base/
    │   └── relationship_resolution.rs    # READ-ONLY shape reference for §2 assertions
    ├── tests/
    │   ├── capability_matrix.rs          # MODIFY: typed evidence schema, structured-pending shape, exception rule
    │   └── pending_shape_contract.rs     # CREATE (Phase 2.1): per-language structured-pending shape contract
    └── <language>/                       # MODIFY per Phase 4 task — emit structured pending where applicable

fixtures/extraction/
├── capabilities.json                     # SINGLE SOURCE OF TRUTH — stays here; crate consumes via include_str!
├── tree-sitter-real-world-corpus.toml    # MODIFY: add VB.NET row, raise min_relationships, add representative_specs
├── <language>/basic/
│   ├── source.<ext>                      # MODIFY per Phase 4: add cross-file/unresolved reference shapes
│   └── expected.json                     # MODIFY per Phase 4: assert structured_pending_relationships fields
├── <language>/negative/                  # CREATE per Phase 4: encode wrong-edge scenarios per language
└── vbnet/basic/source.vb.* / expected.json  # ALREADY EXISTS — Phase 4 fixture work applies

xtask/src/
├── tree_sitter_real_world.rs             # MODIFY: extend hard_failures with spec-driven assertions; add representative_specs to TreeSitterRealWorldRepo
├── tree_sitter_real_world_report.rs      # MODIFY: serialize representative_specs results
└── tree_sitter_certification.rs          # MODIFY (Phase 8): record capability schema migration in cert report

docs/
├── EXTRACTION_CONTRACT.md                # CREATE (Phase 7.5)
├── LANGUAGE_VERIFICATION_CHECKLIST.md    # DELETE (Phase 7.1)
├── LANGUAGE_VERIFICATION_RESULTS.md      # DELETE after harvest (Phase 7.2)
├── verification/                         # DELETE (Phase 7.3)
├── findings/                             # COMMIT staged deletions (Phase 7.4)
├── LANGUAGE_CERTIFICATION_REPORT.md      # REGENERATE at HEAD (Phase 8.1)
├── LANGUAGE_REAL_WORLD_EVIDENCE.{json,md} # REGENERATE at HEAD with --profile release (Phase 6.5)
└── TREE_SITTER_QUALITY_BAR.md            # MODIFY: refresh "Current Verdict" + "Open Gaps" (Phase 7.6)

docs/plans/
└── escalations/                          # CREATE per-escalation as needed during the run
```

**File-ownership rule:** Each task below names its modify scope. Two tasks must not list the same file under "Modify" unless this plan explicitly sequences them. The plan is structured so that Phases 1–3 finish before Phase 4 fans out per-language work; this prevents file conflicts in `tests/capability_matrix.rs` and `registry.rs`.

---

## Phase 1 — Capability Schema + Validators

**Goal:** Eliminate doc-rot and contradictory gap classification BEFORE any per-language work runs. Without this, later phases can close the same row by mutually exclusive paths.

### Task 1.1: Migrate `capabilities.json` `evidence` to typed object

**Files:**
- Modify: `fixtures/extraction/capabilities.json` (all 33+ gap rows; field rename from `evidence: String` to `evidence: Object`)
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs:44-50` (the `CapabilityGap` struct)
- Test: `crates/julie-extractors/src/tests/capability_matrix.rs` (add `capability_matrix_evidence_is_typed_object`)

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/capability_matrix.rs`:

```rust
#[test]
fn capability_matrix_evidence_is_typed_object() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    for row in &matrix.languages {
        for gap in &row.capability_gaps {
            assert!(
                !matches!(gap.evidence, EvidenceRef::DeadString(_)),
                "language {} gap {} still has bare-string evidence — migrate to typed object \
                 {{kind, value, command}}",
                row.language,
                gap.capability
            );
            match &gap.evidence {
                EvidenceRef::Test { value, command } => {
                    assert!(
                        !value.is_empty(),
                        "language {} gap {} test evidence has empty value",
                        row.language,
                        gap.capability
                    );
                    assert!(
                        command.starts_with("cargo nextest"),
                        "language {} gap {} test evidence command must start with `cargo nextest`",
                        row.language,
                        gap.capability
                    );
                }
                EvidenceRef::Fixture { value, .. } => {
                    let path = root.join(value);
                    assert!(
                        path.exists(),
                        "language {} gap {} fixture evidence path does not exist: {}",
                        row.language,
                        gap.capability,
                        path.display()
                    );
                }
                EvidenceRef::Commit { value, .. } => {
                    assert_eq!(value.len(), 40, "commit SHA must be 40 hex chars");
                    assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
                }
                EvidenceRef::DeadString(_) => unreachable!(),
            }
        }
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_evidence_is_typed_object 2>&1 | tail -20`
Expected: FAIL — `EvidenceRef::DeadString` is the only variant currently produced; the test's typed-object branches are unreachable until the migration runs.

**Step 3: Migrate the schema**

Replace the `CapabilityGap` struct's `evidence: String` field with a typed enum:

```rust
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EvidenceRef {
    Test { kind: TestKind, value: String, command: String },
    Fixture { kind: FixtureKind, value: String, command: String },
    Commit { kind: CommitKind, value: String, command: String },
    DeadString(String),  // legacy; the test rejects this variant
}

#[derive(Debug, Deserialize)] #[serde(rename_all = "lowercase")] enum TestKind { Test }
#[derive(Debug, Deserialize)] #[serde(rename_all = "lowercase")] enum FixtureKind { Fixture }
#[derive(Debug, Deserialize)] #[serde(rename_all = "lowercase")] enum CommitKind { Commit }

#[derive(Debug, Deserialize)]
struct CapabilityGap {
    capability: String,
    status: String,
    reason: String,
    required_closure: String,
    /// Resolvable artifact: a real test name, a real fixture path, or a real
    /// 40-char commit SHA. Open rows must NOT use `evidence` for planned work —
    /// use `planned_closure_task` instead.
    evidence: EvidenceRef,
    /// Required for any row whose `status` is `open`. Names the Phase task that
    /// will close the row, e.g. `"Phase 3.1"` or `"Phase 4b.html"`. Resolver
    /// tests reject open rows without this field; non-open rows reject having
    /// it (use `evidence` once the closure lands).
    #[serde(default)]
    planned_closure_task: Option<String>,
}
```

Then rewrite every `evidence` field in `fixtures/extraction/capabilities.json` from the bare string `"docs/findings/COMPILED-FINDINGS.md"` (33+ rows) to one of:

- `{"kind": "test", "value": "<test_name>", "command": "cargo nextest run -p julie-extractors --lib <test_name>"}` for rows whose closing test exists.
- `{"kind": "fixture", "value": "fixtures/extraction/<language>/<name>/expected.json", "command": "cargo nextest run -p julie-extractors --lib golden_fixtures_match_canonical_extraction"}` for rows where a golden fixture is the closing evidence.
- `{"kind": "commit", "value": "<40-char SHA>", "command": "git show <SHA>"}` for rows that point at a closing commit.

For rows that are exceptions (not closures), the evidence object's `command` should be the locking-test command (e.g., `cargo nextest run -p julie-extractors --lib regex_pending_relationships_remain_unsupported`).

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_evidence_is_typed_object 2>&1 | tail -10`
Expected: PASS.

**Step 5: Commit**

```bash
git add fixtures/extraction/capabilities.json crates/julie-extractors/src/tests/capability_matrix.rs
git commit -m "feat(extractors): migrate capabilities.json evidence field to typed object"
```

### Task 1.2: Add typed-evidence resolver test + ban "not implemented" exceptions

**Files:**
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs` (add test inventory loader, exception schema rule)
- Test: same file — add `capability_matrix_evidence_resolves` and `capability_matrix_no_not_implemented_exceptions`

**Step 1: Write the failing tests**

```rust
#[test]
fn capability_matrix_evidence_resolves() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let test_inventory = load_test_inventory(&root);
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for gap in &row.capability_gaps {
            match &gap.evidence {
                EvidenceRef::Test { value, .. } => {
                    if !test_inventory.contains(value) {
                        errors.push(format!(
                            "language {} gap {} references test `{}` that is not in the test inventory",
                            row.language, gap.capability, value
                        ));
                    }
                }
                EvidenceRef::Fixture { value, .. } => {
                    if !root.join(value).exists() {
                        errors.push(format!(
                            "language {} gap {} fixture path `{}` does not exist",
                            row.language, gap.capability, value
                        ));
                    }
                }
                EvidenceRef::Commit { value, .. } => {
                    let output = std::process::Command::new("git")
                        .args(["cat-file", "-e", value])
                        .current_dir(&root)
                        .output()
                        .expect("git available");
                    if !output.status.success() {
                        errors.push(format!(
                            "language {} gap {} commit `{}` does not resolve",
                            row.language, gap.capability, value
                        ));
                    }
                }
                EvidenceRef::DeadString(s) => errors.push(format!(
                    "language {} gap {} still has bare-string evidence: {}",
                    row.language, gap.capability, s
                )),
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn capability_matrix_no_not_implemented_exceptions() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let banned = ["not implemented", "not yet supported", "todo", "todo:", "coming soon"];
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for gap in &row.capability_gaps {
            if gap.status != "exception" {
                continue;
            }
            let lower = gap.reason.to_lowercase();
            for ban in &banned {
                if lower.contains(ban) {
                    errors.push(format!(
                        "language {} gap {} has exception reason containing `{}`: {}",
                        row.language, gap.capability, ban, gap.reason
                    ));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

fn load_test_inventory(root: &Path) -> std::collections::HashSet<String> {
    let output = std::process::Command::new("cargo")
        .args(["nextest", "list", "-p", "julie-extractors", "--message-format", "json"])
        .current_dir(root)
        .output()
        .expect("nextest list");
    // Parse nextest's JSON to extract test names. Implementation:
    // for each line that's valid JSON, deserialize into a struct with `test_id: String`,
    // and collect the bare test names (everything after the last `::`).
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut names = std::collections::HashSet::new();
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(test_name) = v.get("test").and_then(|t| t.get("name")).and_then(|n| n.as_str()) {
                if let Some(bare) = test_name.split("::").last() {
                    names.insert(bare.to_string());
                }
            }
        }
    }
    names
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_evidence_resolves capability_matrix_no_not_implemented_exceptions 2>&1 | tail -20`
Expected: BOTH FAIL initially. The resolver test fails because most evidence values still reference test names that don't exist (we haven't authored the closing tests yet); the exception test passes for now if no current row has a banned phrase, otherwise fails.

**Step 3: Update capability rows so they resolve**

For every row whose evidence is `kind: test`, ensure the test name listed exists in the nextest inventory NOW. If the closing test does not exist yet, do NOT keep the `evidence` pointing at a planned test name — that defeats the resolver gate. Instead:

- Set `status: open`.
- Set `planned_closure_task: "Phase N[.subtask]"` (e.g. `"Phase 3.1"` for SQL pending, `"Phase 4b.html"` for HTML, `"Phase 4a.ruby"` for Ruby — the value must match a task heading in this plan, case-sensitively).
- Set `evidence` to a `kind: commit` pointing at the latest commit on the current branch where the row's `required_closure` text was last edited. This makes the resolver gate green (the commit resolves) while the open status carries the work-not-yet-done signal in a separate field.

Do not use `kind: fixture` with a planned-but-not-yet-existing path. The resolver checks `path.exists()` and will fail.

For every `exception` row's `reason` field, rewrite phrases that match the banned list. Acceptable reasons: "intrinsic to language" with a one-sentence justification; "documented parser limitation in tree-sitter-<crate>"; "handled by embedded <language> extractor at <path>". Exception rows must NOT carry `planned_closure_task` — they are closed by definition.

**Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_evidence_resolves capability_matrix_no_not_implemented_exceptions 2>&1 | tail -10`
Expected: PASS for both. `_evidence_resolves` fails if any row has unresolvable evidence (test name not in inventory, fixture path missing, commit SHA does not resolve). The resolver must NOT be weakened to skip `open` rows — open status only changes which Phase task closes the row, not whether `evidence` resolves. The separate `planned_closure_task` field carries the "what closes this" signal.

**Step 5: Add the `planned_closure_task` invariant test**

```rust
#[test]
fn capability_matrix_open_rows_have_planned_closure_task() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let plan_path = root.join("docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md");
    let plan_body = std::fs::read_to_string(&plan_path).expect("plan file must exist");
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for gap in &row.capability_gaps {
            match gap.status.as_str() {
                "open" => {
                    let Some(task) = gap.planned_closure_task.as_deref() else {
                        errors.push(format!(
                            "language {} gap {} has status=open but no planned_closure_task field",
                            row.language, gap.capability
                        ));
                        continue;
                    };
                    // Plan task references must appear as a literal heading
                    // (`### Task <task>` or section anchor) in the plan body.
                    if !plan_body.contains(task) {
                        errors.push(format!(
                            "language {} gap {} planned_closure_task `{}` does not appear in the plan",
                            row.language, gap.capability, task
                        ));
                    }
                }
                "exception" | "closed" => {
                    if gap.planned_closure_task.is_some() {
                        errors.push(format!(
                            "language {} gap {} status={} but carries planned_closure_task; remove the field",
                            row.language, gap.capability, gap.status
                        ));
                    }
                }
                other => errors.push(format!(
                    "language {} gap {} has unrecognized status `{}` (expected one of: open, exception, closed)",
                    row.language, gap.capability, other
                )),
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
```

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_open_rows_have_planned_closure_task 2>&1 | tail -20`
Expected: PASS once every open row in Task 1.3 has a `planned_closure_task` pointing at a real Phase task heading.

**Step 6: Commit**

```bash
git add fixtures/extraction/capabilities.json crates/julie-extractors/src/tests/capability_matrix.rs
git commit -m "feat(extractors): validate typed evidence resolves, ban not-implemented exceptions, and require planned_closure_task on open rows"
```

### Task 1.3: Resolve gap-classification contradictions

**Files:**
- Modify: `fixtures/extraction/capabilities.json` (rows: razor, sql, html)
- Test: existing capability_matrix tests pass; no new test needed (the contradictions are resolved by JSON-only edits)

**Step 1: Edit the JSON**

**Recipe:** For an `open` row, use `planned_closure_task` to name the Phase task that will close it, and use `evidence: {kind: commit, value: <SHA>, command: "git show <SHA>"}` pointing at the commit where the `required_closure` text was last edited (this commit; the one that adds these row changes). The resolver gate stays green because every `evidence` entry resolves; the "what closes this" signal lives in the separate `planned_closure_task` field.

For an `exception` row, evidence points at the locking-test name (which IS in the inventory because we add it in Phase 4) and `planned_closure_task` is absent. If the locking test does not yet exist, defer the exception classification — keep `status: open` with `planned_closure_task` until the locking test lands.

For `razor`:
- `pending_relationships` gap → `status: exception` only AFTER the Phase 4b `razor_pending_relationships_handled_by_csharp_embed` locking test exists. Until then, keep `status: open`, `planned_closure_task: "Phase 4b.razor"`, evidence = commit. Once the test lands (Phase 4b), flip to `status: exception`, `reason: "Razor's external references are extracted by the embedded C# extractor; the Razor extractor itself emits no pending relationships by design."`, `evidence: {kind: test, value: razor_pending_relationships_handled_by_csharp_embed, command: "cargo nextest run -p julie-extractors --lib razor_pending_relationships_handled_by_csharp_embed"}`, `planned_closure_task: null`.

For `sql`:
- `pending_relationships` gap → `status: open`, `required_closure: "Move SQL out of NO_PENDING_CAPABILITIES; emit StructuredPendingRelationship for cross-file FK targets; close in Phase 3.1."`, `planned_closure_task: "Phase 3.1"`, `evidence: {kind: commit, value: <SHA>, command: "git show <SHA>"}`. Phase 3.1 swaps this to `evidence: {kind: test, value: test_sql_emits_structured_pending_for_cross_file_fk, ...}` and removes `planned_closure_task`.

For `html`:
- `pending_relationships` gap → `status: open`, `required_closure: "Emit pending relationships for external script/style src=... references; close in Phase 4b (HTML task)."`, `planned_closure_task: "Phase 4b.html"`, `evidence: {kind: commit, value: <SHA>, command: "git show <SHA>"}`. Phase 4b swaps this to the real closing test name and removes `planned_closure_task`.

**Step 2: Verify capability_matrix tests still pass**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix 2>&1 | tail -20`
Expected: PASS for `capability_matrix_evidence_resolves` (every evidence cell now references a real commit, fixture, or test), PASS for `capability_matrix_no_not_implemented_exceptions`, PASS for `capability_matrix_open_rows_have_planned_closure_task` (each of razor/sql/html either has a valid `planned_closure_task` pointing at a real plan heading or is in exception status with no such field).

**Step 3: Commit**

```bash
git add fixtures/extraction/capabilities.json
git commit -m "fix(extractors): resolve razor/sql/html gap-classification contradictions using planned_closure_task"
```

**Phase 1 boundary gate:** `cargo xtask test changed`. Append a verification ledger row.

---

## Phase 2 — Shared Extraction-Contract Regression Bar

**Goal:** Prevent shallow pending-implementation closures from passing the rubric. Tighten the structured-pending shape contract test to assert real fields, not just non-emptiness.

### Task 2.1: Add structured-pending shape contract test

**Files:**
- Create: `crates/julie-extractors/src/tests/pending_shape_contract.rs`
- Modify: `crates/julie-extractors/src/tests.rs` (add `mod pending_shape_contract;`)
- Reference (read-only): `crates/julie-extractors/src/base/relationship_resolution.rs:7-26` (`UnresolvedTarget` field shape)

**Step 1: Write the failing test**

```rust
//! Shape contract for StructuredPendingRelationship outputs across all languages.
//!
//! Every golden fixture that emits at least one structured_pending_relationships
//! entry must have entries with non-placeholder field values. This is the contract
//! that makes the pending-relationship signal useful at resolve time — without it,
//! cross-file calls collapse onto wrong targets.

use crate::registry::supported_languages;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[test]
fn structured_pending_entries_have_non_placeholder_fields() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    let mut errors = Vec::new();
    for row in &matrix.languages {
        for fixture in &row.fixtures {
            let expected = load_expected_fixture(&root, fixture);
            let pending = match expected.get("structured_pending_relationships") {
                Some(Value::Array(arr)) if !arr.is_empty() => arr,
                _ => continue, // languages with no structured pending entries are evaluated by other tests
            };
            for entry in pending {
                let target = entry.get("target")
                    .and_then(|t| t.as_object())
                    .unwrap_or_else(|| panic!(
                        "fixture {}/{} has structured_pending entry without `target` object",
                        row.language, fixture.name
                    ));
                let terminal = target.get("terminalName").and_then(|v| v.as_str()).unwrap_or("");
                assert!(
                    !terminal.is_empty(),
                    "fixture {}/{} structured_pending entry has empty target.terminalName",
                    row.language, fixture.name
                );
                let display = target.get("displayName").and_then(|v| v.as_str()).unwrap_or("");
                assert!(
                    !display.is_empty(),
                    "fixture {}/{} structured_pending entry has empty target.displayName",
                    row.language, fixture.name
                );
                let scope = entry.get("callerScopeSymbolId");
                if let Some(s) = scope {
                    assert!(s.is_string() && !s.as_str().unwrap().is_empty(),
                        "fixture {}/{} structured_pending entry has empty callerScopeSymbolId",
                        row.language, fixture.name);
                }
                // pending sub-object must have non-empty file_path and line_number > 0
                let pending_obj = entry.get("pending").and_then(|p| p.as_object()).unwrap();
                let line = pending_obj.get("line_number").and_then(|v| v.as_u64()).unwrap_or(0);
                assert!(line > 0, "fixture {}/{} pending.line_number must be > 0", row.language, fixture.name);
                let file_path = pending_obj.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                assert!(!file_path.is_empty(), "fixture {}/{} pending.file_path empty", row.language, fixture.name);
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

// re-use load_matrix and load_expected_fixture from capability_matrix.rs by making them pub(crate)
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-extractors --lib structured_pending_entries_have_non_placeholder_fields 2>&1 | tail -10`
Expected: FAIL on the FIRST language whose existing fixture has a `structured_pending_relationships` entry with placeholder fields. If no language currently emits structured pending in fixtures (likely true at HEAD per the certification report's `pending` column = 0 for most languages), the test trivially passes; in that case the test stays in place as a safety net for Phase 4 work.

**Step 3: Make load_matrix and load_expected_fixture pub(crate)**

In `crates/julie-extractors/src/tests/capability_matrix.rs`, change `fn load_matrix` and `fn load_expected_fixture` from private to `pub(crate)`. Same for `workspace_root` if not already.

**Step 4: Run test to verify it compiles + passes (or fails for a real reason)**

Run: `cargo nextest run -p julie-extractors --lib structured_pending_entries_have_non_placeholder_fields 2>&1 | tail -10`
Expected: PASS at HEAD (no fixtures emit structured pending yet); becomes a real gate as Phase 4 adds fixtures.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/tests/pending_shape_contract.rs crates/julie-extractors/src/tests/capability_matrix.rs crates/julie-extractors/src/tests.rs
git commit -m "feat(extractors): add structured pending shape contract test"
```

### Task 2.2: Add negative-case enforcement to capability_matrix (ignored until Phase 4d)

**Files:**
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs` (add `capability_matrix_negative_cases_emit_no_wrong_edges`)

**Sequencing decision (resolves Codex finding #2):** The negative-fixture requirement is a Phase 4 gate, not a Phase 2 gate. Committing it red here would fail Phase 2's `cargo xtask test changed` boundary by design. Instead we land the test as `#[ignore]` here so the shape contract is visible in the test suite, and an explicit Phase 4d task (Task 4d.ignore-flip — see Phase 4d below) removes the `#[ignore]` attribute and lets it gate. The Phase 4 per-language work creates each `fixtures/extraction/<lang>/negative/` directory; flipping the gate at the end of Phase 4d turns the populated negatives into the proof.

**Step 1: Write the failing-when-not-ignored test**

```rust
#[test]
#[ignore = "negative fixtures land per-language during Phase 4; Task 4d.ignore-flip removes this attribute"]
fn capability_matrix_negative_cases_emit_no_wrong_edges() {
    let root = workspace_root();
    let matrix = load_matrix(&root);
    for row in &matrix.languages {
        // Languages whose target_capabilities.relationships is true must have at
        // least one fixture asserting an explicit negative case: a code shape
        // that should NOT produce a relationship edge. We encode this as a fixture
        // sub-directory named `negative` (alongside `basic`).
        if !row.target_capabilities.relationships {
            continue;
        }
        let has_negative = row.fixtures.iter().any(|f| f.name.contains("negative"));
        assert!(
            has_negative,
            "language {} declares target_capabilities.relationships=true but has no `negative` fixture proving wrong edges are not emitted; add fixtures/extraction/{}/negative/",
            row.language, row.language
        );
    }
}
```

**Step 2: Verify the test compiles and is ignored (does not fail the suite)**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_negative_cases_emit_no_wrong_edges 2>&1 | tail -10`
Expected: the test is reported as `IGNORED`, not `FAIL`. This keeps the Phase 2 boundary gate green.

**Step 3: Confirm the de-ignore task exists in Phase 4d**

Verify Task 4d.ignore-flip is present in the Phase 4d section below. That task is where the failing-then-passing TDD cycle for this test actually happens. If Task 4d.ignore-flip is missing, the lead reports a plan mismatch instead of removing the `#[ignore]` attribute locally.

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/tests/capability_matrix.rs
git commit -m "feat(extractors): require negative-case fixtures for relationship-claiming languages (ignored until Phase 4d)"
```

**Phase 2 boundary gate:** `cargo xtask test changed`. The ignored test does not block this gate. Ledger row.

---

## Phase 3 — Implementation Shape (SQL pending, JSON $ref, TOML)

**Goal:** Define the pending-relationship and relationship-edge shapes that Phase 4 fixtures will mirror. Doing this first prevents per-language fixtures from calcifying around an inconsistent shape.

### Task 3.1: SQL — emit structured pending for cross-file FK targets

**Files:**
- Modify: `crates/julie-extractors/src/sql/relationships.rs` (FK target resolution)
- Modify: `crates/julie-extractors/src/sql/schema_relationships.rs` (cross-file FK detection)
- Modify: `crates/julie-extractors/src/registry.rs` (move SQL out of `define_no_pending_extractors`; SQL graduates to `define_structured_full_language_extractors` or a custom extract_sql function similar to extract_java)
- Modify: `crates/julie-extractors/src/sql/mod.rs` (add `add_structured_pending_relationship` and `get_structured_pending_relationships` per the pattern in `csharp/mod.rs:117` and `scala/mod.rs:46`)
- Test: `crates/julie-extractors/src/tests/sql/relationships.rs` (add `test_sql_emits_structured_pending_for_cross_file_fk`)
- Modify: `fixtures/extraction/sql/basic/source.sql` and `expected.json` (cross-file FK reference)

**Step 1: Write the failing test**

```rust
#[test]
fn test_sql_emits_structured_pending_for_cross_file_fk() {
    // Source contains: CREATE TABLE orders (user_id INT REFERENCES other_schema.users(id));
    // The other_schema.users target is not in the same file → must produce a
    // StructuredPendingRelationship with target.terminalName="users",
    // target.namespacePath=["other_schema"], target.receiver=None,
    // RelationshipKind::References, file_path/line_number set, callerScopeSymbolId
    // pointing at the orders table symbol.
    let source = include_str!("../../../../../fixtures/extraction/sql/cross_file/source.sql");
    let workspace_root = workspace_root_static();
    let result = extract_canonical("source.sql", source, workspace_root).unwrap();
    let pendings = &result.structured_pending_relationships;
    let users_ref = pendings.iter().find(|p| p.target.terminal_name == "users")
        .expect("expected structured pending for `users` cross-schema reference");
    assert_eq!(users_ref.target.namespace_path, vec!["other_schema".to_string()]);
    assert_eq!(users_ref.pending.kind, RelationshipKind::References);
    assert!(users_ref.caller_scope_symbol_id.is_some(),
        "callerScopeSymbolId must point at the `orders` table symbol");
}
```

Add fixture file `fixtures/extraction/sql/cross_file/source.sql`:

```sql
CREATE TABLE orders (
    id INT PRIMARY KEY,
    user_id INT REFERENCES other_schema.users(id)
);
```

Add fixture row to `fixtures/extraction/capabilities.json` for SQL: `{"name": "cross_file", "source": "fixtures/extraction/sql/cross_file/source.sql", "expected": "fixtures/extraction/sql/cross_file/expected.json"}`.

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-extractors --lib test_sql_emits_structured_pending_for_cross_file_fk 2>&1 | tail -10`
Expected: FAIL — SQL currently uses `define_no_pending_extractors` so `structured_pending_relationships` is empty.

**Step 3: Migrate SQL out of no_pending macro**

In `registry.rs`, remove SQL's entry from the `define_no_pending_extractors!` invocation. Add a hand-written `extract_sql` function modeled after `extract_java` (lines 235-262), pulling pending and structured-pending arrays from the SQL extractor.

In `crates/julie-extractors/src/sql/mod.rs`, add the `add_structured_pending_relationship` / `get_structured_pending_relationships` methods following the `csharp/mod.rs:117` pattern.

In `crates/julie-extractors/src/sql/relationships.rs` (or `schema_relationships.rs`), when extracting `REFERENCES <schema>.<table>(<col>)` clauses, detect when `<schema>` is non-empty and the target table is not present in the local symbol map; in that case, build an `UnresolvedTarget { display_name: "other_schema.users", terminal_name: "users", namespace_path: vec!["other_schema"], receiver: None, import_context: None }` and call `base.create_pending_relationship(..., RelationshipKind::References, &node, Some(orders_symbol_id), None)`, then `self.add_structured_pending_relationship(pending)`.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-extractors --lib test_sql_emits_structured_pending_for_cross_file_fk 2>&1 | tail -10`
Expected: PASS.

**Step 5: Update the golden fixture**

Run the canonical extraction to produce the expected.json:

```bash
cargo run -p julie-extractors --example regen_golden -- fixtures/extraction/sql/cross_file/source.sql > fixtures/extraction/sql/cross_file/expected.json
```

(If `regen_golden` example does not exist yet, defer: write the expected.json manually with the exact structured-pending shape, then add `regen_golden` as a small example in Phase 5.6.)

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/sql/ crates/julie-extractors/src/registry.rs fixtures/extraction/sql/cross_file/ fixtures/extraction/capabilities.json
git commit -m "feat(sql): emit structured pending relationships for cross-file FK targets"
```

### Task 3.2: JSON — emit relationships for `$ref`

**Files:**
- Modify: `crates/julie-extractors/src/json/mod.rs` (relationship extraction for `$ref`)
- Modify: `crates/julie-extractors/src/registry.rs` (move JSON out of `define_data_only_extractors`; new hand-written extract_json mirroring extract_java but with relationships+pending only, no types)
- Test: `crates/julie-extractors/src/tests/json/relationships.rs` (create directory + module + test)
- Modify: `crates/julie-extractors/src/tests.rs` (add `mod json { mod relationships; }`)
- Create: `fixtures/extraction/json/refs/source.json` + `expected.json`

**Step 1: Write the failing test**

```rust
//! JSON Schema $ref relationship extraction tests.

use julie_extractors::extract_canonical;
use std::path::Path;

#[test]
fn test_json_emits_relationship_for_local_ref() {
    let source = r#"{
        "$defs": {
            "Address": { "type": "object" }
        },
        "properties": {
            "billing": { "$ref": "#/$defs/Address" }
        }
    }"#;
    let result = extract_canonical("schema.json", source, workspace_root_static()).unwrap();
    let billing_to_address = result.relationships.iter()
        .find(|r| r.from_symbol_id.contains("billing") && r.to_symbol_id.contains("Address"))
        .expect("expected billing → Address relationship from local $ref");
    assert!(matches!(billing_to_address.kind, RelationshipKind::References));
}

#[test]
fn test_json_emits_structured_pending_for_external_ref() {
    let source = r#"{
        "properties": {
            "billing": { "$ref": "external.json#/$defs/Address" }
        }
    }"#;
    let result = extract_canonical("schema.json", source, workspace_root_static()).unwrap();
    let pending = result.structured_pending_relationships.iter()
        .find(|p| p.target.terminal_name == "Address")
        .expect("expected structured pending for external $ref");
    assert!(pending.target.import_context.as_deref() == Some("external.json"));
}

#[test]
fn test_json_no_relationship_for_malformed_ref() {
    // Negative case: $ref pointing at a non-existent local path produces no edge.
    let source = r#"{
        "properties": {
            "broken": { "$ref": "#/nonexistent/Path" }
        }
    }"#;
    let result = extract_canonical("schema.json", source, workspace_root_static()).unwrap();
    assert!(result.relationships.iter().all(|r| !r.to_symbol_id.contains("nonexistent")),
        "no concrete relationship should be emitted for malformed $ref; structured pending is the right place");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p julie-extractors --lib test_json_emits 2>&1 | tail -20`
Expected: FAIL — JSON currently uses `define_data_only_extractors` (no relationships, no pending).

**Step 3: Implement `$ref` extraction**

Move JSON out of `define_data_only_extractors!`. Add a hand-written `extract_json` function in `registry.rs` that calls relationship + identifier extraction (no types).

In `crates/julie-extractors/src/json/mod.rs`, add a `relationships.rs` module that walks the AST for `pair` nodes whose key is `"$ref"`, parses the value:
- If the value starts with `#/`, it's a local pointer. Resolve to the symbol whose `$defs/<name>` location matches; emit `Relationship { kind: RelationshipKind::References, from_symbol_id: <containing object symbol>, to_symbol_id: <resolved Address symbol> }`.
- If the value is `<file>.json#/...`, the target is external; emit `StructuredPendingRelationship` with `target.import_context = Some("<file>.json".to_string())`, `target.terminal_name = <last path segment>`, `target.namespace_path = <middle path segments>`.
- If the local pointer doesn't resolve (negative case), emit nothing.

**Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p julie-extractors --lib test_json_emits test_json_no 2>&1 | tail -10`
Expected: PASS.

**Step 5: Add fixture + capability row**

Add `fixtures/extraction/json/refs/source.json` and `expected.json`. Update `capabilities.json` JSON row: `target_capabilities.relationships: true`, add fixture entry, change relationships gap to closed with `kind: test, value: test_json_emits_relationship_for_local_ref`.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/json/ crates/julie-extractors/src/registry.rs crates/julie-extractors/src/tests/ fixtures/extraction/json/ fixtures/extraction/capabilities.json
git commit -m "feat(json): emit relationships for JSON Schema \$ref"
```

### Task 3.3: TOML — emit relationships for Cargo deps + pyproject tool tables

**Files:**
- Modify: `crates/julie-extractors/src/toml/mod.rs` (relationship extraction)
- Modify: `crates/julie-extractors/src/registry.rs` (move TOML out of `define_data_only_extractors`)
- Test: `crates/julie-extractors/src/tests/toml/relationships.rs`
- Create: `fixtures/extraction/toml/cargo_deps/source.toml` + `expected.json`, `fixtures/extraction/toml/pyproject/source.toml` + `expected.json`

**Step 1: Write the failing tests**

```rust
//! TOML domain-relationship extraction tests.

#[test]
fn test_toml_cargo_dependencies_emit_relationships() {
    let source = r#"
[package]
name = "myapp"

[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["full"] }
    "#;
    let result = extract_canonical("Cargo.toml", source, workspace_root_static()).unwrap();
    assert!(result.relationships.iter().any(|r|
        r.to_symbol_id.contains("serde") && matches!(r.kind, RelationshipKind::Imports)),
        "expected Cargo.toml [dependencies] serde → Imports relationship");
    assert!(result.relationships.iter().any(|r| r.to_symbol_id.contains("tokio")));
}

#[test]
fn test_toml_pyproject_tool_tables_emit_relationships() {
    let source = r#"
[project]
name = "myapp"

[tool.ruff]
line-length = 88

[tool.pytest.ini_options]
asyncio_mode = "auto"
    "#;
    let result = extract_canonical("pyproject.toml", source, workspace_root_static()).unwrap();
    assert!(result.relationships.iter().any(|r|
        r.to_symbol_id.contains("ruff") && matches!(r.kind, RelationshipKind::References)),
        "expected pyproject.toml [tool.ruff] → References relationship");
    assert!(result.relationships.iter().any(|r| r.to_symbol_id.contains("pytest")));
}

#[test]
fn test_toml_arbitrary_table_emits_no_relationship() {
    // Negative case: a non-dependency, non-tool table produces no relationship.
    let source = r#"
[some.other.table]
key = "value"
    "#;
    let result = extract_canonical("config.toml", source, workspace_root_static()).unwrap();
    assert!(result.relationships.is_empty(),
        "no relationships should be emitted for arbitrary tables, got: {:?}", result.relationships);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p julie-extractors --lib test_toml 2>&1 | tail -20`
Expected: FAIL.

**Step 3: Implement TOML domain relationships**

Move TOML out of `define_data_only_extractors!`. Add hand-written `extract_toml`. In `crates/julie-extractors/src/toml/mod.rs`, add a `relationships.rs` module that walks `table` and `dotted_key` nodes:
- For tables matching `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, `[target.<triple>.dependencies]`: emit `Relationship { kind: RelationshipKind::Imports, from: <Cargo.toml file symbol>, to: <key as terminal_name>, ... }` per child key.
- For dotted keys `tool.<x>.<...>` in pyproject.toml: emit `Relationship { kind: RelationshipKind::References, from: <project symbol>, to: <"x" terminal_name>, ... }` once per unique top-level tool name.
- All other tables: no relationship.

File-name detection: use the file_path argument; only Cargo.toml-named files trigger Cargo dep extraction; only pyproject.toml-named files trigger tool table extraction. (For files named other than these, the table dispatcher above falls through to the negative case.)

**Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p julie-extractors --lib test_toml 2>&1 | tail -10`
Expected: PASS.

**Step 5: Add fixtures + capability rows**

Two new fixture directories. Update TOML row in `capabilities.json`: `target_capabilities.relationships: true`, two fixture entries, relationship gap closed with `kind: test, value: test_toml_cargo_dependencies_emit_relationships`.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/toml/ crates/julie-extractors/src/registry.rs crates/julie-extractors/src/tests/ fixtures/extraction/toml/ fixtures/extraction/capabilities.json
git commit -m "feat(toml): emit relationships for cargo deps and pyproject tool tables"
```

**Phase 3 boundary gate:** `cargo xtask test changed`, then `cargo xtask test bucket extractors`. Ledger rows for each task.

---

## Phase 4 — Per-Tier Language Work

**Goal:** Close the structured-pending and relationship gaps for every language whose extractor currently doesn't emit them or whose fixtures don't prove them. Tier-ordered: general programming first (largest set), then component/template, then query/declarative, then doc/data.

The work pattern depends on whether the language emits structured pending (most languages) or is an intrinsic-N/A row that only needs a locking test (e.g., razor's pending is handled by the embedded C# extractor; regex has no concept of cross-file references). Each table below tags every language with a **Recipe** column (`A` or `B`).

### Per-Language TDD Cycle — Recipe A (implement structured pending) — DEFAULT

Use Recipe A when the language has a sensible cross-file or unresolved reference shape (imports, requires, FK targets, $ref, anchor refs, etc.). All Phase 4a rows use Recipe A. Most Phase 4b/4c/4d rows use Recipe A unless tagged Recipe B.

**Files (substitute `<lang>`):**
- Modify: `crates/julie-extractors/src/<lang>/mod.rs` — if the language uses `add_structured_pending_relationship` (csharp/java pattern), ensure the cross-file reference detection emits structured pending. If the language is currently on `define_no_pending_extractors!`, migrate to `define_structured_full_language_extractors!` or a hand-written extract function.
- Modify: `crates/julie-extractors/src/<lang>/relationships.rs` — emit structured pending entries with full UnresolvedTarget fields for cross-file or unresolved references.
- Modify: `crates/julie-extractors/src/registry.rs` — adjust macro-membership.
- Test: `crates/julie-extractors/src/tests/<lang>/<existing-test-file>.rs` (or create `pending.rs` if none) — add `test_<lang>_emits_structured_pending_for_<scenario>` AND `test_<lang>_negative_<scenario>_emits_no_wrong_edge`.
- Modify: `fixtures/extraction/<lang>/basic/source.<ext>` — add a cross-file/unresolved reference of the per-language scenario shape.
- Modify: `fixtures/extraction/<lang>/basic/expected.json` — assert the structured_pending_relationships shape.
- Create: `fixtures/extraction/<lang>/negative/source.<ext>` + `expected.json` — encode the per-language negative scenario.
- Modify: `fixtures/extraction/capabilities.json` — add `negative` fixture entry, close pending gap with `kind: test, value: test_<lang>_emits_structured_pending_for_<scenario>, command: cargo nextest run -p julie-extractors --lib test_<lang>_emits_structured_pending_for_<scenario>`. Remove `planned_closure_task` from this row (status flips from `open` to `closed`).

**Steps:**
1. Write the failing positive test asserting the structured-pending shape (target.terminal_name, target.namespace_path/import_context/receiver as appropriate, callerScopeSymbolId, line_number > 0).
2. Run: `cargo nextest run -p julie-extractors --lib test_<lang>_emits_structured_pending_for_<scenario> 2>&1 | tail -10`. Expected: FAIL.
3. Modify the extractor or registry to emit the structured pending.
4. Run the same test. Expected: PASS.
5. Write the failing negative test asserting no wrong edge is emitted for the negative scenario.
6. Run the negative test. Expected: FAIL if the implementation is over-eager.
7. Tighten the implementation if needed.
8. Run the negative test. Expected: PASS.
9. Add fixtures (positive + negative) and update `expected.json` + `capabilities.json`.
10. Run `cargo nextest run -p julie-extractors --lib golden_fixtures_match_canonical_extraction`. Expected: PASS.
11. Commit:
    ```bash
    git add crates/julie-extractors/src/<lang>/ crates/julie-extractors/src/registry.rs crates/julie-extractors/src/tests/<lang>/ fixtures/extraction/<lang>/ fixtures/extraction/capabilities.json
    git commit -m "feat(<lang>): emit structured pending and prove negative cases"
    ```

### Per-Language TDD Cycle — Recipe B (lock exception / no-wrong-edge)

Use Recipe B for intrinsic-N/A rows: languages whose external references are either handled by an embedded extractor (razor → C# embed) or whose grammar has no concept of cross-file references (regex). Recipe B emits NO structured pending entries; the proof is a locking test that asserts the extractor produces zero pending and a negative fixture proving no wrong edges escape.

**Files (substitute `<lang>`):**
- Read-only: `crates/julie-extractors/src/<lang>/mod.rs` (no extractor changes; verify it remains on `define_no_pending_extractors!` or equivalent)
- Test: `crates/julie-extractors/src/tests/<lang>/pending.rs` — add `<lang>_pending_relationships_remain_unsupported` (or `<lang>_pending_relationships_handled_by_<embed>` for embed-delegated rows) asserting `structured_pending_relationships.is_empty()` and that any expected pending entries appear in the embed extractor's output instead (for razor).
- Create: `fixtures/extraction/<lang>/negative/source.<ext>` + `expected.json` — encode the wrong-edge scenario; expected.json asserts `relationships: []` and `structured_pending_relationships: []`.
- Modify: `fixtures/extraction/capabilities.json` — flip the row from `status: open, planned_closure_task: "Phase 4..."` to `status: exception` with `evidence: {kind: test, value: <lang>_pending_relationships_remain_unsupported, command: "cargo nextest run -p julie-extractors --lib <lang>_pending_relationships_remain_unsupported"}` and remove `planned_closure_task`. Reason must avoid the banned phrases from Task 1.2 — use "intrinsic to language" or "handled by embedded <embed> extractor at <path>".

**Steps:**
1. Write the failing locking test asserting zero pending output (for embed-delegated rows, also assert the embed extractor produced the expected entries).
2. Run: `cargo nextest run -p julie-extractors --lib <lang>_pending_relationships_remain_unsupported 2>&1 | tail -10`. Expected: PASS at HEAD (the extractor already emits zero pending) — this is a locking test, so PASS-at-HEAD is the correct initial state. If the test fails, the extractor is leaking edges and Recipe B is the wrong choice; switch to Recipe A.
3. Add the negative fixture and update `expected.json` with `relationships: []` and `structured_pending_relationships: []`.
4. Run `cargo nextest run -p julie-extractors --lib golden_fixtures_match_canonical_extraction`. Expected: PASS.
5. Flip `capabilities.json` row to `status: exception`, evidence pointing at the new locking test, `planned_closure_task` removed.
6. Run all three resolver-related tests: `cargo nextest run -p julie-extractors --lib capability_matrix_evidence_resolves capability_matrix_no_not_implemented_exceptions capability_matrix_open_rows_have_planned_closure_task 2>&1 | tail -20`. Expected: PASS.
7. Commit:
    ```bash
    git add crates/julie-extractors/src/tests/<lang>/ fixtures/extraction/<lang>/ fixtures/extraction/capabilities.json
    git commit -m "test(<lang>): lock intrinsic-N/A pending classification with negative fixture"
    ```

### Recipe-Selection Reference

| Row | Recipe | Why |
|---|---|---|
| Phase 4a all 24 langs | A | General-programming tier: every language has imports/calls/cross-file refs to encode |
| Phase 4b html | A | `<script src=...>` is a real cross-file reference |
| Phase 4b vue | A | `<script setup>` imports are real cross-file refs |
| Phase 4b razor | B | External refs handled by embedded C# extractor; razor itself must emit zero pending |
| Phase 4b qml | A | `import OtherModule 1.0` is a real cross-module ref |
| Phase 4c sql | A | Already implemented in Phase 3.1 |
| Phase 4c css | A | Cross-file `var(--brand)` is a real ref |
| Phase 4c regex | B | Regex has no concept of cross-file references; backref scope is intra-pattern |
| Phase 4d markdown | A | `[text](./other.md#anchor)` is a real cross-file ref |
| Phase 4d json | A | Already implemented in Phase 3.2 |
| Phase 4d toml | A | Already implemented in Phase 3.3 |
| Phase 4d yaml | A | Cross-file anchor refs are real (per yaml-include conventions in the wild) |

### Phase 4a — General Programming Tier (24 languages)

| Lang | Positive scenario | Negative scenario | Per-language note |
|---|---|---|---|
| rust | `use crate::other_module::Function;` cross-module call to `Function()` → pending with `terminal_name="Function"`, `namespace_path=["crate","other_module"]` | `let Function = 1; Function;` (shadowed local) → no edge | already on `define_structured_full_language_extractors`; verify it emits, fix if not |
| c | `extern int other_func(void);` then `other_func();` in main → pending `terminal_name="other_func"` | static helper called from same file → resolved relationship, NOT pending | already structured-full; verify |
| cpp | `#include "other.h"` then `other_ns::do_thing();` → pending `terminal_name="do_thing"`, `namespace_path=["other_ns"]` | template parameter type `T` referenced inside body → no edge for T | check templates aren't producing wrong edges |
| go | `import "example/other"` then `other.DoIt()` → pending `terminal_name="DoIt"`, `import_context=Some("example/other")` | unexported `dofoo()` called same-file → resolved relationship | already structured-full |
| zig | `const m = @import("other.zig"); m.func();` → pending `terminal_name="func"`, `import_context="other.zig"` | builtin `@import` itself → no relationship target | already structured-full |
| typescript | `import { Foo } from './other'; new Foo();` → pending `terminal_name="Foo"`, `import_context="./other"` | type-only import `import type { T }` referenced as a type in same file → resolved type usage, NOT call pending | currently hand-written; verify |
| tsx | same as typescript with JSX `<Foo/>` element → pending; bare `<div/>` → no edge | unclosed JSX `<Component>` at malformed position → no edge | tsx fixture already clean |
| javascript | `const { foo } = require('./other'); foo();` → pending `terminal_name="foo"`, `import_context="./other"` | undeclared global `console.log` → no edge | currently hand-written |
| jsx | mirrors javascript with JSX | same | already clean |
| python | `from other import bar` then `bar()` → pending `terminal_name="bar"`, `import_context="other"` | local `bar = 1; bar` → no edge | currently uses `extract_python` — verify it emits |
| java | `import com.example.Other; new Other();` → pending `terminal_name="Other"`, `namespace_path=["com","example"]` | inner class `Inner` referenced inside outer → resolved | hand-written, has add_structured_pending; verify |
| csharp | `using OtherNs; new OtherClass();` → pending `terminal_name="OtherClass"`, `import_context="OtherNs"` | nameof(LocalVar) → no edge | hand-written |
| vbnet | `Imports OtherNs` then `Dim x As New OtherClass()` → pending | local Dim referenced same-scope → resolved | already structured-full |
| php | `use App\Other; new Other();` → pending `terminal_name="Other"`, `namespace_path=["App"]` | `$this->method()` → resolved if same-class | hand-written |
| ruby | `require "other"; OtherModule::do_thing` → pending `terminal_name="do_thing"`, `namespace_path=["OtherModule"]`, `import_context="other"` | `self.method` calls within same class → resolved | currently uses no-pending macro? verify and migrate |
| swift | `import Other; Other.thing()` → pending | property accessed via self → resolved | hand-written |
| kotlin | `import other.Thing; Thing()` → pending | sealed-class subclass referenced in same file → resolved | hand-written |
| scala | `import other.Thing; Thing.apply()` → pending; given/extension methods cross-file → pending | implicit conversions same-file → resolved | hand-written |
| dart | `import 'other.dart'; Other()` → pending `import_context="other.dart"` | factory constructor same-class → resolved | already structured-full |
| elixir | `alias Phoenix.Router; Router.match` → pending `terminal_name="match"`, `namespace_path=["Phoenix","Router"]` | private fn called same-module → resolved | already on `define_full_language_extractors` (no structured); migrate to structured-full |
| bash | `source ./other.sh; other_fn args` → pending | local function defined and called same-script → resolved | currently no-pending; migrate |
| powershell | `Import-Module Other; Invoke-Other -arg` → pending | `$LocalVar` referenced same-scope → resolved | currently no-pending; migrate |
| gdscript | `extends "res://other.gd"` then `other_method()` → pending | local method same-class → resolved | already structured-full |
| lua | `local other = require("other"); other.fn()` → pending `import_context="other"` | local function same-file → resolved | currently no-pending; migrate |
| r | `library(other); other::do_thing()` → pending `namespace_path=["other"]` | local closure same-file → resolved | currently no-pending; migrate |

**Note on Elixir migration:** Elixir is the only language on `define_full_language_extractors!` (the variant that supports pending but not structured). To meet the rubric's structured-pending shape contract, Elixir migrates to `define_structured_full_language_extractors!` (or a hand-written function) so its `get_structured_pending_relationships()` is wired. This may require adding `add_structured_pending_relationship` to `elixir/mod.rs` per the csharp/java pattern.

**Phase 4a boundary gate:** `cargo xtask test changed` then `cargo xtask test dev`. Ledger row.

### Phase 4b — Component/Template Tier (4 languages)

| Lang | Positive scenario | Negative scenario | Note |
|---|---|---|---|
| html | `<script src="./app.js"></script>` → pending `terminal_name="./app.js"`, `target.import_context=Some("script-src")` | inline `<script>const x = 1;</script>` with no src → no pending for the inline body's calls (those are emitted by the embedded JS extractor) | currently no-pending; migrate |
| vue | `<script setup>import { foo } from './other'; foo();</script>` cross-file → pending | template-only ref to a defined `<template>`-scope variable → resolved within file | already has `extract_structured_pending_relationships`; verify it covers script-setup imports |
| razor | `@using OtherNs; <OtherComponent/>` → pending **emitted by the embedded C# extractor**, not by the Razor extractor itself | bare `<div>` element → no pending | Razor task: add a `razor_pending_relationships_handled_by_csharp_embed` locking test that asserts the Razor extractor emits zero pending and the embedded C# emits the expected ones |
| qml | `import OtherModule 1.0; OtherType { ... }` → pending `terminal_name="OtherType"`, `namespace_path=["OtherModule"]` | local id-referenced item same-component → resolved | currently uses no-pending? verify and migrate |

**Phase 4b boundary gate:** `cargo xtask test changed`. Ledger row.

### Phase 4c — Query/Declarative Tier (3 languages)

| Lang | Positive scenario | Negative scenario | Note |
|---|---|---|---|
| sql | already closed in §3.1; ensure negative fixture exists | malformed FK target → no edge, structured pending instead | from §3.1 |
| css | `.box { color: var(--brand); }` referencing `--brand` defined in another file → pending `terminal_name="--brand"` | local `var(--local)` referencing `--local` defined same-file → resolved | currently uses `define_relationship_data_extractors`; migrate to allow pending |
| regex | named backreference `(?P<name>...)\g<name>` → resolved (same-pattern); `\g<undefined>` → no edge | unicode property `\p{Letter}` standalone → no edge | currently uses no-pending macro; verify the existing capture/backreference logic, then add negative fixture |

**Phase 4c boundary gate:** `cargo xtask test changed`. Ledger row.

### Phase 4d — Documentation/Data Tier (4 languages)

| Lang | Positive scenario | Negative scenario | Note |
|---|---|---|---|
| markdown | `[text](./other.md#anchor)` link → pending `terminal_name="anchor"`, `import_context="./other.md"` | inline code span containing parens → no edge | currently no-pending; migrate |
| json | already closed in §3.2 | from §3.2 | done |
| toml | already closed in §3.3 | from §3.3 | done |
| yaml | YAML anchor reference `*name` to `&name` in another file → pending; same-file `*name` → resolved | bare scalar that happens to start with `*` inside a quoted string → no edge | currently uses `define_relationship_data_extractors`; verify or migrate |

### Task 4d.ignore-flip: De-ignore `capability_matrix_negative_cases_emit_no_wrong_edges`

**Files:**
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs` — remove the `#[ignore = "..."]` attribute on `capability_matrix_negative_cases_emit_no_wrong_edges` added in Task 2.2

**Sequencing precondition:** Every language whose row in `capabilities.json` has `target_capabilities.relationships = true` must already have a `fixtures/extraction/<lang>/negative/` entry committed by an earlier Phase 4 task. Verify by running the test before removing the attribute.

**Step 1: Confirm all negative fixtures exist**

Run (the test is still ignored, so this is a manual inventory): `ls fixtures/extraction/*/negative/expected.json | sort`. Cross-check against `capabilities.json` rows where `target_capabilities.relationships = true`. Every such row must have a corresponding `negative` directory.

**Step 2: Remove the `#[ignore]` attribute**

Delete the `#[ignore = "..."]` line above `fn capability_matrix_negative_cases_emit_no_wrong_edges`.

**Step 3: Run the test to verify it passes**

Run: `cargo nextest run -p julie-extractors --lib capability_matrix_negative_cases_emit_no_wrong_edges 2>&1 | tail -10`
Expected: PASS. If FAIL, identify the missing-negative language(s) and route back to the relevant Phase 4 sub-task to add the fixture — do not re-ignore.

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/tests/capability_matrix.rs
git commit -m "test(extractors): activate negative-case gate now that all Phase 4 fixtures exist"
```

**Phase 4d boundary gate:** `cargo xtask test changed` and `cargo xtask test dev`. Ledger rows.

---

## Phase 5 — Pillar 3 Hardening

**Goal:** Make `julie-extractors` consumable as a stable Rust crate dependency.

### Task 5.1: Inventory references to `fixtures/extraction/capabilities.json` and confirm path stability

**Decision (resolves Codex finding #5):** `capabilities.json` stays at `fixtures/extraction/capabilities.json`. It is NOT moved into the crate. The design lists it as the single source of truth, 44 in-repo refs already point there, and the crate consumes it via `include_str!("../../../fixtures/extraction/capabilities.json")` from `src/capability_snapshot.rs`. Phase 5.2 sets up the include_str; this task only verifies and inventories.

**Files (this task makes no path-modifying edits):**
- Read-only inventory: every consumer of the path. Use `mcp__plugin_julie_julie__fast_search(query="capabilities.json", search_target="files")` to enumerate.

**Step 1: Confirm the file lives at the canonical path**

Run: `test -f fixtures/extraction/capabilities.json && echo OK`
Expected: `OK`.

**Step 2: Enumerate all consumers**

Use `mcp__plugin_julie_julie__fast_search(query="capabilities.json", search_target="files")`. Record the count and a short list in a Step-3 note. The expectation is roughly 44 references across xtask, tests, docs, and the new `src/capability_snapshot.rs` (which is created in Phase 5.2).

**Step 3: Add a path-stability regression test**

Add to `crates/julie-extractors/src/tests/capability_matrix.rs`:

```rust
#[test]
fn capabilities_json_canonical_path_exists() {
    let path = workspace_root().join("fixtures/extraction/capabilities.json");
    assert!(
        path.exists(),
        "capabilities.json must remain at fixtures/extraction/capabilities.json — \
         this is the single source of truth and the include_str! path in \
         crates/julie-extractors/src/capability_snapshot.rs targets it. \
         Moving this file requires updating both that include_str! and ~44 other refs."
    );
}
```

**Step 4: Run + commit**

Run: `cargo nextest run -p julie-extractors --lib capabilities_json_canonical_path_exists 2>&1 | tail -10`
Expected: PASS.

```bash
git add crates/julie-extractors/src/tests/capability_matrix.rs
git commit -m "test(extractors): lock canonical path of fixtures/extraction/capabilities.json"
```

### Task 5.2: Add `capability_snapshot()` public function

**Files:**
- Create: `crates/julie-extractors/src/capability_snapshot.rs`
- Modify: `crates/julie-extractors/src/lib.rs` — add `pub mod capability_snapshot; pub use capability_snapshot::{CapabilitySnapshot, CapabilityRow};`
- Test: `crates/julie-extractors/src/tests/capability_snapshot_test.rs` — add `test_capability_snapshot_loads_all_languages`, `test_capability_snapshot_get_returns_none_for_unknown`, `test_capability_snapshot_uses_oncelock_not_build_script`

**Step 1: Write the failing tests**

```rust
use julie_extractors::{capability_snapshot, CapabilityRow};

#[test]
fn test_capability_snapshot_loads_all_languages() {
    let snap = capability_snapshot();
    assert_eq!(snap.languages().count(), 36);
    assert!(snap.get("rust").is_some());
    assert!(snap.get("vbnet").is_some());
}

#[test]
fn test_capability_snapshot_get_returns_none_for_unknown() {
    assert!(capability_snapshot().get("klingon").is_none());
}

#[test]
fn test_capability_snapshot_uses_oncelock_not_build_script() {
    // Verifies the data is loaded via include_str! at compile time, not via a build.rs.
    // This is checked by the absence of a build script: cargo metadata should show
    // build = false for julie-extractors.
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "-p", "julie-extractors"])
        .output()
        .expect("cargo metadata");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let pkg = json["packages"].as_array().unwrap()
        .iter().find(|p| p["name"] == "julie-extractors").unwrap();
    assert!(pkg["targets"].as_array().unwrap()
        .iter().all(|t| t["kind"] != serde_json::json!(["custom-build"])),
        "julie-extractors must not have a build script; use include_str! instead");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p julie-extractors --lib test_capability_snapshot 2>&1 | tail -20`
Expected: FAIL — `capability_snapshot` doesn't exist yet.

**Step 3: Implement `capability_snapshot.rs`**

```rust
//! Stable, downstream-readable capability declaration.
//!
//! Loads from `fixtures/extraction/capabilities.json` (single source of truth)
//! via `include_str!` at compile time. No build script. Path-dep and git-dep
//! consumers work because cargo serves the whole repo at its on-disk location.
//! This crate is intentionally NOT `cargo publish`-able to crates.io — see
//! `docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md` Architecture
//! Quality section for the Pillar-3 scope decision.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const CAPABILITIES_JSON: &str = include_str!("../../../fixtures/extraction/capabilities.json");

#[derive(Debug, Deserialize)]
pub struct CapabilitySnapshot {
    languages: Vec<CapabilityRow>,
    #[serde(skip)]
    by_name: HashMap<String, usize>,
}

#[derive(Debug, Deserialize)]
pub struct CapabilityRow {
    pub language: String,
    pub parser_crate: String,
    pub extensions: Vec<String>,
    pub dependency_status: String,
    pub target_capabilities: CapabilityFlags,
    pub capabilities: CapabilityFlags,
    pub fixtures: Vec<FixtureRef>,
    #[serde(default)]
    pub capability_gaps: Vec<CapabilityGap>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct CapabilityFlags {
    pub symbols: bool,
    pub relationships: bool,
    pub pending_relationships: bool,
    pub identifiers: bool,
    pub types: bool,
}

#[derive(Debug, Deserialize)]
pub struct FixtureRef {
    pub name: String,
    pub source: String,
    pub expected: String,
}

#[derive(Debug, Deserialize)]
pub struct CapabilityGap {
    pub capability: String,
    pub status: String,
    pub reason: String,
    pub required_closure: String,
    pub evidence: serde_json::Value,
}

impl CapabilitySnapshot {
    pub fn languages(&self) -> impl Iterator<Item = &CapabilityRow> {
        self.languages.iter()
    }

    pub fn get(&self, language: &str) -> Option<&CapabilityRow> {
        self.by_name.get(language).map(|&i| &self.languages[i])
    }
}

pub fn capability_snapshot() -> &'static CapabilitySnapshot {
    static SNAPSHOT: OnceLock<CapabilitySnapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        let mut snap: CapabilitySnapshot = serde_json::from_str(CAPABILITIES_JSON)
            .expect("capabilities.json must be valid JSON matching the snapshot schema");
        snap.by_name = snap.languages.iter().enumerate()
            .map(|(i, row)| (row.language.clone(), i))
            .collect();
        snap
    })
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p julie-extractors --lib test_capability_snapshot 2>&1 | tail -10`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/capability_snapshot.rs crates/julie-extractors/src/lib.rs crates/julie-extractors/src/tests/capability_snapshot_test.rs crates/julie-extractors/src/tests.rs
git commit -m "feat(extractors): add capability_snapshot() public API for downstream consumers"
```

### Task 5.3: Add `EXTRACTION_CONTRACT_VERSION` constant + composition test

**Files:**
- Modify: `crates/julie-extractors/src/lib.rs` — add `pub const EXTRACTION_CONTRACT_VERSION: &str = "2026-05-10.tree-sitter-best-in-class-v1";`
- Modify: `src/tools/workspace/indexing/engine_version.rs:6` — the existing constant is named `SEMANTIC_INDEX_ENGINE_VERSION` (not `SEMANTIC_ENGINE_VERSION`) and is currently `pub(crate)`. **Both must change in this task:** (a) bump visibility to `pub` so the in-repo test in `src/tests/core/` can import it; (b) recompose its value to include `julie_extractors::EXTRACTION_CONTRACT_VERSION`. Keep the name `SEMANTIC_INDEX_ENGINE_VERSION` — do not rename. Update every downstream call-site that reads the const (write rule: don't change call-sites yet — `pub(crate)` → `pub` is a strict widening, so callers compile unchanged).
- Test: `src/tests/core/engine_version.rs` (or wherever existing tests live) — add `test_semantic_index_engine_version_includes_extraction_contract`

**Step 1: Write the failing test**

```rust
use julie::tools::workspace::indexing::engine_version::SEMANTIC_INDEX_ENGINE_VERSION;

#[test]
fn test_semantic_index_engine_version_includes_extraction_contract() {
    assert!(SEMANTIC_INDEX_ENGINE_VERSION.contains(julie_extractors::EXTRACTION_CONTRACT_VERSION),
        "SEMANTIC_INDEX_ENGINE_VERSION ({}) must include EXTRACTION_CONTRACT_VERSION ({}) for drift detection",
        SEMANTIC_INDEX_ENGINE_VERSION, julie_extractors::EXTRACTION_CONTRACT_VERSION);
}
```

**Step 2: Run the test to verify it fails for the right reason**

Run: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract 2>&1 | tail -20`
Expected: FAIL because (a) `SEMANTIC_INDEX_ENGINE_VERSION` is `pub(crate)` so the import fails to compile, OR (b) the current value `"2026-05-05.reference-identifier-v3"` does not contain `EXTRACTION_CONTRACT_VERSION`. Either failure mode is the right reason.

**Step 3: Bump visibility + recompose**

In `src/tools/workspace/indexing/engine_version.rs:6`, change `pub(crate) const SEMANTIC_INDEX_ENGINE_VERSION: &str = "..."` to a `pub const` initialized via `concat!`:

```rust
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str = concat!(
    "extractors=",
    julie_extractors::EXTRACTION_CONTRACT_VERSION,
    "+schema-v",
    env!("CARGO_PKG_VERSION"),   // or a literal schema-version string if one is tracked elsewhere
);
```

If `concat!` cannot accept `EXTRACTION_CONTRACT_VERSION` because Rust requires literal string arguments to `concat!`, fall back to a `pub static SEMANTIC_INDEX_ENGINE_VERSION: std::sync::LazyLock<String>` (Rust 1.80+) or a `OnceLock<String>` populated in a `pub fn semantic_index_engine_version() -> &'static str` accessor — and update existing call-sites to use the accessor.

**Step 4: Run the test, verify it passes, commit**

Run: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract 2>&1 | tail -10`
Expected: PASS.

```bash
git add crates/julie-extractors/src/lib.rs src/tools/workspace/indexing/engine_version.rs src/tests/core/engine_version.rs
git commit -m "feat(extractors): expose EXTRACTION_CONTRACT_VERSION; engine version composes from it"
```

### Task 5.4: Public API audit — doc comments + stability declaration

**Files:**
- Modify: every `pub` item in `crates/julie-extractors/src/lib.rs` and the modules it exposes — add `///` doc comments

**Step 1: Generate the public-item inventory**

Use `cargo doc -p julie-extractors --no-deps` and inspect the produced rustdoc. Every public item without a doc comment is in scope.

Alternatively use `cargo +nightly rustdoc -- --output-format json` and parse for items missing docs.

**Step 2: Add doc comments**

For each public item: a one-sentence `///` comment minimum. For top-level structs/functions, a paragraph plus an `# Examples` section if non-trivial.

Stability: items currently re-exported by the main `julie` crate (`extract_canonical`, `ExtractorManager`, `ExtractionResults`, `Symbol`, `Relationship`, `Identifier`, etc.) get `///` comments noting they're stable. New items (`capability_snapshot`, `CapabilitySnapshot`, `EXTRACTION_CONTRACT_VERSION`) get a `/// **Stable.** ...` marker.

**Step 3: Add a missing-docs lint gate**

Add to `crates/julie-extractors/src/lib.rs`: `#![warn(missing_docs)]`. Convert to `#![deny(missing_docs)]` only after all items have comments.

**Step 4: Verify**

Run: `cargo doc -p julie-extractors --no-deps 2>&1 | grep -i warning`
Expected: no missing-docs warnings.

**Step 5: Commit**

```bash
git add crates/julie-extractors/
git commit -m "docs(extractors): add doc comments to every public item"
```

### Task 5.5: Crate-level rustdoc with runnable quickstart

**Files:**
- Modify: `crates/julie-extractors/src/lib.rs` — top-of-file `//!` block

**Step 1: Write the rustdoc + quickstart**

```rust
//! # julie-extractors
//!
//! Tree-sitter-backed code extraction for 34 languages plus TSX/JSX variants.
//! Produces a stable [`ExtractionResults`] shape: symbols, relationships,
//! structured pending relationships, identifiers, type info, and parse
//! diagnostics. Used by Julie's MCP server but consumable from any Rust crate.
//!
//! ## Quickstart
//!
//! ```
//! use julie_extractors::{extract_canonical, capability_snapshot};
//! use std::path::Path;
//!
//! let source = "fn main() { println!(\"hi\"); }";
//! let result = extract_canonical(
//!     "hello.rs",
//!     source,
//!     Path::new("."),
//! ).unwrap();
//! assert!(!result.symbols.is_empty());
//!
//! // Inspect what the crate guarantees for this language:
//! let cap = capability_snapshot().get("rust").unwrap();
//! assert!(cap.target_capabilities.symbols);
//! ```
//!
//! See [`EXTRACTION_CONTRACT_VERSION`] for drift detection.

#![warn(missing_docs)]
```

**Step 2: Run doctest**

Run: `cargo test -p julie-extractors --doc 2>&1 | tail -20`
Expected: PASS. The quickstart compiles and runs.

**Step 3: Commit**

```bash
git add crates/julie-extractors/src/lib.rs
git commit -m "docs(extractors): add crate-level rustdoc with runnable quickstart"
```

### Task 5.6: Example consumer

**Files:**
- Create: `crates/julie-extractors/examples/extract_file.rs`

**Step 1: Write the example**

```rust
//! Example: extract symbols from a file path argument.
//!
//! Run: `cargo run -p julie-extractors --example extract_file -- path/to/file.rs`

use julie_extractors::{capability_snapshot, extract_canonical};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("usage: extract_file <path>");
    let path = PathBuf::from(path);
    let file_path_str = path.to_str()
        .ok_or_else(|| anyhow::anyhow!("file path is not valid UTF-8: {:?}", path))?;
    let source = fs::read_to_string(&path)?;
    let workspace_root = path.parent().unwrap_or(Path::new("."));
    let result = extract_canonical(file_path_str, &source, workspace_root)?;
    println!("# {}", path.display());
    println!("Symbols: {}", result.symbols.len());
    for s in &result.symbols {
        println!("  - {} ({:?}) at line {}", s.name, s.kind, s.start_line);
    }
    println!("Relationships: {}", result.relationships.len());
    println!("Structured pending: {}", result.structured_pending_relationships.len());

    // Auxiliary: report the declared capabilities for the matching language
    // by extension lookup against the capability snapshot.
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let snap = capability_snapshot();
    if let Some(cap) = snap.languages()
        .find(|row| row.extensions.iter().any(|e| e == extension))
    {
        println!("\nLanguage: {}", cap.language);
        println!("Capabilities: symbols={} relationships={} pending={} identifiers={} types={}",
            cap.capabilities.symbols, cap.capabilities.relationships,
            cap.capabilities.pending_relationships, cap.capabilities.identifiers, cap.capabilities.types);
    } else {
        println!("\n(no julie-extractors language matches extension `.{}`; extraction still proceeded via filename heuristics)", extension);
    }
    Ok(())
}
```

**Step 2: Build the example**

Run: `cargo build --examples -p julie-extractors 2>&1 | tail -10`
Expected: success.

**Step 3: Smoke-test the example**

Run: `cargo run -p julie-extractors --example extract_file -- crates/julie-extractors/src/lib.rs`
Expected: prints symbol list, capability summary.

**Step 4: Commit**

```bash
git add crates/julie-extractors/examples/extract_file.rs
git commit -m "feat(extractors): add extract_file example demonstrating crate API"
```

### Task 5.7: Downstream-consumer integration test (replaces `cargo package --list` gate)

**Decision (resolves Codex finding #3 + cross-check Pillar 3 external crate usability):** `cargo package --list` does not prove the crate is usable downstream — it only enumerates files. Real `cargo package -p julie-extractors --allow-dirty` fails because four inherent git dependencies (`tree-sitter-qmljs`, `tree-sitter-razor`, `tree-sitter-powershell`, `tree-sitter-vb-dotnet`) lack crates.io versions, so the crate is not publishable to crates.io. The actual Pillar 3 contract is "consumable as a Rust path/git dependency from a downstream crate". This task proves it directly with an integration test that spawns a temp consumer crate, path-deps `julie-extractors`, and runs a program calling both `extract_canonical` and `capability_snapshot`.

**Files:**
- Create: `crates/julie-extractors/tests/downstream_smoke.rs`
- Modify: `xtask/src/test.rs` — register the new test in an appropriate bucket (extend `extractors` bucket)

**Step 1: Write the failing test**

```rust
//! Integration test that proves julie-extractors is usable from a downstream
//! Rust crate via a path dependency. This is the Pillar 3 gate — see plan
//! Task 5.7 for context.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn julie_extractors_works_as_path_dependency_in_downstream_crate() {
    // Locate this crate's manifest root via cargo metadata (do not hard-code).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let extractors_path = PathBuf::from(manifest_dir);
    assert!(
        extractors_path.join("Cargo.toml").exists(),
        "expected extractors Cargo.toml at {:?}",
        extractors_path
    );

    let tempdir = tempfile::tempdir().expect("create tempdir");
    let consumer = tempdir.path();

    let extractors_abs = extractors_path
        .canonicalize()
        .expect("canonicalize extractors path");
    let extractors_abs_str = extractors_abs.to_string_lossy().replace('\\', "/");

    fs::write(
        consumer.join("Cargo.toml"),
        format!(
            r#"[package]
name = "julie_extractors_downstream_smoke"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
julie-extractors = {{ path = "{extractors_abs_str}" }}
anyhow = "1.0"
"#
        ),
    )
    .expect("write consumer Cargo.toml");

    fs::create_dir_all(consumer.join("src")).expect("create src/");
    fs::write(
        consumer.join("src/main.rs"),
        r#"use std::path::Path;

fn main() -> anyhow::Result<()> {
    let source = "fn main() { println!(\"hi\"); }";
    let result = julie_extractors::extract_canonical("hello.rs", source, Path::new("."))?;
    assert!(!result.symbols.is_empty(), "expected at least one symbol");

    let snap = julie_extractors::capability_snapshot();
    let rust = snap.get("rust").expect("rust language row");
    assert!(rust.target_capabilities.symbols);

    let _version: &str = julie_extractors::EXTRACTION_CONTRACT_VERSION;
    Ok(())
}
"#,
    )
    .expect("write consumer main.rs");

    // Use an isolated target dir so we don't poison the parent workspace cache.
    let target_dir = consumer.join("target");

    let status = Command::new(env!("CARGO"))
        .args(["build", "--manifest-path"])
        .arg(consumer.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "downstream consumer crate failed to build");

    let run_status = Command::new(env!("CARGO"))
        .args(["run", "--manifest-path"])
        .arg(consumer.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("spawn cargo run");
    assert!(run_status.success(), "downstream consumer crate failed to run");
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in `crates/julie-extractors/Cargo.toml` if not already present.

**Step 2: Run + verify**

Run: `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate 2>&1 | tail -20`
Expected: PASS. The test takes ~30–60s because it spawns a real cargo build in a tempdir; that is the entire point of the gate.

**Step 3: Register the test in the extractors bucket**

In `xtask/src/test.rs` (or the bucket-registration module), confirm `cargo nextest run -p julie-extractors` picks up integration tests under `tests/`. If the existing bucket filters only `--lib`, add an `--all-targets` or `--test downstream_smoke` invocation so the gate runs as part of `cargo xtask test bucket extractors`.

**Step 4: Commit**

```bash
git add crates/julie-extractors/tests/downstream_smoke.rs crates/julie-extractors/Cargo.toml xtask/src/test.rs
git commit -m "feat(extractors): add downstream-consumer integration test as Pillar 3 gate"
```

**Phase 5 boundary gate:** `cargo xtask test changed`, `cargo doc -p julie-extractors --no-deps`, `cargo test -p julie-extractors --doc`, `cargo build --examples -p julie-extractors`, `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate`. Ledger row.

---

## Phase 6 — Real-World Evidence Regen

**Goal:** Lift real-world evidence from "non-zero relationships" to semantic correctness. Add VB.NET. Author per-repo specs. Extend `hard_failures` to enforce them.

### Task 6.1: Add VB.NET reference repo to corpus

**Files:**
- Modify: `fixtures/extraction/tree-sitter-real-world-corpus.toml` — add VB.NET row to `[profiles.release]` and `[[repos]]`

**Step 1: Identify a candidate repo**

Search `~/source/` for VB.NET projects. Candidates to evaluate (pick the first that exists with non-trivial code):
- `~/source/dotnet-runtime` (VB samples within)
- `~/source/visualstudio-extensibility-samples` (VB modules)
- A small focused VB library cloned during the run, e.g. `https://github.com/dotnet/samples` VB section

If none exist locally, the run clones a candidate to `~/source/<repo>/` and proceeds. If clone fails or no candidate is suitable, write `docs/plans/escalations/2026-05-10-vbnet-real-world.md` documenting the search and propose alternatives. Continue with the rest of Phase 6.

**Step 2: Add the corpus row**

```toml
[profiles.release]
repos = [
  ...existing 21...,
  "<vbnet-repo-name>",
]

[[repos]]
name = "<vbnet-repo-name>"
language = "vbnet"
profile_tags = ["release"]
min_relationships = 5  # tightened per Task 6.2 — see below
```

**Step 3: Commit**

```bash
git add fixtures/extraction/tree-sitter-real-world-corpus.toml
git commit -m "feat(corpus): add VB.NET reference repo to release profile"
```

### Task 6.2: Raise `min_relationships` per repo

**Files:**
- Modify: `fixtures/extraction/tree-sitter-real-world-corpus.toml`

**Step 1: Compute thresholds**

For each repo currently at `min_relationships = 1`, set the new minimum to `max(5, 5 × language_file_count_observed)`. Use the language_file_count from the existing `LANGUAGE_REAL_WORLD_EVIDENCE.json` as a baseline:

| Repo | Lang files (current evidence) | New min_relationships |
|---|---|---|
| Alamofire | 96 | 480 |
| Newtonsoft.Json | 945 | 4725 |
| Slim | 125 | 625 |
| cats | 835 | 4175 |
| cobra | 36 | 180 |
| gson | 259 | 1295 |
| jq | 47 | 235 |
| lite | 28 | 140 |
| moshi | 99 | 495 |
| nlohmann-json | 464 | 2320 |
| phoenix | 291 | 1455 |
| riverpod | 1156 | 5780 |
| sinatra | 147 | 735 |

For repos newly added to release (`pandora`, `zls`, `zod`, `flask`, `express`, `kirigami`, `blazor-samples`), use the same 5× rule based on their language file count from a quick `cargo xtask certify tree-sitter --real-world --profile release` dry run.

For VB.NET, use a conservative `min_relationships = 5` until a real run produces a baseline.

**Step 2: Run the certify and verify the new thresholds pass**

Run: `cargo xtask certify tree-sitter --real-world --profile release --out /tmp/baseline.json 2>&1 | tail -10`
Inspect `/tmp/baseline.json` for `hard_failures` per repo. Iterate the threshold values until each repo passes the new floor without being so loose that obvious regressions slip through.

**Step 3: Commit**

```bash
git add fixtures/extraction/tree-sitter-real-world-corpus.toml
git commit -m "feat(corpus): raise min_relationships from 1 to 5x language-file-count baseline"
```

### Task 6.3: Extend `hard_failures` to enforce specs (gate-first ordering)

**Sequencing decision (resolves Codex cross-check on §3 real-world):** Codex flagged that the original ordering (author specs, then build enforcement) lets spec mistakes accumulate before any gate catches them. We swap the order: build the enforcement gate first (this task), then author specs (Task 6.4). Each spec authored in 6.4 is validated by the existing `hard_failures` machinery at commit time, so typos and unresolvable specs surface immediately instead of at the Phase 6 boundary.

**Files:**
- Modify: `xtask/src/tree_sitter_real_world.rs:309` — `hard_failures` function
- Modify: same file's `TreeSitterRealWorldRepo` struct to include `representative_specs: Vec<RepresentativeSpec>`
- Modify: same file — add `RepresentativeSpec` enum/struct deserialization

**Step 1: Write the failing test**

Add to `xtask/src/tree_sitter_real_world.rs` (test module):

```rust
#[test]
fn hard_failures_enforces_representative_specs() {
    let repo = TreeSitterRealWorldRepo {
        name: "phoenix".to_string(),
        language: "elixir".to_string(),
        profile_tags: vec!["release".to_string()],
        min_files: 1,
        min_language_files: 1,
        min_symbols: 1,
        min_relationships: 1,
        max_parse_diagnostic_files: None,
        representative_specs: vec![
            RepresentativeSpec::ReferenceCountAtLeast { name: "Phoenix.Router".to_string(), min: 30 }
        ],
    };
    let counts = RepoCounts { /* ... pass thresholds */ };
    let db_path: PathBuf = /* test fixture DB */;
    let failures = hard_failures(&repo, &counts, &db_path);
    // If the test DB has only 5 references for Phoenix.Router, expect a failure.
    assert!(failures.iter().any(|f| f.contains("Phoenix.Router") && f.contains("at_least")),
        "expected hard failure for unsatisfied reference_count_at_least spec");
}
```

**Step 2: Run, fail, implement.**

Implement spec-driven assertions in `hard_failures`. Read symbol/relationship/identifier counts from the per-repo SQLite DB at `db_path` for the named symbols. Compose failure strings of the form `"phoenix: representative_specs.reference_count_at_least(Phoenix.Router): expected ≥30, got 5"`. Add a separate failure category `"representative_specs.unresolvable_symbol(...)"` that fires when a spec names a symbol that doesn't exist in the DB at all — this gives Task 6.4 immediate feedback when a typo'd spec is committed.

**Step 3: Run, pass, commit.**

```bash
git add xtask/src/tree_sitter_real_world.rs
git commit -m "feat(xtask): enforce representative correctness specs in hard_failures"
```

### Task 6.4: Author per-repo representative-correctness specs

**Files:**
- Modify: `fixtures/extraction/tree-sitter-real-world-corpus.toml` — extend `[[repos]]` with `[repos.representative_specs]` sub-table or array of inline tables
- Modify: `xtask/src/tree_sitter_real_world.rs` — extend `TreeSitterRealWorldRepo` to deserialize `representative_specs`

**Step 1: Define the spec schema**

Each spec row has:
- `kind`: `"symbol_kind"`, `"reference_count_at_least"`, `"parent_id_links"`, `"identifier_at_position"`, `"relationship_endpoints"`
- `language`: redundant but explicit (the repo's primary language by default)
- Spec-specific fields per kind. Examples:
  - `kind = "symbol_kind"`, `name = "Phoenix.Router"`, `expected_kind = "module"`
  - `kind = "reference_count_at_least"`, `name = "Phoenix.Router"`, `min = 30`
  - `kind = "parent_id_links"`, `child_name = "Phoenix.Router.match"`, `parent_name = "Phoenix.Router"`
  - `kind = "identifier_at_position"`, `name = "Phoenix.Router"`, `kind_filter = "type_usage"`, `file_path_contains = "lib/blog_web/router.ex"`, `line_min = 1`
  - `kind = "relationship_endpoints"`, `from_name = "Phoenix.Endpoint"`, `to_name = "Phoenix.Router"`, `relationship_kind = "Uses"`

**Step 2: Author specs for each release-profile repo (22 entries)**

This is hand-authored work, one block per repo. The lead authors at least one spec per repo and prefers 3–5 per repo. Examples:

```toml
[[repos]]
name = "phoenix"
language = "elixir"
# ...

[[repos.representative_specs]]
kind = "symbol_kind"
name = "Phoenix.Router"
expected_kind = "module"

[[repos.representative_specs]]
kind = "reference_count_at_least"
name = "Phoenix.Router"
min = 30

[[repos.representative_specs]]
kind = "identifier_at_position"
name = "Phoenix.Router"
kind_filter = "type_usage"
file_path_contains = "lib/blog_web/"
line_min = 1
```

Author analogous specs for: Alamofire, Newtonsoft.Json, Slim, cats, cobra, gson, jq, lite, moshi, nlohmann-json, riverpod, sinatra, julie (rust), pandora (gdscript), zls (zig), zod (typescript), flask (python), express (javascript), kirigami (qml), blazor-samples (razor), and the VB.NET reference repo.

For repos the lead doesn't have direct domain knowledge of, query the repo's README and top-level structure with `mcp__plugin_julie_julie__fast_search(query="...", search_target="definitions")` to identify representative core symbols.

**Step 3: Commit**

Per-commit validation: each repo's spec block is committed individually, and `cargo nextest run --test corpus_specs_resolve` (a small test added in Task 6.3 that loads the corpus.toml and runs each `representative_specs` entry against the existing real-world DB) MUST PASS for that commit. If a spec entry names a symbol the DB doesn't contain, the test surfaces `"representative_specs.unresolvable_symbol(...)"` and the commit is rolled back. Commit per ~5 repos (so the diff is reviewable). Each commit message: `feat(corpus): author representative specs for <repo> [, <repo>, ...]`.

### Task 6.5: Regenerate evidence at HEAD with `--profile release`

**Files:**
- Modify: `docs/LANGUAGE_REAL_WORLD_EVIDENCE.{json,md}` — regenerated, no hand edits

**Steps:**

```bash
cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json
cargo xtask certify tree-sitter --check  # regenerates the .md companion
```

If any repo fails: triage, fix the source defect (extractor bug, fixture, or spec), commit the fix, regenerate. Do NOT loosen specs to make them pass without lead approval.

Commit:

```bash
git add docs/LANGUAGE_REAL_WORLD_EVIDENCE.json docs/LANGUAGE_REAL_WORLD_EVIDENCE.md docs/LANGUAGE_CERTIFICATION_REPORT.md
git commit -m "feat(docs): regenerate real-world evidence with release profile + semantic specs at HEAD"
```

**Phase 6 boundary gate:** `cargo xtask test changed`, `cargo xtask test dogfood`. Ledger row.

---

## Phase 7 — Doc Cleanup

### Task 7.1: Delete `LANGUAGE_VERIFICATION_CHECKLIST.md`

```bash
git rm docs/LANGUAGE_VERIFICATION_CHECKLIST.md
git commit -m "docs: delete restored historical LANGUAGE_VERIFICATION_CHECKLIST"
```

### Task 7.2: Harvest + delete `LANGUAGE_VERIFICATION_RESULTS.md`

For each row in the file's "Known Limitations" table, if not already represented in `capabilities.json` exception rows, add it. Then delete the file.

```bash
git rm docs/LANGUAGE_VERIFICATION_RESULTS.md
git add fixtures/extraction/capabilities.json
git commit -m "docs: harvest known-limitations into capabilities.json exception rows; delete RESULTS"
```

### Task 7.3: Delete `docs/verification/` directory

```bash
git rm -r docs/verification/
git commit -m "docs: delete restored historical per-language verification notes"
```

### Task 7.4: Commit `docs/findings/` deletions

The original session already staged these. Commit them:

```bash
git add -u docs/findings/
git commit -m "docs: remove dead per-LLM audit findings; capabilities.json now uses typed evidence"
```

### Task 7.5: Write `docs/EXTRACTION_CONTRACT.md`

**Files:**
- Create: `docs/EXTRACTION_CONTRACT.md` (≤200 lines)

Sections:
1. **Overview.** What `julie-extractors` extracts, in one paragraph. Link to the rubric.
2. **Tier model.** Reproduce the four target groups from the Quality Bar, with one sentence per tier.
3. **`ExtractionResults` shape.** Field-by-field reference for `Symbol`, `Relationship`, `Identifier`, `TypeInfo`, `ParseDiagnostic`, `NormalizedSpan`. Link to source.
4. **Structured pending relationship contract.** Required fields per the rubric §2.1. Reference `crates/julie-extractors/src/base/relationship_resolution.rs:7-26` for `UnresolvedTarget`.
5. **Capability snapshot API.** How downstream consumers read per-language guarantees.
6. **Typed evidence schema.** What `evidence` objects in `capabilities.json` look like.
7. **Where to find machine-checked truth.** Three pointers: capabilities.json, LANGUAGE_CERTIFICATION_REPORT.md, LANGUAGE_REAL_WORLD_EVIDENCE.json.

Keep ≤200 lines. Commit:

```bash
git add docs/EXTRACTION_CONTRACT.md
git commit -m "docs: add EXTRACTION_CONTRACT.md downstream-facing reference"
```

### Task 7.6: Update `docs/TREE_SITTER_QUALITY_BAR.md`

Refresh the "Current Verdict" and "Current Open Gaps" sections to reflect the run's outcome. Move every previously-open gap to a closed/exception status with a date and a reference to its closing test or PR.

```bash
git add docs/TREE_SITTER_QUALITY_BAR.md
git commit -m "docs: refresh Quality Bar verdict and open-gaps sections"
```

**Phase 7 boundary gate:** None — doc edits only. The Phase 8 release-gate sweep is the next gate.

---

## Phase 8 — Release Gates + Live MCP Dogfood Handoff

### Task 8.1: Run all release gates at HEAD, record ledger

Run, in order, and append a ledger row per command:

```bash
cargo fmt --check
git diff --check
cargo xtask certify tree-sitter --check
cargo xtask test bucket extractors
cargo xtask test bucket parser-upgrade
cargo xtask test changed
cargo xtask test system
cargo xtask test dogfood
cargo xtask test full
cargo build --release
cargo build --examples -p julie-extractors
cargo test -p julie-extractors --doc
cargo doc -p julie-extractors --no-deps
cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate
```

If any fails: stop, root-cause, fix, recommit, restart from the failed gate. Do not skip.

### Task 8.2: Stage live MCP dogfood note

**Files:**
- Create: `docs/plans/2026-05-10-best-in-class-tree-sitter-handoff.md`

Contents: a one-page handoff document for the user, listing the exact post-rebuild dogfood checks from the rubric §6:

```markdown
# Best-in-Class Tree-Sitter — Live Dogfood Handoff

After the autonomous run completes, the user runs:

1. `cargo build --release`
2. Restart Claude Code (so the MCP client respawns the new server).
3. In the Julie workspace, run via the MCP client:
   - `manage_workspace health` — expect ready status.
   - `call_path extract_symbols_static extract_canonical` — expect a one-hop edge.
   - `fast_refs extract_canonical` — expect definition + references.
   - SQLite check: inspect the on-disk index metadata for the engine version column actually written by the indexer (verify the column name against `src/database/schema.rs` before running). The recorded value must contain `EXTRACTION_CONTRACT_VERSION` per Task 5.3's composition.
   - `manage_workspace refresh workspace_id=julie_<id>` — expect "already up-to-date" without full reindex.
4. Sign off: append a ledger row to the rubric file with timestamp + result.
5. Merge `.worktrees/best-in-class-treesitter/` back to `main`.
```

Commit:

```bash
git add docs/plans/2026-05-10-best-in-class-tree-sitter-handoff.md
git commit -m "docs: add live MCP dogfood handoff note for user sign-off"
```

### Task 8.3: Final cleanup + merge prep

- Verify the worktree's HEAD has no uncommitted changes (`git status` is empty).
- Verify the verification ledger at the bottom of this plan has a row for every release gate run.
- Verify the rubric file's verification ledger has rows for every closed criterion.
- Verify all open escalation files in `docs/plans/escalations/` either have a resolution note or are summarized in a final escalation report.
- Push the worktree branch.
- Notify the user via a final commit message or output that the run is complete and live dogfood is pending.

---

## Verification Ledger

| Invariant | Command | Scope label | Commit SHA | Result | Timestamp (UTC) | Evidence reused |
|---|---|---|---|---|---|---|
| Phase 1 boundary gate | `cargo xtask test changed` | phase-1-changed | c2eb72b6 | Pass — `changed` reports "no code/test buckets matched local changes" (xtask diff mapper has no bucket for `crates/julie-extractors/src/tests/`, `xtask/src/`, `xtask/tests/`, or `fixtures/extraction/capabilities.json`). Extractors bucket run explicitly: 3/3 PASS. xtask test bin: 144/144 PASS. | 2026-05-10T19:16:55Z | No |
| Phase 2 boundary gate | `cargo xtask test bucket extractors` (substituting `changed` no-op) | phase-2-extractors | e6a9806c | Pass — 3/3 commands (golden, capability_matrix, certify --check); cert report regenerated at HEAD. `cargo xtask test changed` returns no buckets matched (xtask diff mapper has no trigger for the touched files). | 2026-05-10T19:22:28Z | No |
| Phase 3 SQL pending closure | `cargo nextest run -p julie-extractors --lib test_sql_emits_structured_pending_for_cross_file_fk` | sql-pending-closure | a4beac3a | Pass — 1/1 (RED → GREEN); also tests::sql 30/30, tests::capability_matrix 14/14, tests::pending_shape_contract 1/1, tests::golden 1/1 | 2026-05-10T19:38:00Z | No |
| Phase 3 JSON $ref closure | `cargo nextest run -p julie-extractors --lib test_json_emits` | json-ref-closure | 8120d04e | Pass — 3/3 (RED → GREEN: local_ref + external_ref + malformed_ref); also tests::json 64/64, tests::capability_matrix 16/16, tests::pending_shape_contract 1/1, tests::golden 1/1 | 2026-05-10T19:46:37Z | No |
| Phase 3 TOML closure | `cargo nextest run -p julie-extractors --lib tests::toml::relationships` | toml-closure | 2e4c50c7 | Pass — 3/3 (RED → GREEN: cargo_dependencies + pyproject_tool_tables + arbitrary_no_relationship); also tests::toml 58/58, tests::capability_matrix 16/16, tests::pending_shape_contract 1/1, tests::golden 1/1 | 2026-05-10T19:52:50Z | No |
| Phase 3 boundary gate | `cargo xtask test bucket extractors` | phase-3-extractors | 2e4c50c7 | Pass — 3/3 commands (golden, capability_matrix, certify --check) green at HEAD; cert report regenerated. Phase 3 SQL+JSON+TOML closures all visible in matrix. | 2026-05-10T19:52:50Z | No |
| Phase 4a general programming gate | `cargo xtask test dev` | phase-4a-dev | _TBD_ | _TBD_ | _TBD_ | No |
| Phase 4b component/template gate | `cargo xtask test changed` | phase-4b-changed | _TBD_ | _TBD_ | _TBD_ | No |
| Phase 4c query/declarative gate | `cargo xtask test changed` | phase-4c-changed | _TBD_ | _TBD_ | _TBD_ | No |
| Phase 4d doc/data gate | `cargo xtask test dev` | phase-4d-dev | _TBD_ | _TBD_ | _TBD_ | No |
| Phase 5 hardening gate | `cargo doc + cargo test --doc + cargo build --examples + downstream_smoke integration test` | phase-5-pillar3 | _TBD_ | _TBD_ | _TBD_ | No |
| Phase 6 real-world gate | `cargo xtask test dogfood` + `cargo xtask certify tree-sitter --check` | phase-6-realworld | _TBD_ | _TBD_ | _TBD_ | No |
| Final formatter | `cargo fmt --check` | release-formatter | _TBD_ | _TBD_ | _TBD_ | No |
| Final cert check | `cargo xtask certify tree-sitter --check` | release-cert | _TBD_ | _TBD_ | _TBD_ | No |
| Final extractors bucket | `cargo xtask test bucket extractors` | release-extractors | _TBD_ | _TBD_ | _TBD_ | No |
| Final parser-upgrade bucket | `cargo xtask test bucket parser-upgrade` | release-parser-upgrade | _TBD_ | _TBD_ | _TBD_ | No |
| Final changed tier | `cargo xtask test changed` | release-changed | _TBD_ | _TBD_ | _TBD_ | No |
| Final system tier | `cargo xtask test system` | release-system | _TBD_ | _TBD_ | _TBD_ | No |
| Final dogfood tier | `cargo xtask test dogfood` | release-dogfood | _TBD_ | _TBD_ | _TBD_ | No |
| Final full tier | `cargo xtask test full` | release-full | _TBD_ | _TBD_ | _TBD_ | No |
| Release build | `cargo build --release` | release-build | _TBD_ | _TBD_ | _TBD_ | No |
| Examples build | `cargo build --examples -p julie-extractors` | release-examples | _TBD_ | _TBD_ | _TBD_ | No |
| Doctest | `cargo test -p julie-extractors --doc` | release-doctest | _TBD_ | _TBD_ | _TBD_ | No |
| Rustdoc | `cargo doc -p julie-extractors --no-deps` | release-rustdoc | _TBD_ | _TBD_ | _TBD_ | No |
| Pillar 3 downstream smoke | `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate` | release-downstream-smoke | _TBD_ | _TBD_ | _TBD_ | No |
| Live MCP health (manual) | `manage_workspace health` | live-health | _TBD_ | _TBD_ (user) | _TBD_ | No |
| Live MCP call_path (manual) | `call_path extract_symbols_static extract_canonical` | live-call-path | _TBD_ | _TBD_ (user) | _TBD_ | No |
| Live MCP refs (manual) | `fast_refs extract_canonical` | live-refs | _TBD_ | _TBD_ (user) | _TBD_ | No |
| Live SQLite state (manual) | `sqlite3 ... select schema_version, semantic_engine_version` | live-sqlite | _TBD_ | _TBD_ (user) | _TBD_ | No |
| Live MCP refresh (manual) | `manage_workspace refresh` | live-refresh | _TBD_ | _TBD_ (user) | _TBD_ | No |

**Reuse rule:** If the same HEAD already has a passing ledger entry for the required scope, reuse it instead of rerunning. Each row records its commit SHA so reuse is traceable.

---

## Iteration Discipline (for the autonomous /loop driver)

- **Per-task budget.** 3 failed iterations OR 90 min wall-clock without measurable progress on a single task → write `docs/plans/escalations/2026-05-10-<task-id>.md` and continue with other tasks.
- **Per-phase checkpoint.** After each phase, commit + push + write a brief progress note to `.memories/2026-05-10/<phase-id>.md`.
- **Hard stop.** 5+ open escalations OR `cargo xtask test full` fails with a regression that survives gap closure → stop, write summary, wait for user.
- **Subagent rules.** Workers run only narrow targeted tests (`cargo nextest run --lib <name>`). The lead orchestrates `cargo xtask test changed` between batches and `cargo xtask test full` for the section-5 release gate.
- **Pillar-aware grading.** The /loop driver reads the rubric file (`docs/plans/2026-05-10-best-in-class-tree-sitter-rubric.md`) each iteration and scores per criterion. A criterion that flips from `satisfied` back to `needs_revision` due to later edits triggers a regression escalation.
