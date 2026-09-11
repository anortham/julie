# Machine Service Phase 6a: Output, Guidance, and Head-to-Head Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Keep Julie's proven tool surface (names, descriptions, instructions, skills) and ship the parts of phase 6 that have evidence behind them: compact output with stateless paging, server instructions that fit the host budget plus a session-start routing hook, deletion of the two edit tools nothing calls, the no-argument git-diff form of `blast_radius`, and the head-to-head retrieval matrix against Miller on the fresh public corpus.

**Architecture:** No tool is renamed and no tool is folded. `edit_file` and its `editing` skill stay. `rewrite_symbol` and `rename_symbol` are deleted with the `refactoring` crate module and the symbol-rewrite half of `editing`. Paging is an `offset` parameter that re-runs the same query against the same snapshot and ends the output with a `next:` line; there is no continuation store. The long agent guidance moves from `JULIE_AGENT_INSTRUCTIONS.md` (which the host truncates near 2 KB) into a session-start hook, so nothing an agent relied on is lost; it is delivered through a channel that is not truncated.

**Tech Stack:** Rust (`serde`, `schemars`), Node hook script (port of Miller's), Python 3 head-to-head harness (port of Miller's `bench-foundation-matrix.py` and `benchlib/mcp_client.py`).

**Architecture Quality:** Owner decision 2026-09-10: the combination of tool name, description, server instructions, and skill is one fragile unit that has worked and failed unpredictably across iterations; do not change it without telemetry. So phase 6 splits. This plan is 6a. Phase 6b (Miller names, `inspect` and `trace` folds, telemetry migration) waits for two weeks of `tool_calls` data from real Julie sessions and is not scheduled. Main risk in 6a: the compact renderer changes what every search returns, so Task 3 carries the `dogfood` gate and the `search_quality` bucket is a hard gate for it.

## Global Constraints

- Design: `docs/plans/2026-09-09-machine-service-design.md` sections 9, 12, 14 item 6. Decision record: `docs/findings/2026-09-10-tool-contract-comparison.md`. Split rationale: memory note `keep-proven-tool-surface` and the finding this plan's Task 7 writes.
- Public tool names after this plan, exactly the ten: `blast_radius`, `call_path`, `deep_dive`, `edit_file`, `fast_refs`, `fast_search`, `get_context`, `get_symbols`, `manage_workspace`, `patterns`. `AVAILABLE_TOOLS` in `src/request_engine/catalog.rs` is the single source of truth.
- Tool descriptions in `src/request_engine/catalog.rs` and in the `#[tool(description = …)]` attributes do not change in this plan, except to delete the two removed tools and to add the `offset` and `git` parameters' field docs.
- Paging: `fast_search`, `fast_refs`, `blast_radius`, and `get_symbols` take `offset: u32` (default 0). When rows remain, the last output line is exactly `next: <tool> <same args> offset=<offset+kept>`. No byte caps, no continuation tokens.
- `fast_search.return_format` gains the value `compact` and it becomes the default. `full` keeps today's output. `locations` is removed.
- `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters (Claude Code truncates merged server instructions near 2 KB). It must still contain the words the docs contract asserts: `fast_search`, `patterns`, `regions`, `source_regions`, `structural_facts`, `complexity_metrics`.
- The session-start routing block (`.claude/hooks/julie-routing-block.md`) carries the full guidance, at most 4,000 bytes, and names every tool with the same one-line purpose the instructions use.
- Head-to-head corpus: the ten public repos in `docs/eval/semantic-value/scorecard.toml` under `/home/murphy/source/`. Task 6 records each repo's `git rev-parse HEAD` in `cases.json` before the first run. Never index the manifest, expected answers, or results.
- Telemetry: `registry.db` migration 008 adds `client`, `client_session`, `julie_version` to `tool_calls`; `result_count` is filled for every tool except `manage_workspace`; tests never write the live registry.
- Net lines: this phase must be negative against the branch base `5eafea53` (`tokei` before and after, recorded in the gate finding).
- Commit messages: conventional commits, one commit per task, on branch `contract` in `/home/murphy/source/julie/.worktrees/contract`.
- No pushes, no releases, no `.gitignore` or `.git/info/exclude` edits, no tool caches committed, no edits to `~/source/miller` or `~/source/julie-plugin`.
- File size: no line limit. Split only by responsibility.
- Test rules from `CLAUDE.md`: workers run exact tests only, at most two runs per change. The lead runs `cargo xtask test changed` and the tiers.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` sections "RUNNING TESTS" and "Canonical Test Tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name>` for top-crate tests; `cargo nextest run -p julie-tools <exact_test_name>` for `crates/julie-tools`; `cargo test -p xtask --test docs_contract_tests <exact_test_name>` for the docs contract.

**Worker ceiling:** the exact test names written in each task. Workers never run `cargo xtask test …` or an unfiltered `cargo nextest run`.

**Worker gate invariant:** each task lists its invariant under **Acceptance criteria**. The invariant shared by Tasks 1 through 5 is `catalog_lists_exactly_the_ten_tools` in `src/tests/request_engine.rs`.

**Lead affected-change scope:** `cargo xtask test changed` after each task lands; `cargo xtask test bucket cli` and `cargo xtask test bucket service` after Task 1; `cargo xtask test system` and the live-registry row-count check after Task 2; `cargo xtask test dogfood` after Task 3; `cargo test -p xtask --test docs_contract_tests` after Task 5.

**Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test dogfood`, `cargo xtask test full`. Then the phase 6a gate finding (Task 7). Phase 4 showed that only `full` caught cross-subsystem regressions, so the lead runs `full` after Task 1, Task 2, and Task 3 as well.

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: every test named in this plan passes; `search_quality` bucket passes at Task 3; `fast` median under 10 s; trimmed `full` median under 120 s; net lines negative; `JULIE_AGENT_INSTRUCTIONS.md` at most 1,900 characters. Report-only: head-to-head top-5 per task class, latency per tool, resident memory, `tool_calls` baseline counts.

**Escalation triggers:** any change to `src/service/`, `src/request_engine/`, or `src/handler.rs` runs `cargo xtask test system`. Any change under `crates/julie-tools/src/search/` runs `cargo xtask test dogfood`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-10-machine-service-phase6-ledger.md` using `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only when scope label and HEAD match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Delete `rewrite_symbol` and `rename_symbol` | None - serial | Delete `crates/julie-tools/src/refactoring/`, `crates/julie-tools/src/editing/{rewrite_symbol,symbol_lookup}.rs`, `src/handler/tools/{rewrite_symbol,rename_symbol}.rs`, `crates/julie-tools/src/tests/refactoring_ast_aware.rs`, tests listed in the task. Modify `crates/julie-tools/src/editing/mod.rs`, `crates/julie-tools/src/lib.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands,generic,catalog}.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/dashboard/routes/metrics.rs`, `.claude/skills/editing/SKILL.md`, `JULIE_AGENT_INSTRUCTIONS.md` (two bullets only), `README.md`, `docs/site/{index.html,script.js}`, `xtask/tests/docs_contract_tests.rs`, `src/tests/request_engine.rs`. | Yes | Clears the surface before output changes. |
| Task 2: Telemetry parity | None - serial | Modify `src/registry/database/{migrations,tool_calls}.rs`, `src/handler/tool_metrics.rs`, `src/handler/mcp_adapter.rs`, `src/service/shim.rs`, `src/handler/tools/{get_symbols,fast_refs,deep_dive,call_path,blast_radius,get_context,patterns,edit_file}.rs`, `src/tests/registry/database.rs`, `src/tests/service/shim.rs`, `src/tests/core/handler/metrics_recording.rs`, tests that write the live registry. | Yes | Needs Task 1's tool list; later output changes are measured with these columns. |
| Task 3: Compact output and offset paging | None - serial | Create `crates/julie-tools/src/tests/search_paging_tests.rs`. Modify `crates/julie-tools/src/search/{params,tool_execution,formatting,region_search}.rs`, `crates/julie-tools/src/shared.rs`, `crates/julie-tools/src/impact/{mod,formatting}.rs`, `crates/julie-tools/src/get_context/formatting.rs`, `crates/julie-tools/src/navigation/{fast_refs,formatting}.rs`, `crates/julie-tools/src/symbols/{mod,formatting}.rs`, `crates/julie-tools/src/tests/{mod,formatting_tests}.rs`, `src/handler/search_telemetry.rs`, `src/cli_tools/subcommands.rs`, `docs/eval/semantic-value/{run_scorecard.py,scorecard.toml}`, tests listed in the task. | Yes | Needs Task 1's catalog and Task 2's columns. |
| Task 4: `blast_radius` git-diff seeding | None - serial | Create `crates/julie-tools/src/impact/git_seed.rs`, `crates/julie-tools/src/tests/impact_git_seed_tests.rs`. Modify `crates/julie-tools/src/impact/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/handler/tools/blast_radius.rs`, `src/handler/tool_targets.rs`, `src/cli_tools/subcommands.rs`, tests listed in the task. | Yes | Needs Task 3's `next_line`. |
| Task 5: Instructions, routing hook, docs | None - serial | Modify `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/hooks.json`, `xtask/tests/docs_contract_tests.rs`, `src/tests/core/workspace_init/instructions_paths.rs`, `CLAUDE.md`, `AGENTS.md`. Create `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`. | Yes | Parameters must be final. |
| Task 6: Head-to-head run | None - serial | Create `docs/eval/head-to-head/{run_matrix.py,mcp_client.py,cases.json,README.md}`, `docs/eval/head-to-head/results/<timestamp>.{json,md}`, `docs/findings/2026-09-1X-head-to-head-miller.md`. | Yes | Needs the finished output shape. |
| Task 7: Phase 6a gate finding | None - serial | Create `docs/findings/2026-09-1X-machine-service-phase6a-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`. Modify `docs/plans/2026-09-09-machine-service-design.md` (phase 6 note), `CLAUDE.md`, `AGENTS.md`. | Yes | Measures the finished branch. |

Commit mode for every task: `serial-worker-commit`.

---

## Task 1: Delete `rewrite_symbol` and `rename_symbol`

**Files:**
- Delete: `crates/julie-tools/src/refactoring/` (4 files), `crates/julie-tools/src/editing/rewrite_symbol.rs`, `crates/julie-tools/src/editing/symbol_lookup.rs` (only if `cargo check` shows no remaining user after the rewrite deletion; same rule for `crates/julie-tools/src/editing/ast_validation.rs`), `src/handler/tools/rewrite_symbol.rs`, `src/handler/tools/rename_symbol.rs`, `crates/julie-tools/src/tests/refactoring_ast_aware.rs`
- Modify: `crates/julie-tools/src/editing/mod.rs:9-12`, `crates/julie-tools/src/lib.rs:14,27`, `src/tools/mod.rs:20,31`, `src/request_engine/catalog.rs:225-243` (two rows), `src/request_engine/dispatch.rs:246-262` (two arms), `src/handler.rs:1346-1359` (router composition), `src/handler.rs:262-270`, `src/handler.rs:1401-1420` (`is_write_exempt` keeps `edit_file`), `src/handler/tools/mod.rs:21-22`, `src/handler/tool_targets.rs:124-160` (`rename_symbol_metadata`, `rewrite_symbol_metadata`, the `"kind": "rename_symbol"` literal), `src/cli.rs:63-67` (`rename`, `rewrite` subcommands), `src/cli_tools/subcommands.rs` (`RenameArgs`, `RewriteArgs`), `src/cli_tools/commands.rs:419-460`, `src/cli_tools/generic.rs:14-27,39-94` (two names, two arms), `src/cli_tools/catalog.rs:10,31` (replace the literal `13` with `AVAILABLE_TOOLS.len()`), `src/tools/metrics/session.rs:40-78` (delete `RenameSymbol`, `RewriteSymbol`, and the dead `QueryMetrics`; renumber; `COUNT` = 9), `src/dashboard/search_analysis.rs:8-17` (drop the two names), `src/dashboard/routes/metrics.rs:118-120` (`is_edit_tool` matches only `edit_file`), `.claude/skills/editing/SKILL.md` (remove the two tools from `description`, `allowed-tools`, the "Which tool" list, and the two subsections; keep everything about `edit_file`), `JULIE_AGENT_INSTRUCTIONS.md:21,24` (delete the two bullets; Task 5 rewrites the rest), `README.md` and `docs/site/{index.html,script.js}` (delete the two tool cards and mentions), `xtask/tests/docs_contract_tests.rs:133-160` (ten cards), `src/tests/request_engine.rs` (catalog count test)
- Test: `src/tests/request_engine.rs`

**Interfaces:**
- Consumes: `register_tool_catalog!` rows in `src/request_engine/catalog.rs:121-243`; `ToolKind` in `src/tools/metrics/session.rs`.
- Produces: a ten-tool catalog. Later tasks rely on `AVAILABLE_TOOLS` being exactly `["blast_radius","call_path","deep_dive","edit_file","fast_refs","fast_search","get_context","get_symbols","manage_workspace","patterns"]`.

**Contract inputs:** the current catalog table; `edit_file` and its `EditFileTool` (`crates/julie-tools/src/editing/edit_file.rs:175`) stay untouched.

**File ownership:** Delete `crates/julie-tools/src/refactoring/`, `crates/julie-tools/src/editing/{rewrite_symbol,symbol_lookup}.rs`, `src/handler/tools/{rewrite_symbol,rename_symbol}.rs`, `crates/julie-tools/src/tests/refactoring_ast_aware.rs`, tests listed in the task. Modify `crates/julie-tools/src/editing/mod.rs`, `crates/julie-tools/src/lib.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands,generic,catalog}.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/dashboard/routes/metrics.rs`, `.claude/skills/editing/SKILL.md`, `JULIE_AGENT_INSTRUCTIONS.md` (two bullets only), `README.md`, `docs/site/{index.html,script.js}`, `xtask/tests/docs_contract_tests.rs`, `src/tests/request_engine.rs`.

**Serialization required:** Yes

**Dependency reason:** Clears the surface before output changes.

**Step 1: Write the failing test**

Replace the existing twelve-tool catalog count test in `src/tests/request_engine.rs` with:

```rust
#[test]
fn catalog_lists_exactly_the_ten_tools() {
    use crate::request_engine::catalog::{AVAILABLE_TOOLS, ToolCatalog};

    let expected = [
        "blast_radius", "call_path", "deep_dive", "edit_file", "fast_refs", "fast_search",
        "get_context", "get_symbols", "manage_workspace", "patterns",
    ];
    assert_eq!(AVAILABLE_TOOLS, &expected);
    let listed: Vec<&str> = ToolCatalog::list().iter().map(|t| t.name).collect();
    assert_eq!(listed, expected);
    for old in ["rewrite_symbol", "rename_symbol"] {
        assert!(ToolCatalog::schema(old).is_err(), "{old} still has a schema");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run --lib catalog_lists_exactly_the_ten_tools 2>&1 | tail -10`
Expected: FAIL on the `AVAILABLE_TOOLS` assertion (twelve names today).

**Step 3: Write minimal implementation**

1. `git rm -r` every path in the **Delete** list (decide `symbol_lookup.rs` and `ast_validation.rs` by `cargo check` after the rest is gone: delete each only when nothing else uses it).
2. `src/request_engine/catalog.rs`: delete the `"rename_symbol"` and `"rewrite_symbol"` rows.
3. `src/request_engine/dispatch.rs:246-262`: delete the `RenameSymbol` and `RewriteSymbol` arms.
4. `src/handler.rs:1346-1359`: delete `+ Self::tool_router_rename_symbol()` and `+ Self::tool_router_rewrite_symbol()`. `src/handler.rs:262-270`: drop `"rename_symbol"` from the match and delete the separate `"rewrite_symbol" => workspace_is_primary` arm. `src/handler.rs:1405`: `if tool_name == "edit_file" { return true; }`.
5. `src/handler/tools/mod.rs`: delete `pub(crate) mod rename_symbol;` and `pub(crate) mod rewrite_symbol;`. `src/handler/tool_targets.rs`: delete `rename_symbol_metadata`, `rewrite_symbol_metadata`, and the `"kind": "rename_symbol"` literal at `:126`.
6. `crates/julie-tools/src/editing/mod.rs`: delete `pub mod rewrite_symbol;` and `mod symbol_lookup;` (and `pub mod ast_validation;` if deleted). `crates/julie-tools/src/lib.rs`: delete `pub mod refactoring;` and `pub use refactoring::RenameSymbolTool;`. `src/tools/mod.rs`: delete `pub use julie_tools::refactoring;` and `pub use refactoring::RenameSymbolTool;`.
7. `src/cli.rs:63-67`: delete the `rename` and `rewrite` subcommands and their aliases. `src/cli_tools/subcommands.rs`: delete `RenameArgs` and `RewriteArgs`. `src/cli_tools/commands.rs:419-460`: delete the two `CliToolCommand` impls. `src/cli_tools/generic.rs`: delete the two names from `AVAILABLE_TOOLS` and the two match arms; fix the doc comment at `:4` ("All 13") to say the catalog count. `src/cli_tools/catalog.rs:10,31`: use `crate::request_engine::catalog::AVAILABLE_TOOLS.len()`.
8. `src/tools/metrics/session.rs`: delete `RenameSymbol`, `QueryMetrics`, `RewriteSymbol` variants and arms; renumber; `COUNT` = 9. `src/dashboard/search_analysis.rs:8-17`: `USEFUL_ACTIONS` keeps `deep_dive, get_symbols, fast_refs, call_path, get_context, edit_file`. `src/dashboard/routes/metrics.rs:118-120`: `matches!(tool_name, "edit_file")`.
9. `.claude/skills/editing/SKILL.md`: the skill keeps its shape and every `edit_file` sentence. Delete the `rewrite_symbol` and `rename_symbol` lines in `description` and `allowed-tools`, the two bullets in "Which tool do I use?", the two table rows that name them, and the two subsections. The "Which tool" list becomes: new file → Write; any text change → `edit_file`; need structure first → `get_symbols` or `deep_dive`, then `edit_file`.
10. `JULIE_AGENT_INSTRUCTIONS.md`: delete the `rename_symbol` and `rewrite_symbol` bullets (`:21`, `:24`) and the two sentences in "Editing Workflow" that name them. Nothing else changes here; Task 5 rewrites the file.
11. `README.md`, `docs/site/index.html`, `docs/site/script.js`: delete the two tool cards and every mention. `xtask/tests/docs_contract_tests.rs:133-160`: the card count becomes ten.
12. Tests: delete `crates/julie-tools/src/tests/refactoring_ast_aware.rs` and every test function whose subject is one of the two tools in `src/tests/cli_execution_tests.rs`, `src/tests/cli_tools_tests.rs`, `src/tests/cli_input_contract.rs`, `src/tests/request_scenarios.rs`, `src/tests/mcp_protocol_contract.rs`, `src/tests/request_transport_parity.rs`, `src/tests/edit_recovery_contract.rs`, `src/tests/core/handler/{editing_metrics,metrics_recording,public_surface,workspace_binding_metrics}.rs`, `src/tests/dashboard/{search_analysis,state}.rs`, `src/tests/dashboard/integration/metrics.rs`, `src/tests/tools/metrics/session_metrics_tests.rs`, `src/tests/registry/database.rs`, `crates/julie-tools/src/tests/{extractor_migration_editing,formatting_tests}.rs`, and the inline tests in `src/cli_tools/generic.rs:130-350`. Edit, not delete, a test that only counts tools. Update `crates/julie-tools/src/tests/mod.rs` and `src/tests/mod.rs`. Rename or delete the `tools-refactoring` bucket in `xtask/test_tiers.toml` and its mappings under `xtask/src/changed/` (`cargo xtask test list` must not name an empty bucket).
13. `cargo check --workspace --all-targets` until clean. Every error is a dead rewrite or rename path; delete, do not stub.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run --lib catalog_lists_exactly_the_ten_tools 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "refactor(tools): delete rewrite_symbol and rename_symbol"` and record the SHA.

**Acceptance criteria:**
- [x] `catalog_lists_exactly_the_ten_tools` passes.
- [ ] `cargo check --workspace --all-targets` is clean; `grep -rn "rewrite_symbol\|rename_symbol\|RenameSymbolTool\|RewriteSymbolTool" src crates xtask xtask-eval .claude README.md docs/site JULIE_AGENT_INSTRUCTIONS.md --include='*' ` returns nothing.
- [x] `edit_file` still works: `julie-server edit src/lib.rs --old-text "<any line>" --new-text "<same line>" --dry-run` (or the equivalent `tool edit_file --params`) returns a preview.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 2: Telemetry parity for the phase 6b decision

**Files:**
- Modify: `src/registry/database/migrations.rs` (add `migration_008_add_tool_call_client_columns`), `src/registry/database/tool_calls.rs:60-100` (insert takes `client`, `client_session`; writes `julie_version`), `src/handler/tool_metrics.rs:46-215` (`MetricsTask` and `record_tool_call_outcome` carry the client), `src/handler/mcp_adapter.rs:218-231` (read `request.meta`), `src/service/shim.rs:30-106` (stamp `_meta` on every `tools/call`), `src/handler/tools/{get_symbols,fast_refs,deep_dive,call_path,blast_radius,get_context,patterns,edit_file}.rs` (fill `result_count`), the tests under `src/tests/service/` and `src/tests/tools/search_quality/helpers.rs` that write the live registry
- Test: `src/tests/registry/database.rs`, `src/tests/service/shim.rs`, `src/tests/core/handler/metrics_recording.rs`

**Interfaces:**
- Consumes: `tool_calls` (migration 001 and 005 shape: `workspace_id, session_id, timestamp, tool_name, duration_ms, result_count, source_bytes, input_bytes, output_bytes, success, metadata`); `ToolCallReport { result_count, input_bytes, source_bytes, output_bytes, metadata, source_file_paths }`; rmcp 3.0.1 `CallToolRequestParams.meta: Option<RequestMetaObject>` (the MCP `_meta` object); the shim's `forward` loop, which already rewrites each message in `bind_default_workspace`.
- Produces: three new columns `client TEXT`, `client_session TEXT`, `julie_version TEXT`; `result_count` populated for every tool except `manage_workspace`; a `CallClient { name: String, session: String }` value that travels from the shim to the metrics row. Task 7 queries these columns for the baseline.

**Contract inputs:** Miller's `tool_telemetry` has `outcome IN ('ok','empty','error')`, `error_kind`, `miller_version`, and `op`. Julie reaches parity for the decision without new enums: `empty` is `success = 1 AND result_count = 0`; error text is `metadata->>'error.message'`; `op` is the tool's mode field in `metadata` (`return_format`, `backend`, `mode`, `depth`, `operation`). Two things Miller lacks and Julie adds: `client` (the MCP client name and version from `initialize`, for example `claude-code/2.1.0` or `grok/1.0.25`) and `client_session` (one id per shim process, so tool chains within one agent session can be reconstructed). The existing `session_id` column is per handler, one per workspace, and stays as it is.

**File ownership:** Modify `src/registry/database/{migrations,tool_calls}.rs`, `src/handler/tool_metrics.rs`, `src/handler/mcp_adapter.rs`, `src/service/shim.rs`, `src/handler/tools/{get_symbols,fast_refs,deep_dive,call_path,blast_radius,get_context,patterns,edit_file}.rs`, `src/tests/registry/database.rs`, `src/tests/service/shim.rs`, `src/tests/core/handler/metrics_recording.rs`, and the tests that write the live registry.

**Serialization required:** Yes

**Dependency reason:** Needs Task 1's tool list; Task 3's output change must be measured with these columns in place.

**Step 1: Write the failing test**

In `src/tests/registry/database.rs`:

```rust
#[test]
fn tool_call_rows_carry_client_session_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = DaemonDatabase::open(&dir.path().join("registry.db")).unwrap();
    db.insert_tool_call_with_input_bytes(
        "ws", "handler-session", "fast_refs", 3.0, Some(0), None, Some(40), Some(120), true, None,
        Some(&CallClient { name: "grok/1.0.25".into(), session: "shim-1".into() }),
    )
    .unwrap();
    let (client, session, version, count): (String, String, String, i64) = db
        .conn_for_test()
        .query_row(
            "SELECT client, client_session, julie_version, result_count FROM tool_calls",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((client.as_str(), session.as_str(), count), ("grok/1.0.25", "shim-1", 0));
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}
```

In `src/tests/service/shim.rs`, next to the existing `bind_default_workspace` tests:

```rust
#[test]
fn shim_stamps_client_and_session_meta_on_tool_calls() {
    let mut stamp = ClientStamp::new("shim-1");
    let mut init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"clientInfo":{"name":"grok","version":"1.0.25"}}});
    stamp.observe(&mut init);
    let mut call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"fast_refs","arguments":{"symbol":"x"}}});
    stamp.observe(&mut call);
    assert_eq!(call["params"]["_meta"]["julie"]["client"], "grok/1.0.25");
    assert_eq!(call["params"]["_meta"]["julie"]["session"], "shim-1");
    assert!(init["params"].get("_meta").is_none());
}
```

In `src/tests/core/handler/metrics_recording.rs`, one test per tool that asserts `result_count` is `Some(n)` with the fixture's known count after a successful call (use the existing fixture helpers in that file; eight tools: `get_symbols`, `fast_refs`, `deep_dive`, `call_path`, `blast_radius`, `get_context`, `patterns`, `edit_file`). Name each `<tool>_records_result_count`.

**Step 2: Run test to verify it fails**

Run: `cargo nextest run --lib tool_call_rows_carry_client_session_and_version 2>&1 | tail -10`
Expected: FAIL to compile (`CallClient` and the extra argument do not exist).

**Step 3: Write minimal implementation**

1. `src/registry/database/migrations.rs`: register `migration_008_add_tool_call_client_columns` after 007 with three `ALTER TABLE tool_calls ADD COLUMN` statements: `client TEXT`, `client_session TEXT`, `julie_version TEXT`. Follow migration 005's shape.
2. `src/registry/database/tool_calls.rs`: add

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallClient {
    pub name: String,
    pub session: String,
}
```

   and an eleventh parameter `client: Option<&CallClient>` on `insert_tool_call_with_input_bytes`; the INSERT writes `client.map(|c| &c.name)`, `client.map(|c| &c.session)`, and `env!("CARGO_PKG_VERSION")`. Update every caller (`grep -rn insert_tool_call_with_input_bytes src`).
3. `src/handler/tool_metrics.rs`: add `pub client: Option<CallClient>` to `MetricsTask` and pass it to the insert. Add

```rust
tokio::task_local! {
    pub(crate) static CALL_CLIENT: Option<CallClient>;
}
```

   and in `record_tool_call_outcome` set `client: CALL_CLIENT.try_with(|c| c.clone()).ok().flatten()`.
4. `src/handler/mcp_adapter.rs:218-231`: before `adapt_request`, read `request.meta` and take `meta["julie"]["client"]` and `meta["julie"]["session"]` as strings into `Option<CallClient>`; wrap `self.engine.execute(req, app_ctx)` in `CALL_CLIENT.scope(client, async { ... }).await`. If the engine crosses a `tokio::spawn` boundary so the task-local is empty inside the tool (the shim test in `src/tests/service/` that runs a real call will show `client` NULL), carry the value on the engine request context instead and say so in the commit body.
5. `src/service/shim.rs`: add

```rust
pub struct ClientStamp {
    session: String,
    client: Option<String>,
}

impl ClientStamp {
    pub fn new(session: impl Into<String>) -> Self {
        Self { session: session.into(), client: None }
    }

    pub fn observe(&mut self, message: &mut serde_json::Value) {
        match message["method"].as_str() {
            Some("initialize") => {
                let info = &message["params"]["clientInfo"];
                if let Some(name) = info["name"].as_str() {
                    let version = info["version"].as_str().unwrap_or("");
                    self.client = Some(format!("{name}/{version}"));
                }
            }
            Some("tools/call") => {
                message["params"]["_meta"]["julie"] = serde_json::json!({
                    "client": self.client.clone().unwrap_or_default(),
                    "session": self.session,
                });
            }
            _ => {}
        }
    }
}
```

   In `forward`, create `let mut stamp = ClientStamp::new(uuid::Uuid::new_v4().to_string());` before the loop and call `stamp.observe(&mut message)` right after `bind_default_workspace`.
6. `result_count` in the eight tool wrappers: `get_symbols` = symbols rendered; `fast_refs` = references returned; `deep_dive` = 1 when the symbol resolved, else 0; `call_path` = paths found (0 or 1); `blast_radius` = impacted symbols; `get_context` = items in the bundle; `patterns` = rows returned (pattern ids in list mode); `edit_file` = 1 when the edit applied or the dry run matched, else 0. Each wrapper already builds a `ToolCallReport`; set its `result_count` from the value the tool computed, never by re-parsing the rendered text.
7. Live-registry pollution: `crates/julie-core/src/paths.rs:239-260` falls back to `~/.julie` when `JULIE_HOME` is unset. Find every test that reaches the live registry (the live `~/.julie/registry.db` holds 33 rows from workspaces named `ws__tmp*`, `tmp_*`, and `target_*`, written 2026-09-08 to 2026-09-10 by the service tests under `src/tests/service/` and the search-quality helpers). Give each such test a temporary `JULIE_HOME` (`tempfile::tempdir` plus the existing helper that sets the env for the spawned process, or `RegistryPaths::with_home(temp)` where the test constructs paths directly). Do not edit `.cargo/config.toml`.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run --lib tool_call_rows_carry_client_session_and_version 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run --lib shim_stamps_client_and_session_meta_on_tool_calls 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run --lib _records_result_count 2>&1 | tail -15`
Expected: PASS (8 tests).

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(telemetry): record client, client session, version, and result counts for every tool"` and record the SHA.

**Acceptance criteria:**
- [x] The ten new tests pass.
- [ ] After `cargo build --release` and `julie-server service restart`, one `fast_refs` call from this Claude Code session produces a row whose `client` starts with the client's name, whose `client_session` is non-empty, whose `julie_version` equals `Cargo.toml`, and whose `result_count` is not NULL (lead verifies with `sqlite3 ~/.julie/registry.db`).
- [ ] `sqlite3 ~/.julie/registry.db "select count(*) from tool_calls"` is unchanged after the lead runs `cargo xtask test dev` and `cargo xtask test system` at this commit.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 3: Compact output and offset paging

**Files:**
- Create: `crates/julie-tools/src/tests/search_paging_tests.rs`
- Modify: `crates/julie-tools/src/search/params.rs:14-190` (`FastSearchTool`: `return_format` default `compact`, add `offset`), `crates/julie-tools/src/search/tool_execution.rs:200-260` (format switch, paging), `crates/julie-tools/src/search/region_search.rs:126` (`"locations"` handling), `crates/julie-tools/src/search/formatting.rs` (compact renderer), `crates/julie-tools/src/shared.rs:11-13` (replace `truncation_line` with `next_line`), `crates/julie-tools/src/impact/mod.rs:42` (`offset`), `crates/julie-tools/src/impact/formatting.rs:100`, `crates/julie-tools/src/get_context/formatting.rs:191,252`, `crates/julie-tools/src/navigation/fast_refs.rs:42` (`offset`), `crates/julie-tools/src/navigation/formatting.rs` (refs paging trailer), `crates/julie-tools/src/symbols/mod.rs:80` (`offset`), `crates/julie-tools/src/symbols/formatting.rs` (listing paging trailer), `crates/julie-tools/src/tests/{mod,formatting_tests}.rs`, `src/handler/search_telemetry.rs` (log `return_format` and `offset`), `src/cli_tools/subcommands.rs` (`--offset` on `SearchArgs`, `RefsArgs`, `SymbolsArgs`, `BlastRadiusArgs`), `docs/eval/semantic-value/run_scorecard.py` (parse the compact shape), `docs/eval/semantic-value/scorecard.toml` (`return_format = "compact"`)
- Test: `crates/julie-tools/src/tests/search_paging_tests.rs`, `crates/julie-tools/src/tests/formatting_tests.rs`

**Interfaces:**
- Consumes: `FastSearchTool` fields (`query`, `language`, `file_pattern`, `limit`, `context_lines`, `exclude_tests`, `backend`, `workspace`, `return_format: String`, `semantics`), wrapper `FastSearchParams { search, regions }`; `execute_search_unified` in `crates/julie-tools/src/search/execution/mod.rs:70` and its `SearchHit` rows (`SearchHitBacking` distinguishes symbol, file, and line hits); `FastRefsTool.limit`, `BlastRadiusTool.limit`, `GetSymbolsTool.limit`.
- Produces: `return_format` values `compact` (default) and `full`; `offset: u32` on the four tools; `julie_tools::shared::next_line(tool, args, next_offset) -> String`; `render_compact` in `crates/julie-tools/src/search/formatting.rs`. Task 4 relies on `next_line`; Task 6 relies on `compact` being the default.

**Contract inputs:** the compact shape, exactly:

```
<N> hits for "<query>" (<backend>)
<path>:
  :<line> <Name> <kind>
  :<line> <Name> <kind>
<path>:<line> <Name> <kind>
next: fast_search query="<query>" offset=<offset+kept>
```

A file that appears once renders on one line; a file that appears more than once renders as a group. File-row hits render as `<path>` alone. Line hits render as `<path>:<line>` plus the matched line trimmed to 110 characters. The `next:` line appears only when more rows exist. `<backend>` is `lexical`, `semantic`, `hybrid`, or `auto`. With `return_format=full` the current full renderer runs unchanged.

**File ownership:** Create `crates/julie-tools/src/tests/search_paging_tests.rs`. Modify `crates/julie-tools/src/search/{params,tool_execution,formatting,region_search}.rs`, `crates/julie-tools/src/shared.rs`, `crates/julie-tools/src/impact/{mod,formatting}.rs`, `crates/julie-tools/src/get_context/formatting.rs`, `crates/julie-tools/src/navigation/{fast_refs,formatting}.rs`, `crates/julie-tools/src/symbols/{mod,formatting}.rs`, `crates/julie-tools/src/tests/{mod,formatting_tests}.rs`, `src/handler/search_telemetry.rs`, `src/cli_tools/subcommands.rs`, `docs/eval/semantic-value/{run_scorecard.py,scorecard.toml}`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 1's catalog and Task 2's columns so the new output is measured.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/search_paging_tests.rs`:

```rust
use crate::search::FastSearchParams;
use crate::shared::next_line;

#[test]
fn fast_search_defaults_to_compact_format_and_zero_offset() {
    let p: FastSearchParams = serde_json::from_value(serde_json::json!({ "query": "x" })).unwrap();
    assert_eq!(p.search.return_format, "compact");
    assert_eq!(p.search.offset, 0);
}

#[test]
fn fast_search_rejects_the_removed_locations_format() {
    let p: FastSearchParams =
        serde_json::from_value(serde_json::json!({ "query": "x", "return_format": "locations" })).unwrap();
    let err = p.search.validated_format().unwrap_err();
    assert!(err.to_string().contains("compact"), "{err}");
}

#[test]
fn next_line_names_the_tool_and_the_next_offset() {
    assert_eq!(
        next_line("fast_search", &[("query", "\"a b\"")], 16),
        "next: fast_search query=\"a b\" offset=16"
    );
    assert_eq!(
        next_line("fast_refs", &[("symbol", "Foo")], 10),
        "next: fast_refs symbol=Foo offset=10"
    );
}
```

In `crates/julie-tools/src/tests/formatting_tests.rs` add (write `fn hit(path, line, name, kind) -> SearchHit` in that file, building the symbol-backed variant of `SearchHit` with `SearchHitBacking` the way `make_line_hit` in `search_lean_format_tests.rs` builds the line-backed variant):

```rust
#[test]
fn compact_search_groups_repeated_files_and_appends_next_when_rows_remain() {
    let hits = vec![
        hit("src/a.rs", 10, "alpha", "function"),
        hit("src/a.rs", 20, "beta", "function"),
        hit("src/b.rs", 5, "gamma", "struct"),
    ];
    let text = crate::search::formatting::render_compact("q", "lexical", &hits, 0, 3, true);
    assert_eq!(
        text,
        "3 hits for \"q\" (lexical)\nsrc/a.rs:\n  :10 alpha function\n  :20 beta function\nsrc/b.rs:5 gamma struct\nnext: fast_search query=\"q\" offset=3"
    );
    let last_page = crate::search::formatting::render_compact("q", "lexical", &hits, 0, 3, false);
    assert!(!last_page.contains("next:"));
}
```

Register the new module in `crates/julie-tools/src/tests/mod.rs`.

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools fast_search_defaults_to_compact_format_and_zero_offset 2>&1 | tail -10`
Expected: FAIL (`return_format` is `"full"` today; `offset` missing).

**Step 3: Write minimal implementation**

1. `crates/julie-tools/src/search/params.rs`: default `return_format` to `"compact"` (`:163`), add `pub offset: u32` with `julie_core::serde_lenient::deserialize_u32_lenient` and `#[serde(default)]`, and add:

```rust
impl FastSearchTool {
    pub fn validated_format(&self) -> anyhow::Result<&str> {
        match self.return_format.as_str() {
            "compact" | "full" => Ok(self.return_format.as_str()),
            other => anyhow::bail!("Invalid return_format: '{other}'. Expected compact or full"),
        }
    }
}
```

   Update the field doc at `:50-51` to `Return format: "compact" (default, one line per hit grouped by file) or "full" (code context and rich summaries)`. Update `Default` at `:186`.
2. `crates/julie-tools/src/shared.rs`: replace `truncation_line` with:

```rust
/// Trailer for a paged result: the exact call that returns the next page.
pub fn next_line(tool: &str, args: &[(&str, &str)], next_offset: usize) -> String {
    let mut line = format!("next: {tool}");
    for (key, value) in args {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(value);
    }
    line.push_str(&format!(" offset={next_offset}"));
    line
}
```

3. `crates/julie-tools/src/search/tool_execution.rs`: call `validated_format()` first and return its error as the tool error. Request `min(500, limit + offset)` rows, skip `offset`, keep `limit`, and pass `more = fetched.len() > offset + limit`. Route `compact` to `render_compact` and `full` to today's renderer. Delete the `== "locations"` branches at `:209`, `:220` and in `region_search.rs:126`.
4. `crates/julie-tools/src/search/formatting.rs`: add `pub fn render_compact(query: &str, backend: &str, hits: &[SearchHit], offset: usize, kept: usize, more: bool) -> String` producing the shape in **Contract inputs**.
5. Paging on the other three tools: add `offset: u32` (lenient, default 0) to `FastRefsTool`, `BlastRadiusTool`, and `GetSymbolsTool`; in each execution path fetch `limit + offset`, skip `offset`, keep `limit`, and when more remain append `next_line("fast_refs", &[("symbol", &symbol)], offset + kept)`, `next_line("blast_radius", &seed_args, …)` (seed args are `file_paths=<comma list>` or `symbol_ids=<comma list>`), or `next_line("get_symbols", &[("file_path", &path)], …)`. Replace `truncation_line` at `impact/formatting.rs:100` with the trailer; delete the two `truncation_line` calls in `get_context/formatting.rs:191,252` (context is budgeted, not paged; the token budget already bounds it).
6. `src/handler/search_telemetry.rs`: add `return_format` and `offset` to the metadata. `src/cli_tools/subcommands.rs`: `--offset` on the four arg structs; `--return-format` help text lists `compact|full`.
7. `docs/eval/semantic-value/scorecard.toml`: `return_format = "compact"`. `docs/eval/semantic-value/run_scorecard.py`: parse `<path>:<line>` rows and `<path>:` group headers followed by `  :<line>` rows; ignore the header and `next:` lines. Run one lexical dry run (`JULIE_HOME=<temp> JULIE_EMBEDDING_PROVIDER=none python3 docs/eval/semantic-value/run_scorecard.py --binary target/release/julie-server --backend lexical --no-write --timeout 300`) and record the top-5 count in the commit body (6 of 23 on 2026-09-10 before this change; a lower number is a parser bug).
8. Rewrite test literals: `"locations"` → `"compact"`, and every assertion on the old `Output truncated at` line (`grep -rn 'truncation_line\|Output truncated\|"locations"' src crates docs/eval`).

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools fast_search_defaults_to_compact_format_and_zero_offset 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run -p julie-tools compact_search_groups_repeated_files_and_appends_next_when_rows_remain 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(search): compact output by default with offset paging on search, refs, symbols, and impact"` and record the SHA.

**Acceptance criteria:**
- [x] The four new tests pass; `julie-server search "bind_default_workspace" --json` returns the compact shape; `--offset 6` returns the next page; `--return-format full` returns today's full output; `--return-format locations` returns the invalid-format error.
- [x] `grep -rn "truncation_line\|\"locations\"" src crates docs/eval` returns nothing except the required reject test `fast_search_rejects_the_removed_locations_format`.
- [ ] `cargo xtask test dogfood` passes at this commit (lead runs it; worker reports ready).
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 4: `blast_radius` git-diff seeding

**Files:**
- Create: `crates/julie-tools/src/impact/git_seed.rs`, `crates/julie-tools/src/tests/impact_git_seed_tests.rs`
- Modify: `crates/julie-tools/src/impact/mod.rs:42-100` (`git: bool` field; seed resolution), `crates/julie-tools/src/tests/mod.rs`, `src/handler/tools/blast_radius.rs` (pass the workspace root), `src/handler/tool_targets.rs:111-123` (`seed` key), `src/cli_tools/subcommands.rs` (`--git` on `BlastRadiusArgs`)
- Test: `crates/julie-tools/src/tests/impact_git_seed_tests.rs`

**Interfaces:**
- Consumes: `BlastRadiusTool { symbol_ids, file_paths, max_depth, limit, offset, include_tests, format, workspace, mode }`; the workspace root the handler already resolves for the tool (`WorkspaceTarget` in `crates/julie-tools/src/navigation/resolution.rs`).
- Produces: `git: bool` on `BlastRadiusTool`; with no `symbol_ids`, no `file_paths`, and `git` unset, the tool behaves as `git=true`: it reads the working-tree diff and seeds `file_paths` from it. `changed_files_from_git_output(diff, untracked) -> Vec<String>` and `changed_files(root) -> Result<Vec<String>>` in `git_seed.rs`.

**Contract inputs:** `git diff --name-only HEAD` plus `git ls-files --others --exclude-standard`, run in the workspace root. The description in `src/request_engine/catalog.rs:124` becomes `"Deterministic impact analysis for changed symbols or files. With no arguments it reads the working-tree git diff."`; the `#[tool(description)]` in `src/handler/tools/blast_radius.rs` gets the same sentence appended.

**File ownership:** Create `crates/julie-tools/src/impact/git_seed.rs`, `crates/julie-tools/src/tests/impact_git_seed_tests.rs`. Modify `crates/julie-tools/src/impact/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/handler/tools/blast_radius.rs`, `src/handler/tool_targets.rs`, `src/cli_tools/subcommands.rs`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 3's `next_line`.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/impact_git_seed_tests.rs`:

```rust
use crate::impact::BlastRadiusTool;
use crate::impact::git_seed::changed_files_from_git_output;

#[test]
fn blast_radius_with_no_seed_uses_the_git_diff() {
    let tool: BlastRadiusTool = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(tool.seeds_from_git());
    let explicit: BlastRadiusTool =
        serde_json::from_value(serde_json::json!({ "file_paths": ["src/a.rs"] })).unwrap();
    assert!(!explicit.seeds_from_git());
}

#[test]
fn changed_files_merges_diff_and_untracked_output() {
    let paths = changed_files_from_git_output("src/a.rs\nsrc/b.rs\n", "new.rs\n\n");
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs", "new.rs"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools blast_radius_with_no_seed_uses_the_git_diff 2>&1 | tail -10`
Expected: FAIL to compile (`seeds_from_git`, `git_seed` missing).

**Step 3: Write minimal implementation**

`crates/julie-tools/src/impact/git_seed.rs`:

```rust
//! Seeds `blast_radius` from the working-tree git diff when no seed is given.

use std::path::Path;

use anyhow::{Context, Result};

pub fn changed_files(root: &Path) -> Result<Vec<String>> {
    let run = |args: &[&str]| -> Result<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .with_context(|| format!("git {} in {}", args.join(" "), root.display()))?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    Ok(changed_files_from_git_output(
        &run(&["diff", "--name-only", "HEAD"])?,
        &run(&["ls-files", "--others", "--exclude-standard"])?,
    ))
}

pub fn changed_files_from_git_output(diff: &str, untracked: &str) -> Vec<String> {
    diff.lines()
        .chain(untracked.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}
```

In `crates/julie-tools/src/impact/mod.rs`: add `pub mod git_seed;`, add to `BlastRadiusTool`:

```rust
    /// Seed from the working-tree git diff. Default when symbol_ids and file_paths are both empty.
    #[serde(default, deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient")]
    pub git: bool,
```

and

```rust
impl BlastRadiusTool {
    pub fn seeds_from_git(&self) -> bool {
        self.git || (self.symbol_ids.is_empty() && self.file_paths.is_empty())
    }
}
```

In the tool's `call_tool` (or the handler's `execute_blast_radius`, whichever already resolves the workspace root), when `seeds_from_git()` is true replace `file_paths` with `git_seed::changed_files(root)?` before the existing seed logic; when the list is empty return the text `No changed files in the working tree.` Add `("git", "true")` to the `next_line` seed args from Task 3 when seeding from git. `tool_targets::blast_radius_metadata` adds `"seed": "git" | "files" | "symbols"`. `BlastRadiusArgs` gets `--git`. Update the two descriptions named in **Contract inputs**.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools blast_radius_ 2>&1 | tail -10`
Expected: PASS (2 tests).

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(impact): seed blast_radius from the working-tree git diff when no seed is given"` and record the SHA.

**Acceptance criteria:**
- [x] The two tests pass; with one edited tracked file in the checkout, `julie-server blast-radius --json` lists impacted symbols and likely tests for that file; with a clean tree it returns the no-changes text.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 5: Instructions, routing hook, and docs

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md` (rewrite, at most 1,900 characters), `.claude/hooks/hooks.json`, `xtask/tests/docs_contract_tests.rs:222-249` (add the budget test), `src/tests/core/workspace_init/instructions_paths.rs:34` (still asserts `fast_search`), `CLAUDE.md` and `AGENTS.md` (Quick Reference and CLI list lose `rename-symbol` and `rewrite-symbol`; the `fast_search(query="workspace routing", search_target="definitions", …)` example becomes `fast_search(query="workspace routing", file_pattern="docs/**")`)
- Create: `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`
- Test: `xtask/tests/docs_contract_tests.rs`

**Interfaces:**
- Consumes: `include_str!("../JULIE_AGENT_INSTRUCTIONS.md")` at `src/handler.rs:1335`; the ten tool names and the `offset`, `return_format`, `git` parameters from Tasks 1 through 4; Miller's routing block and session hook as read-only models (`~/source/miller/hooks/miller-routing-block.md`, `~/source/miller/hooks/miller-session-hook.cjs`).
- Produces: the short server instructions; the routing block that carries the full guidance; the dev session hook. `xtask sync-plugin` (`xtask/src/sync_plugin.rs`) reports hook divergence and does not copy hooks, so the plugin repo is a follow-up.

**Contract inputs:** the current `JULIE_AGENT_INSTRUCTIONS.md` (93 lines, 9,400 characters) is the content source. Every rule, tool bullet, workflow, CLI note, and the subagent paste-block move into the routing block, condensed but complete. The server instructions keep the five rules and the ten one-line tool bullets.

**File ownership:** Modify `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/hooks.json`, `xtask/tests/docs_contract_tests.rs`, `src/tests/core/workspace_init/instructions_paths.rs`, `CLAUDE.md`, `AGENTS.md`. Create `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`.

**Serialization required:** Yes

**Dependency reason:** Parameters must be final.

**Step 1: Write the failing test**

In `xtask/tests/docs_contract_tests.rs`:

```rust
#[test]
fn docs_contract_tests_agent_instructions_fit_the_server_instruction_budget() {
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    let count = instructions.chars().count();
    assert!(count <= 1900, "JULIE_AGENT_INSTRUCTIONS.md is {count} characters; the ceiling is 1900");
    for name in [
        "fast_search", "get_symbols", "deep_dive", "fast_refs", "call_path", "get_context",
        "blast_radius", "patterns", "edit_file", "manage_workspace",
        "regions", "source_regions", "structural_facts", "complexity_metrics",
    ] {
        assert!(instructions.contains(&format!("`{name}`")), "instructions must name {name}");
    }
}

#[test]
fn docs_contract_tests_routing_block_carries_the_full_guidance() {
    let block = read_repo_file(".claude/hooks/julie-routing-block.md");
    assert!(block.len() <= 4000, "routing block is {} bytes; the ceiling is 4000", block.len());
    for name in [
        "fast_search", "get_symbols", "deep_dive", "fast_refs", "call_path", "get_context",
        "blast_radius", "patterns", "edit_file", "manage_workspace", "offset", "dry_run",
    ] {
        assert!(block.contains(name), "routing block must mention {name}");
    }
    let hooks: serde_json::Value =
        serde_json::from_str(&read_repo_file(".claude/hooks/hooks.json")).unwrap();
    assert!(hooks["hooks"]["SessionStart"].is_array(), "hooks.json registers SessionStart");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p xtask --test docs_contract_tests docs_contract_tests_agent_instructions_fit_the_server_instruction_budget 2>&1 | tail -10`
Expected: FAIL (`9400 characters`).

**Step 3: Write minimal implementation**

1. `JULIE_AGENT_INSTRUCTIONS.md`, complete text (1,842 characters; keep it under 1,900 after any edit):

```markdown
# Julie - Code Intelligence Server

## Rules

1. Search before coding: `fast_search` before writing new code.
2. Structure before reading: `get_symbols` before Read.
3. References before changes: `fast_refs` before modifying a symbol.
4. `deep_dive` before modifying a symbol; one call replaces the search, symbols, refs, Read chain.
5. Trust results. Pre-indexed and accurate; never verify with grep, find, or Read.

## Tools

- `fast_search`: find code by text, symbol name, path fragment, or concept. `file_pattern` and `language` scope it. `backend` lexical, semantic, or hybrid. `regions` filters to `source_regions` kinds.
- `get_symbols`: file structure without reading it. `target` plus `mode="minimal"` extracts one symbol.
- `deep_dive`: one symbol: definition, callers, callees, children, types, `complexity_metrics`.
- `fast_refs`: every reference to a symbol; `reference_kind` filters.
- `call_path`: one shortest call path between two symbols.
- `get_context`: token-budgeted area orientation for a task; give `entry_symbols`, `edited_files`, `stack_trace`, or `failing_test`.
- `blast_radius`: impact of changed files or symbols plus likely tests. With no arguments it reads the working-tree git diff.
- `patterns`: query persisted `structural_facts` (routes, config keys, SQL, document structure). No arguments lists pattern ids.
- `edit_file`: edit without reading first; `old_text` is fuzzy matched. Always `dry_run=true` first.
- `manage_workspace`: index, list, open, remove, refresh, rebuild, health, status, dashboard.

Output is compact by default. A result with more rows ends with a `next:` line holding the exact call for the next page. `return_format="full"` adds code context.

Every call takes `workspace`: omit it for the checkout this session started in, or pass the id `manage_workspace(operation="list")` returns.
```

2. `.claude/hooks/julie-routing-block.md`: the content of today's `JULIE_AGENT_INSTRUCTIONS.md` sections "Editing Workflow", "Other Workflows", "CLI Dogfooding", "External Extract CLI", and "Subagent Dispatching", condensed, with the two deleted tools removed and the `offset`, `return_format`, and `git` parameters added where the tools are described. Start with a one-line title and the ten-tool list from the instructions above (same wording), then the workflows. At most 4,000 bytes.
3. `.claude/hooks/session-start.cjs`: port of `miller-session-hook.cjs` with `ROUTING_BLOCK_FILE = 'julie-routing-block.md'`, kill switch `JULIE_SESSION_HOOKS` (`0` or `false` prints nothing and exits 0), events `session-start` and `subagent-start`, the Claude/Codex envelope `{hookSpecificOutput:{hookEventName, additionalContext}}`, bounded stdin, fail-open on any exception, and no candidate-root appendix. `.claude/hooks/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [
      { "matcher": "startup|resume|clear|compact", "hooks": [ { "type": "command", "command": "node .claude/hooks/session-start.cjs session-start", "timeout": 10 } ] }
    ],
    "SubagentStart": [
      { "hooks": [ { "type": "command", "command": "node .claude/hooks/session-start.cjs subagent-start", "timeout": 10 } ] }
    ]
  }
}
```

   Leave `pretool-agent.cjs`, `pretool-edit.cjs`, and `session-start-tests.cjs` as they are, except: delete any line in them that names `rewrite_symbol` or `rename_symbol`.
4. `CLAUDE.md` and `AGENTS.md`: apply the edits named in **Files**; keep both files identical (`hooks/pre-commit` enforces it).
5. Run `cargo xtask sync-plugin --dry-run` and paste its report into the commit body; do not run the live sync.

**Step 4: Run test to verify it passes**

Run: `cargo test -p xtask --test docs_contract_tests 2>&1 | tail -15`
Expected: all docs contract tests PASS.

Run: `cargo nextest run --lib instructions_paths 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "docs(guidance): server instructions under the host budget, full guidance in a session-start routing hook"` and record the SHA.

**Acceptance criteria:**
- [x] The two new docs contract tests pass; `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters.
- [x] `node .claude/hooks/session-start.cjs session-start < /dev/null` prints a JSON envelope whose `additionalContext` starts with the routing block; with `JULIE_SESSION_HOOKS=0` it prints nothing and exits 0.
- [x] Nothing that was in the old instructions is absent from the union of the new instructions and the routing block, except the two deleted tools (the worker lists every old section and where it went in the commit body).
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 6: Head-to-head run against Miller

**Files:**
- Create: `docs/eval/head-to-head/run_matrix.py`, `docs/eval/head-to-head/mcp_client.py`, `docs/eval/head-to-head/cases.json`, `docs/eval/head-to-head/README.md`, `docs/eval/head-to-head/results/<timestamp>.json`, `docs/eval/head-to-head/results/<timestamp>.md`, `docs/findings/2026-09-1X-head-to-head-miller.md`
- Test: none in Rust; the harness self-checks (`python3 docs/eval/head-to-head/run_matrix.py --self-check` and `--validate`)

**Interfaces:**
- Consumes: Miller's `scripts/bench-foundation-matrix.py` and `scripts/benchlib/mcp_client.py` (read-only at `~/source/miller`); Miller binary `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller` (build with `dotnet build -c Release` in that repo if absent; do not edit that repo); Julie stdio shim `target/release/julie-server` run with `cwd=<repo>`; the ten public repos from `docs/eval/semantic-value/scorecard.toml` and their 23 search cases.
- Produces: `cases.json` with a top-level `repos` object `{name: {path, commit}}` and rows `{id, repo, task_class, intent, julie:{tool,args}, miller:{tool,args}, expected:{path,anchor}, scoring:{mode}}`; a results pair; the finding.

**Contract inputs:** the frozen contract in `docs/plans/2026-09-07-julie-head-to-head-evaluation.md` (task classes, "never index answers", paired runs, report variation). This task runs the retrieval matrix only (search and symbol-inspect rows); agent episodes and the Rust `xtask-eval revival` harness stay deferred, and the finding says so.

**File ownership:** Create `docs/eval/head-to-head/{run_matrix.py,mcp_client.py,cases.json,README.md}`, `docs/eval/head-to-head/results/<timestamp>.{json,md}`, `docs/findings/2026-09-1X-head-to-head-miller.md`.

**Serialization required:** Yes

**Dependency reason:** Needs the finished output shape.

**Step 1: Write the failing test**

`run_matrix.py --validate` must fail on a manifest with an unknown repo, a checkout whose HEAD differs from the pinned commit, a row naming a tool outside Julie's ten names or Miller's `search|inspect|context|trace|impact|patterns`, or an `expected.path` that does not exist in the repo. Write the validator first with this self-check block at the bottom of `run_matrix.py`:

```python
if __name__ == "__main__" and "--self-check" in sys.argv:
    bad = {"schema_version": 1, "repos": {}, "rows": [{"id": "x", "repo": "nope", "task_class": "retrieval.symbol",
           "julie": {"tool": "search", "args": {}}, "miller": {"tool": "search", "args": {}},
           "expected": {"path": "missing.rs", "anchor": ""}, "scoring": {"mode": "path_top"}}]}
    errors = validate_manifest(bad)
    assert any("unknown repo" in e for e in errors), errors
    assert any("julie tool" in e for e in errors), errors
    print("self-check ok")
    sys.exit(0)
```

**Step 2: Run test to verify it fails**

Run: `python3 docs/eval/head-to-head/run_matrix.py --self-check`
Expected: fails (`validate_manifest` not defined).

**Step 3: Write minimal implementation**

1. Copy `~/source/miller/scripts/benchlib/mcp_client.py` to `docs/eval/head-to-head/mcp_client.py` unchanged except the module docstring.
2. `run_matrix.py`: a trimmed port of `bench-foundation-matrix.py` with these changes: binaries from `--julie-bin` (default `target/release/julie-server`) and `--miller-bin` (default `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller`); repos and pinned commits from the `repos` object in `cases.json` (`--validate` checks paths exist and `git rev-parse HEAD` matches); Julie spawned as `[julie_bin]` with `cwd=repo_path` (the shim binds the working directory); Miller spawned as `[miller_bin, "serve"]` with every Miller row passing `workspace_id` from `workspace operation=open path=<repo>` once per repo; `SUPPORTED_JULIE_TOOLS = {"fast_search","get_symbols","deep_dive","fast_refs","call_path","blast_radius","get_context","patterns"}`; scoring modes `path_top` and `path_top5`; per row record latency and response bytes for both products; output JSON and Markdown to `docs/eval/head-to-head/results/<UTC timestamp>.{json,md}` with a per-task-class table and a per-tool latency table. Keep `--validate`, `--self-check`, `--skip-miller`, `--skip-julie`, `--repos`, `--tasks`.
3. `cases.json`: 46 rows. The 23 search rows come from `scorecard.toml` (`julie: fast_search query=<query> limit=5`, `miller: search query=<query> limit=5 format=json mode=auto`, `expected.path` = first `expected_any`, `scoring.mode = path_top5`, `task_class = retrieval.concept` or `retrieval.implementation` per the case's `category`). Add 23 more rows with `task_class = inspect.symbol`: before any matrix run, list the case's expected file with `julie-server symbols <expected path> --json` and take its first top-level symbol name as the target (record the name in the row's `intent`); `julie: deep_dive symbol=<Name> depth=overview`, `miller: inspect target=<Name> depth=summary format=json`, `scoring.mode = path_top`, expected path unchanged. This uses only the answer file, never candidate search output. Fill `repos` with `path` and `commit` from `git -C <path> rev-parse HEAD`.
4. `README.md`: how to run, how rows are scored, and the two statements from the frozen contract this run honors (cases were written before any candidate output was viewed; the manifest and results live under `docs/eval/`, which neither product indexes as a corpus because the corpus roots are the ten external repos).
5. Run `--validate`, then the full run twice, and keep the second pair (the first warms both indexes; say so in the finding). Total wall time under one hour.
6. `docs/findings/2026-09-1X-head-to-head-miller.md`: per task class top-1 and top-5 for both products, per tool p50 and p95 latency, response bytes, the rows where the products disagree with a one-line reason each (read the responses; do not guess), and an honest scope line: retrieval matrix only, one machine, fresh indexes, no agent episodes. No winner by construction: report the numbers and the trade-offs.

**Step 4: Run test to verify it passes**

Run: `python3 docs/eval/head-to-head/run_matrix.py --self-check`
Expected: `self-check ok`

Run: `python3 docs/eval/head-to-head/run_matrix.py --validate`
Expected: `46 rows valid`

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "docs(eval): head-to-head retrieval matrix against Miller on the fresh public corpus"` and record the SHA.

**Acceptance criteria:**
- [x] `--self-check` and `--validate` pass; both result files exist and the finding cites them by path.
- [x] Every Julie row calls one of the ten tools; no row calls a Miller `edit`, `content`, or `tests` tool.
- [x] The finding reports both products' numbers per task class and per tool, names disagreement rows with reasons, and states the scope limits.
- [x] Tests pass and the change is committed by the worker per commit mode.

---

## Task 7: Phase 6a gate finding

**Files:**
- Create: `docs/findings/2026-09-1X-machine-service-phase6a-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`
- Modify: `docs/plans/2026-09-09-machine-service-design.md` section 14 item 6 (add `*Landed (6a):*` with the commit range, and `*Deferred (6b):*` naming the renames, the folds, and the telemetry condition), `CLAUDE.md` and `AGENTS.md` "Last Updated" line and status
- Test: none new; the lead's gates

**Interfaces:**
- Consumes: the lead's gate results (`cargo xtask test dev`, `system`, `dogfood`, `full`, `fast` timing three times, `cargo test -p xtask --test docs_contract_tests`), `tokei` before and after, the head-to-head finding, and a `tool_calls` baseline.
- Produces: the gate finding and the ledger.

**Contract inputs:** `docs/findings/2026-09-10-machine-service-phase4-gate.md` is the shape to follow; `docs/plans/verification-ledger-template.md` is the ledger shape.

**File ownership:** Create `docs/findings/2026-09-1X-machine-service-phase6a-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`. Modify `docs/plans/2026-09-09-machine-service-design.md`, `CLAUDE.md`, `AGENTS.md`.

**Serialization required:** Yes

**Dependency reason:** Measures the finished branch.

**Step 1: Collect the evidence**

The worker reports `STATUS ready for gates`. The lead runs, at the Task 6 commit, in a clean detached checkout, and records each in the ledger: `cargo fmt --check`; `cargo clippy --workspace --all-targets`; `cargo xtask test dev`; `cargo xtask test system`; `cargo xtask test dogfood`; `cargo xtask test full`; `cargo xtask test fast` three times (median); `cargo test -p xtask --test docs_contract_tests`; `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` at `5eafea53` and at HEAD with the same command; `julie-server service status` after `manage_workspace open` on the julie and miller repos with semantics on (resident memory).

**Step 2: Record the telemetry baseline**

The worker runs, read-only, and pastes the table into the finding: `sqlite3 ~/.julie/registry.db "select julie_version, client, tool_name, count(*) calls, sum(success=0) errors, sum(success=1 and result_count=0) empty, round(avg(duration_ms)) ms, round(avg(output_bytes)) bytes from tool_calls where workspace_id not like 'ws__tmp%' and workspace_id not like 'tmp_%' and workspace_id not like 'target_%' group by 1,2,3 order by 4 desc"` with the date. This is the baseline phase 6b compares against after two weeks.

**Step 3: Write the finding**

Sections, in order: verdict; what 6a shipped (one line per task, including the telemetry columns); what 6b defers and the condition (two weeks of `tool_calls` from real sessions, then a decision on names and folds); gate table; net lines table; `fast` median and `full` wall; resident memory; head-to-head summary (numbers only, link to the Task 6 finding); telemetry baseline table; follow-ups (plugin repo skill list and hooks, phase 7 CT, Rust revival harness).

**Step 4: Update the design and the two instruction files**

Design section 14 item 6: `*Landed (6a):* commits <first>..<last> on branch contract: compact output and offset paging, instructions under 1,900 characters plus session hook, rewrite_symbol and rename_symbol deleted, blast_radius git-diff seeding, head-to-head retrieval matrix. *Deferred (6b):* Miller names, inspect and trace folds, telemetry rename; decision after two weeks of tool_calls data.` `CLAUDE.md` and `AGENTS.md`: "Last Updated: 2026-09-1X | Status: Phase 6a (compact output, paging, guidance hook)".

**Step 5: Verify and apply commit mode**

Run: `cargo test -p xtask --test docs_contract_tests 2>&1 | tail -5`
Expected: PASS.

- `serial-worker-commit`: `git commit -m "docs(contract): phase 6a gate finding and verification ledger"` and record the SHA.

**Acceptance criteria:**
- [x] The finding and ledger exist; every ledger row names a command, scope label, SHA, result, and timestamp.
- [x] Net lines are negative against `5eafea53`.
- [x] `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters and `fast` median is under 10 s.
- [x] The telemetry baseline table is in the finding with its date.
- [x] Tests pass and the change is committed by the worker per commit mode.

---

## Verification Ledger

See `docs/plans/2026-09-10-machine-service-phase6-ledger.md` (created in Task 7).

## Deferred to phase 6b (not scheduled)

Recorded here so the decision and its evidence are not lost. The earlier draft of this plan (commit `5eafea53`) carries the full task text for each item.

1. **Miller names** for the tools (`search`, `inspect`, `context`, `trace`, `impact`, `workspace`) and the `tool_calls` rename migration. No measured evidence that names matter; a Grok worker used Julie's names fluently on 2026-09-10.
2. **Folds** of `get_symbols` + `deep_dive` into `inspect` and `fast_refs` + `call_path` into `trace`. The owner's earlier measurement showed two focused calls cost fewer tokens than one kitchen-sink call, and Miller's `inspect depth=full` at 60% of calls points the same way.
3. **Miller's `edit` operations** and the deletion of `edit_file`. On 2026-09-10 a Grok worker used `edit_file` for every edit (dry run then apply, all successful) while Miller's `edit` had failed one call in eight in its own telemetry.
4. **Decision condition:** two weeks of `tool_calls` rows from real Julie sessions after 6a merges, compared against the Task 7 baseline.
