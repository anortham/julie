# Machine Service Phase 6: Contract and Guidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Replace Julie's twelve tool names with the seven-tool Miller contract (`search`, `inspect`, `context`, `trace`, `impact`, `patterns`, `workspace`), delete the edit tools, add stateless paging and compact output, rewrite the agent guidance, hooks, and skills for the new names, and run the head-to-head against Miller on the fresh public corpus.

**Architecture:** The public contract is the `register_tool_catalog!` table in `src/request_engine/catalog.rs`; every transport (rmcp stdio, Streamable HTTP, `/api/<tool>`, CLI `tool <name>`) decodes through it. Two Julie tools fold into one Miller tool twice: `get_symbols` + `deep_dive` become `inspect`, and `fast_refs` + `call_path` become `trace`. Each fold is a new parameter struct plus a router function that calls the existing execution code; no execution code moves. Edit tools, the edit journal, and edit recovery are deleted outright. Paging is an `offset` parameter that re-runs the same query against the same snapshot; there is no continuation store.

**Tech Stack:** Rust (`serde`, `schemars`, `rmcp` `#[tool]` wrappers), `rusqlite` registry migrations, Node hook script (port of Miller's), Python 3 head-to-head harness (port of Miller's `bench-foundation-matrix.py` and `benchlib/mcp_client.py`).

**Architecture Quality:** Approved shape per `docs/plans/2026-09-09-machine-service-design.md` section 9 with four deviations the owner decided on 2026-09-10: (1) no `edit` tool, agents use their harness editor; (2) no `content` tool, a `large-file` skill routes to `jq`, `rg`, `sed`, and `head`; (3) paging survives as an `offset` parameter with a `next:` trailer instead of Miller's byte caps and continuation tokens; (4) `inspect` keeps the structure/relationship split by defaulting to `depth=summary` and never returning a body unless `depth=full` is explicit, because measured token use showed two focused calls beat one kitchen-sink call. Main risk: the two folds (`inspect`, `trace`) change parameter shapes that 44 test files and every skill name, so Tasks 3 and 4 carry it early and each ends with the catalog contract test green.

## Global Constraints

- Design: `docs/plans/2026-09-09-machine-service-design.md` sections 9, 12, 14 item 6. Decision record: `docs/findings/2026-09-10-tool-contract-comparison.md`.
- Public tool names after this plan, exactly: `search`, `inspect`, `context`, `trace`, `impact`, `patterns`, `workspace`. `AVAILABLE_TOOLS` in `src/request_engine/catalog.rs` is the single source of truth; `src/cli_tools/generic.rs` `AVAILABLE_TOOLS` is deleted.
- No `edit`, `content`, or `tests` tool. Continuous testing is phase 7.
- Every workspace-bound tool keeps Julie's `workspace: Option<String>` parameter with default `"primary"`; the stdio shim binds `primary` to its working directory (`src/service/shim.rs` `bind_default_workspace`). Miller's required `workspace_id` is not adopted.
- Paging: tools that list rows take `offset: u32` (default 0). When rows remain, the last output line is exactly `next: <tool> <same args> offset=<kept+offset>`. No byte caps, no continuation tokens.
- Rust type names do not change (`FastSearchParams`, `GetSymbolsTool`, `DeepDiveTool`, `FastRefsTool`, `CallPathTool`, `BlastRadiusTool`, `GetContextTool`, `PatternsTool`, `ManageWorkspaceTool`). Only public names, files under `src/handler/tools/`, and CLI names change. New structs: `InspectTool`, `TraceTool`, `ImpactTool`.
- `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters (Claude Code truncates merged server instructions near 2 KB; Miller's ceiling).
- `registry.db` migration 008 renames historical `tool_calls.tool_name` values to the new names. No dual-read code.
- Telemetry keeps logging the full parameter object and `target` in `tool_calls.metadata` (`src/handler/tool_targets.rs`).
- Head-to-head corpus: the ten public repos in `docs/eval/semantic-value/scorecard.toml` (express, flask, gson, moshi, Alamofire, cobra, sinatra, nlohmann-json, Newtonsoft.Json, jq) at the commits recorded in the manifest. Never index the manifest, expected answers, or results.
- Net lines: this phase must be negative against `main` at its start (`tokei` before and after, recorded in the gate finding).
- Commit messages: conventional commits, one commit per task, on branch `contract` in `/home/murphy/source/julie/.worktrees/contract`.
- No pushes, no releases, no `.gitignore` or `.git/info/exclude` edits, no tool caches committed, no edits to `~/source/miller` or `~/source/julie-plugin`.
- File size: no line limit. Split only by responsibility.
- Test rules from `CLAUDE.md`: workers run exact tests only, at most two runs per change. The lead runs `cargo xtask test changed` and `cargo xtask test dev`.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` sections "RUNNING TESTS" and "Canonical Test Tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name>` for top-crate tests; `cargo nextest run -p julie-tools <exact_test_name>` for `crates/julie-tools`; `cargo test -p xtask --test docs_contract_tests <exact_test_name>` for the docs contract.

**Worker ceiling:** the exact test names written in each task. Workers never run `cargo xtask test …` or an unfiltered `cargo nextest run`.

**Worker gate invariant:** each task lists its invariant under **Acceptance criteria**. The invariant shared by Tasks 1 through 6 is `catalog_lists_exactly_the_seven_contract_tools` in `src/tests/request_engine.rs`.

**Lead affected-change scope:** `cargo xtask test changed` after each task lands; `cargo xtask test bucket cli` and `cargo xtask test bucket service` after Tasks 2, 3, 4; `cargo test -p xtask --test docs_contract_tests` after Task 7.

**Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test dogfood` (search rendering changes), `cargo xtask test full`. Then the phase 6 gate finding (Task 9).

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: every test named in this plan passes; `fast` median under 10 s; trimmed `full` median under 120 s; net lines negative; `JULIE_AGENT_INSTRUCTIONS.md` at most 1,900 characters. Report-only: head-to-head top-5 per task class, latency per tool, resident memory.

**Escalation triggers:** any change to `src/service/`, `src/request_engine/`, or `src/handler.rs` runs `cargo xtask test system`. Any change under `crates/julie-tools/src/search/` runs `cargo xtask test dogfood`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-10-machine-service-phase6-ledger.md` using `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only when scope label and HEAD match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Delete the edit tools | None - serial | Delete `crates/julie-tools/src/editing/`, `crates/julie-tools/src/refactoring/`, `src/handler/tools/{edit_file,rewrite_symbol,rename_symbol}.rs`, `src/tools/workspace/commands/recover_edit.rs`, `src/cli_tools/recover_edit.rs`, `src/workspace_runtime/{edit_journal,source_edit,source_edit_ops}.rs`, `src/tests/edit_recovery_contract.rs`, `fixtures/editing/`, `.claude/skills/editing/`, edit tests listed in the task. Modify `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/tools/mod.rs`, `crates/julie-tools/src/lib.rs`, `src/cli.rs`, `src/cli_tools/{mod,commands,subcommands,generic}.rs`, `src/tools/workspace/commands/mod.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/dashboard/routes/metrics.rs`, `src/workspace_runtime/mod.rs`, `src/tests/request_engine.rs`. | Yes | Clears the surface before renames. |
| Task 2: Rename the one-to-one tools | None - serial | Rename `src/handler/tools/{fast_search→search,get_context→context,blast_radius→impact,manage_workspace→workspace}.rs`. Modify `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/search_telemetry.rs`, `src/main.rs`, `src/cli.rs`, `src/cli_tools/{mod,commands,subcommands,catalog}.rs`, delete `src/cli_tools/generic.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/registry/database/migrations.rs`, `xtask-eval/src/search_matrix_mine.rs`, `xtask/src/changed/mapping/{front,crates,product}.rs`, `xtask/src/changed/policy.rs`, `xtask/src/changed/mapping.rs`, `crates/julie-tools/src/symbols/mod.rs`, `crates/julie-tools/src/search/tool_execution.rs`, tests listed in the task. | Yes | Needs Task 1's smaller catalog. |
| Task 3: `inspect` | None - serial | Create `crates/julie-tools/src/inspect.rs`, `src/handler/tools/inspect.rs`, `crates/julie-tools/src/tests/inspect_tests.rs`. Delete `src/handler/tools/{get_symbols,deep_dive}.rs`. Modify `crates/julie-tools/src/lib.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands}.rs`, `src/tools/metrics/session.rs`, tests listed in the task. | Yes | Needs Task 2's names. |
| Task 4: `trace` | None - serial | Create `crates/julie-tools/src/navigation/trace.rs`, `src/handler/tools/trace.rs`, `crates/julie-tools/src/tests/trace_tests.rs`. Delete `src/handler/tools/{fast_refs,call_path}.rs`. Modify `crates/julie-tools/src/navigation/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands}.rs`, `src/tools/metrics/session.rs`, tests listed in the task. | Yes | Needs Task 3's catalog. |
| Task 5: `search` shape, compact output, paging | None - serial | Modify `crates/julie-tools/src/search/{params,tool_execution,formatting}.rs`, `crates/julie-tools/src/shared.rs`, `crates/julie-tools/src/impact/formatting.rs`, `crates/julie-tools/src/get_context/formatting.rs`, `crates/julie-tools/src/navigation/{trace,formatting}.rs`, `crates/julie-tools/src/inspect.rs`, `crates/julie-tools/src/symbols/formatting.rs`, `src/request_engine/catalog.rs`, `src/handler/tools/search.rs`, `src/handler/search_telemetry.rs`, `src/cli_tools/subcommands.rs`, `docs/eval/semantic-value/run_scorecard.py`, `docs/eval/semantic-value/scorecard.toml`, tests listed in the task. | Yes | Needs Tasks 3 and 4's structs for the shared trailer. |
| Task 6: `impact` shape | None - serial | Create `crates/julie-tools/src/impact/params.rs`, `crates/julie-tools/src/tests/impact_params_tests.rs`. Modify `crates/julie-tools/src/impact/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler/tools/impact.rs`, `src/handler/tool_targets.rs`, `src/cli_tools/{commands,subcommands}.rs`, tests listed in the task. | Yes | Needs Task 5's trailer helper. |
| Task 7: Guidance, hooks, skills, docs | None - serial | Modify `JULIE_AGENT_INSTRUCTIONS.md`, `README.md`, `docs/site/index.html`, `docs/site/script.js`, `.claude/hooks/hooks.json`, `.claude/skills/{dead-code-audit,explore-area,impact-analysis,search-debug,web-research}/SKILL.md`, `xtask/tests/docs_contract_tests.rs`, `src/tests/core/workspace_init/instructions_paths.rs`, `CLAUDE.md`, `AGENTS.md`. Create `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`, `.claude/skills/large-file/SKILL.md`. Delete `.claude/hooks/pretool-edit.cjs`. | Yes | Names must be final. |
| Task 8: Head-to-head run | None - serial | Create `docs/eval/head-to-head/{run_matrix.py,mcp_client.py,cases.json,README.md}`, `docs/eval/head-to-head/results/<timestamp>.{json,md}`, `docs/findings/2026-09-1X-head-to-head-miller.md`. | Yes | Needs the finished contract. |
| Task 9: Phase 6 gate finding | None - serial | Create `docs/findings/2026-09-1X-machine-service-phase6-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`. Modify `docs/plans/2026-09-09-machine-service-design.md` (phase 6 note), `CLAUDE.md`, `AGENTS.md` (tool list and CLI names). | Yes | Measures the finished branch. |

Commit mode for every task: `serial-worker-commit`.

---

## Task 1: Delete the edit tools

**Files:**
- Delete: `crates/julie-tools/src/editing/` (7 files), `crates/julie-tools/src/refactoring/` (4 files), `src/handler/tools/edit_file.rs`, `src/handler/tools/rewrite_symbol.rs`, `src/handler/tools/rename_symbol.rs`, `src/tools/workspace/commands/recover_edit.rs`, `src/cli_tools/recover_edit.rs`, `src/workspace_runtime/edit_journal.rs`, `src/workspace_runtime/source_edit.rs`, `src/workspace_runtime/source_edit_ops.rs`, `src/tests/edit_recovery_contract.rs`, `src/tests/core/handler/editing_metrics.rs`, `crates/julie-tools/src/tests/extractor_migration_editing.rs`, `crates/julie-tools/src/tests/refactoring_ast_aware.rs`, `fixtures/editing/`, `.claude/skills/editing/`
- Modify: `src/request_engine/catalog.rs:149-157,225-243` (three rows), `src/request_engine/dispatch.rs:219-227,246-262` (three arms), `src/handler.rs:1346-1359` (router composition), `src/handler.rs:262-270` (`request_targets_primary_workspace`), `src/handler.rs:1401-1420` (`is_write_exempt`), `src/handler/tools/mod.rs:13,21,22`, `src/handler/tool_targets.rs:124-160` (three metadata builders), `src/tools/mod.rs:15,20,31`, `crates/julie-tools/src/lib.rs:9,14,22,27`, `src/cli.rs:60-67` (three subcommands), `src/cli_tools/mod.rs`, `src/cli_tools/commands.rs:393-460`, `src/cli_tools/subcommands.rs` (`EditArgs`, `RenameArgs`, `RewriteArgs`), `src/cli_tools/generic.rs` (three arms and three names), `src/tools/workspace/commands/mod.rs:15,48-49,135,193-201,211,240-247,258-262,325-328` (`recover_edit` operation, `edit_id`, `recovery_action`), `src/tools/metrics/session.rs:40-78` (`RenameSymbol`, `EditFile`, `RewriteSymbol`, dead `QueryMetrics`; `COUNT` becomes 8), `src/dashboard/search_analysis.rs:8-17` (drop three names), `src/dashboard/routes/metrics.rs:118-120` (`is_edit_tool` returns false; delete it and its callers), `src/workspace_runtime/mod.rs` (module declarations), `src/tests/request_engine.rs` (catalog count test)
- Test: `src/tests/request_engine.rs`

**Interfaces:**
- Consumes: `register_tool_catalog!` rows in `src/request_engine/catalog.rs`; `AccessClass` enum (`Preview` and `SourceEdit` variants become unused).
- Produces: a nine-tool catalog (`blast_radius`, `call_path`, `deep_dive`, `fast_refs`, `fast_search`, `get_context`, `get_symbols`, `manage_workspace`, `patterns`) and no `AccessClass::Preview` or `AccessClass::SourceEdit` callers. Later tasks rely on `AVAILABLE_TOOLS` containing exactly those nine names.

**Contract inputs:** the current catalog table (`src/request_engine/catalog.rs:121-243`); `ManageWorkspaceOperation::parse` in `src/tools/workspace/commands/mod.rs:40-60`.

**File ownership:** Delete `crates/julie-tools/src/editing/`, `crates/julie-tools/src/refactoring/`, `src/handler/tools/{edit_file,rewrite_symbol,rename_symbol}.rs`, `src/tools/workspace/commands/recover_edit.rs`, `src/cli_tools/recover_edit.rs`, `src/workspace_runtime/{edit_journal,source_edit,source_edit_ops}.rs`, `src/tests/edit_recovery_contract.rs`, `fixtures/editing/`, `.claude/skills/editing/`, edit tests listed in the task. Modify `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/tools/mod.rs`, `crates/julie-tools/src/lib.rs`, `src/cli.rs`, `src/cli_tools/{mod,commands,subcommands,generic}.rs`, `src/tools/workspace/commands/mod.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/dashboard/routes/metrics.rs`, `src/workspace_runtime/mod.rs`, `src/tests/request_engine.rs`.

**Serialization required:** Yes

**Dependency reason:** Clears the surface before renames.

**Step 1: Write the failing test**

Replace the existing catalog count test in `src/tests/request_engine.rs` (the one the phase 4 plan called `catalog_schemas_valid_and_match_all_13_tools`, since renamed for twelve) with:

```rust
#[test]
fn catalog_lists_exactly_the_seven_contract_tools() {
    use crate::request_engine::catalog::{AVAILABLE_TOOLS, ToolCatalog};

    let expected = [
        "context", "impact", "inspect", "patterns", "search", "trace", "workspace",
    ];
    assert_eq!(AVAILABLE_TOOLS, &expected);
    let listed: Vec<&str> = ToolCatalog::list().iter().map(|t| t.name).collect();
    assert_eq!(listed, expected);
    for name in expected {
        let schema = ToolCatalog::schema(name).unwrap();
        assert!(schema.get("properties").is_some(), "{name} schema has properties");
    }
}
```

This test stays red until Task 4 finishes; Task 1 makes it fail for the right reason (nine names listed, no edit names). Add a second test that Task 1 turns green:

```rust
#[test]
fn catalog_has_no_edit_tools() {
    use crate::request_engine::catalog::{AVAILABLE_TOOLS, ToolCatalog};

    for name in ["edit_file", "rewrite_symbol", "rename_symbol"] {
        assert!(!AVAILABLE_TOOLS.contains(&name), "{name} still registered");
        assert!(ToolCatalog::schema(name).is_err(), "{name} still has a schema");
    }
    assert!(
        ToolCatalog::decode("manage_workspace", serde_json::from_value(serde_json::json!({
            "operation": "recover_edit", "edit_id": "x", "recovery_action": "keep"
        })).unwrap()).is_ok(),
        "decode still accepts the shape; execution must reject the operation"
    );
}
```

Then in `src/tests/tools/workspace/manage_workspace_request.rs` add:

```rust
#[test]
fn manage_workspace_rejects_recover_edit_operation() {
    let parsed = crate::tools::workspace::commands::ManageWorkspaceOperation::parse("recover_edit");
    assert!(parsed.is_err(), "recover_edit must no longer parse");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run --lib catalog_has_no_edit_tools 2>&1 | tail -10`
Expected: FAIL with `edit_file still registered`.

Run: `cargo nextest run --lib manage_workspace_rejects_recover_edit_operation 2>&1 | tail -10`
Expected: FAIL (`recover_edit` parses today, `src/tools/workspace/commands/mod.rs:48`).

**Step 3: Write minimal implementation**

1. Delete every file in the **Delete** list with `git rm -r`.
2. `src/request_engine/catalog.rs`: delete the `"edit_file"`, `"rename_symbol"`, and `"rewrite_symbol"` rows.
3. `src/request_engine/dispatch.rs:213-262`: delete the `EditFile`, `RenameSymbol`, and `RewriteSymbol` arms.
4. `src/handler.rs:1346-1359`: delete `+ Self::tool_router_rename_symbol()`, `+ Self::tool_router_edit_file()`, `+ Self::tool_router_rewrite_symbol()`.
5. `src/handler.rs:262-270`: the match becomes

```rust
        match tool_name {
            "fast_search" | "fast_refs" | "call_path" | "get_symbols" | "deep_dive"
            | "get_context" | "blast_radius" => workspace_is_primary,
            "manage_workspace" => Self::manage_workspace_request_targets_primary(arguments),
            _ => false,
        }
```

6. `src/handler.rs:1401-1420`: delete the `matches!(tool_name, "edit_file" | "rename_symbol" | "rewrite_symbol")` early return. Keep the `manage_workspace` mutation branch.
7. `src/handler/tools/mod.rs`: delete `pub(crate) mod edit_file;`, `pub(crate) mod rename_symbol;`, `pub(crate) mod rewrite_symbol;`.
8. `src/handler/tool_targets.rs`: delete `rename_symbol_metadata`, `edit_file_metadata`, `rewrite_symbol_metadata` and the `"kind": "rename_symbol"` literal at `:126`.
9. `src/tools/mod.rs`: delete `pub use julie_tools::editing;`, `pub use julie_tools::refactoring;`, `pub use refactoring::RenameSymbolTool;`. `crates/julie-tools/src/lib.rs`: delete `pub mod editing;`, `pub mod refactoring;`, `pub use editing::EditingTransaction;`, `pub use refactoring::RenameSymbolTool;`. Fix every `use` that breaks (`cargo check` lists them; each is a dead import).
10. `src/cli.rs:60-67`: delete the `edit`, `rename`, `rewrite` subcommands and their aliases. `src/cli_tools/subcommands.rs`: delete `EditArgs`, `RenameArgs`, `RewriteArgs`. `src/cli_tools/commands.rs:393-460`: delete the three `CliToolCommand` impls. `src/cli_tools/mod.rs`: delete the `recover_edit` module and the `edit` re-exports. `src/cli_tools/generic.rs`: delete the three arms and the three names from `AVAILABLE_TOOLS` (the whole file goes in Task 2).
11. `src/tools/workspace/commands/mod.rs`: delete `pub(crate) mod recover_edit;` (`:15`), the two `("recover_edit", …)` / `("recover-edit", …)` parse rows (`:48-49`), the `RecoverEdit` variant (`:135`), the `edit_id` extraction block (`:193-201`), the `recover_edit` words in the `operation` doc (`:211`), the `name` field with its `edit_id` alias (`:240-242`), the `recovery_action` alias on `workspace_id` (`:245`), the `edit_id()` and `recovery_action()` accessors (`:258-262`), and the `handle_recover_edit_command` call (`:325-328`).
12. `src/workspace_runtime/mod.rs`: delete the `edit_journal`, `source_edit`, `source_edit_ops` module declarations and every `pub use` of their items. `cargo check` shows the remaining callers; delete each caller (they are all edit paths).
13. `src/tools/metrics/session.rs:40-78`: delete `RenameSymbol`, `QueryMetrics`, `EditFile`, `RewriteSymbol` variants and their `from_name`/`name` arms; renumber the enum so it is contiguous; set `COUNT` to the variant count (8). Fix `per_tool: [ToolCounters; ToolKind::COUNT]` consumers if any index by literal.
14. `src/dashboard/search_analysis.rs:8-17`: `USEFUL_ACTIONS` becomes `["deep_dive", "get_symbols", "fast_refs", "call_path", "get_context"]`. `src/dashboard/routes/metrics.rs:118-120`: delete `is_edit_tool` and the fields it feeds (`cargo check` names them).
15. `src/tests/request_engine.rs`: replace the twelve-tool count test with the two tests above. Delete tests that only exercise edit tools: `src/tests/core/handler/editing_metrics.rs`, `src/tests/edit_recovery_contract.rs`, `crates/julie-tools/src/tests/extractor_migration_editing.rs`, `crates/julie-tools/src/tests/refactoring_ast_aware.rs`, and the edit sections of `src/tests/cli_execution_tests.rs`, `src/tests/cli_tools_tests.rs`, `src/tests/cli_input_contract.rs`, `src/tests/request_scenarios.rs`, `src/tests/mcp_protocol_contract.rs`, `src/tests/request_transport_parity.rs`, `src/tests/core/handler/{metrics_recording,public_surface,workspace_binding_metrics}.rs`, `src/tests/dashboard/{search_analysis,state}.rs`, `src/tests/dashboard/integration/metrics.rs`, `src/tests/tools/metrics/session_metrics_tests.rs`, `src/tests/registry/database.rs`. Delete the whole test function when its subject is an edit tool; edit the assertion when the test counts tools. Remove the `tests-editing` style buckets from `xtask/test_tiers.toml` and their mappings under `xtask/src/changed/` if they point at deleted files (`cargo xtask test list` must not name a bucket with zero tests).
16. Update the module declarations in `src/tests/mod.rs`, `src/tests/core/handler/mod.rs`, and `crates/julie-tools/src/tests/mod.rs` for the deleted files.
17. `cargo check --workspace --all-targets` until clean. Every error is a dead edit path; delete, do not stub.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run --lib catalog_has_no_edit_tools 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run --lib manage_workspace_rejects_recover_edit_operation 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "refactor(tools): delete the edit tools, edit journal, and edit recovery"` and record the SHA.

**Acceptance criteria:**
- [ ] `catalog_has_no_edit_tools` and `manage_workspace_rejects_recover_edit_operation` pass.
- [ ] `cargo check --workspace --all-targets` is clean; `grep -rn "edit_file\|rewrite_symbol\|rename_symbol\|recover_edit\|EditingTransaction" src crates xtask xtask-eval --include='*.rs'` returns nothing.
- [ ] `tokei` line count is lower than at the branch start (record both numbers in the commit body).
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 2: Rename the one-to-one tools

**Files:**
- Rename (`git mv`): `src/handler/tools/fast_search.rs` → `search.rs`, `get_context.rs` → `context.rs`, `blast_radius.rs` → `impact.rs`, `manage_workspace.rs` → `workspace.rs`
- Delete: `src/cli_tools/generic.rs`
- Modify: `src/request_engine/catalog.rs` (four rows), `src/request_engine/dispatch.rs`, `src/handler.rs:262-270,1346-1359,1401-1420`, `src/handler/tools/mod.rs`, `src/handler/search_telemetry.rs`, `src/main.rs:253`, `src/cli.rs:43-88`, `src/cli_tools/mod.rs`, `src/cli_tools/commands.rs:94,124,158,183,203,235,263,282,306`, `src/cli_tools/subcommands.rs:310-360` (`GenericToolArgs`), `src/cli_tools/catalog.rs:10,31`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs:8-17,64`, `src/registry/database/migrations.rs`, `xtask-eval/src/search_matrix_mine.rs:55`, `xtask/src/changed/mapping/front.rs:117`, `xtask/src/changed/mapping/crates.rs:104-107`, `xtask/src/changed/mapping/product.rs:217-222`, `xtask/src/changed/policy.rs:211`, `xtask/src/changed/mapping.rs:146`, `crates/julie-tools/src/symbols/mod.rs:59,68`, `crates/julie-tools/src/search/tool_execution.rs:19-20`
- Test: `src/tests/request_engine.rs`, `src/tests/registry/database.rs`

**Interfaces:**
- Consumes: Task 1's nine-name catalog.
- Produces: catalog names `search`, `context`, `impact`, `workspace`, `patterns` plus the four still-to-fold names (`get_symbols`, `deep_dive`, `fast_refs`, `call_path`). `julie-server tool <name>` decodes through `ToolCatalog::decode` and dispatches through `RequestEngine`. Migration 008 exists. Later tasks rely on `src/handler/tools/{search,context,impact,workspace,patterns}.rs` existing with `tool_router_<name>`.

**Contract inputs:** `register_tool_catalog!` rows; `ToolCatalog::decode(name, arguments)`; `RequestEngine::dispatch`; migration runner shape in `src/registry/database/migrations.rs:1-40`.

**File ownership:** Rename `src/handler/tools/{fast_search→search,get_context→context,blast_radius→impact,manage_workspace→workspace}.rs`. Modify `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/search_telemetry.rs`, `src/main.rs`, `src/cli.rs`, `src/cli_tools/{mod,commands,subcommands,catalog}.rs`, delete `src/cli_tools/generic.rs`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs`, `src/registry/database/migrations.rs`, `xtask-eval/src/search_matrix_mine.rs`, `xtask/src/changed/mapping/{front,crates,product}.rs`, `xtask/src/changed/policy.rs`, `xtask/src/changed/mapping.rs`, `crates/julie-tools/src/symbols/mod.rs`, `crates/julie-tools/src/search/tool_execution.rs`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 1's smaller catalog.

**Step 1: Write the failing test**

In `src/tests/request_engine.rs`:

```rust
#[test]
fn catalog_uses_contract_names_for_one_to_one_tools() {
    use crate::request_engine::catalog::AVAILABLE_TOOLS;

    for name in ["search", "context", "impact", "workspace", "patterns"] {
        assert!(AVAILABLE_TOOLS.contains(&name), "{name} missing");
    }
    for old in ["fast_search", "get_context", "blast_radius", "manage_workspace"] {
        assert!(!AVAILABLE_TOOLS.contains(&old), "{old} still registered");
    }
}
```

In `src/tests/registry/database.rs`:

```rust
#[test]
fn migration_008_renames_historical_tool_call_names() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("registry.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        crate::registry::database::migrations::run_migrations_up_to(&conn, 7).unwrap();
        conn.execute(
            "INSERT INTO tool_calls (workspace_id, session_id, timestamp, tool_name, duration_ms)
             VALUES ('w', 's', 1, 'fast_search', 1.0), ('w', 's', 2, 'blast_radius', 1.0),
                    ('w', 's', 3, 'get_symbols', 1.0), ('w', 's', 4, 'call_path', 1.0)",
            [],
        )
        .unwrap();
    }
    let db = crate::registry::database::RegistryDatabase::open(&path).unwrap();
    let names: Vec<String> = db
        .connection()
        .prepare("SELECT tool_name FROM tool_calls ORDER BY timestamp")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(names, ["search", "impact", "inspect", "trace"]);
}
```

`run_migrations_up_to(conn, n)` is new: it is `run_migrations` with an upper bound, and `run_migrations` calls it with `i32::MAX`. If `RegistryDatabase` exposes its connection under another name, use that name; do not add a new accessor for the test.

**Step 2: Run test to verify it fails**

Run: `cargo nextest run --lib catalog_uses_contract_names_for_one_to_one_tools 2>&1 | tail -10`
Expected: FAIL with `search missing`.

Run: `cargo nextest run --lib migration_008_renames_historical_tool_call_names 2>&1 | tail -10`
Expected: FAIL to compile (`run_migrations_up_to` missing).

**Step 3: Write minimal implementation**

1. `git mv` the four handler files. Inside each: change `#[tool(name = "…")]`, the `async fn` name, `tool_router = tool_router_<new>`, and the string literal passed to `classify_tool_failure` and `record_tool_*`. Keep the `execute_*` method names as they are (Rust names do not change).
2. `src/handler/tools/mod.rs`: rename the four `pub(crate) mod` lines. `src/handler.rs:1346-1359`: the composition becomes `tool_router_search() + tool_router_fast_refs() + tool_router_call_path() + tool_router_get_symbols() + tool_router_deep_dive() + tool_router_context() + tool_router_impact() + tool_router_workspace() + tool_router_patterns()`.
3. `src/request_engine/catalog.rs`: rename the four row keys. Set the descriptions verbatim (these are the agent-facing descriptions; Task 7 does not revisit them):
   - `"search"`: `"Search indexed code and return ranked hits. Pass a symbol name, identifier, path fragment, or natural-language phrase. mode=symbol for definitions only, mode=file for paths, mode=text for line matches; auto mixes them. retrieval=lexical does zero vector work. Scope with file_pattern, language, limit. Page with offset. NOT for a symbol you can already name (inspect) or its references (trace)."`
   - `"context"`: `"First call in an unfamiliar area: give a task, plus optional entry_symbols, edited_files, failing_test, or stack_trace. Returns ranked pivots with bounded snippets and neighbour signatures within max_tokens. NOT for a symbol you can already name (inspect) or text lookups (search)."`
   - `"impact"`: `"Blast radius: what a change affects and which tests to run. With no arguments it reads the working-tree git diff. Or pass one of target (a symbol name), changed_paths, or git=true. Use before a refactor and after edits. NOT for plain reference lists (trace)."`
   - `"workspace"`: `"Manage the workspace index: status (default), list, open, remove, refresh, rebuild, health, index, dashboard. Use when results look stale or before cross-workspace calls. NOT for reading code."`
   - `"patterns"` keeps its current description.
   Rows keep the same `variant`, `type`, `access`, `workspace`, `unbound`, `semantics` closures. Keep the rows sorted by name.
4. `src/request_engine/dispatch.rs`: no arm names change (variants stay `FastSearch`, `GetContext`, `BlastRadius`, `ManageWorkspace`). Only `manage_workspace` special-cases at `:74-89` compare a string; change those to `"workspace"`.
5. `src/handler.rs:262-270`: `"search" | "fast_refs" | "call_path" | "get_symbols" | "deep_dive" | "context" | "impact" => workspace_is_primary`, `"workspace" => …`. `src/handler.rs:1408`: `if tool_name == "workspace"`. `src/main.rs:253`: `tool_name == "workspace"`.
6. `src/cli.rs`: `blast-radius` becomes `impact`; `search`, `context`, `patterns`, `workspace` already match. Keep `refs`, `symbols`, `call-path`, `deep-dive` for Tasks 3 and 4. `src/cli_tools/commands.rs`: the `tool_name()` literals at `:94`, `:203`, `:263`, `:282` become `"search"`, `"context"`, `"impact"`, `"workspace"`; `:306` error text says `workspace`. `src/cli_tools/catalog.rs:10,31`: replace `13` with the live count from `AVAILABLE_TOOLS.len()`.
7. Delete `src/cli_tools/generic.rs`. The `julie-server tool <name> --params` path (`src/cli_tools/mod.rs`, `GenericToolArgs` in `subcommands.rs`) calls `ToolCatalog::decode(name, params_object)` and then `RequestEngine::dispatch` the same way `src/service/http.rs` `api_call` does. Copy the pattern from `api_call`; do not keep a second name table. Move the inline tests from `generic.rs:130-350` that still apply into `src/tests/cli_tools_tests.rs` with the new names; delete the rest.
8. `src/tools/metrics/session.rs`: variants `Search`, `Context`, `Impact`, `Workspace` with names `"search"`, `"context"`, `"impact"`, `"workspace"`; keep the four fold names for now.
9. `src/dashboard/search_analysis.rs:64` and `xtask-eval/src/search_matrix_mine.rs:55`: `== "search"`. `src/handler/search_telemetry.rs`: any `"fast_search"` literal becomes `"search"`.
10. `crates/julie-tools/src/symbols/mod.rs:59,68` error hints: `inspect(target=…)` and `search(query=…)`. `crates/julie-tools/src/search/tool_execution.rs:19-20`: `Run workspace(operation="index") first.`
11. `xtask/src/changed/`: rename bucket `tools-blast-spillover` to `tools-impact` in `policy.rs:211`, `mapping.rs:146`, `mapping/crates.rs:104-107`, `mapping/product.rs:217-222`, `mapping/front.rs:117-118`; delete the `spillover_get.rs`, `spillover/`, and `spillover_tests.rs` path arms; map `src/handler/tools/impact.rs` to `tools-impact`. Rename the bucket in `xtask/test_tiers.toml` too.
12. Migration 008 in `src/registry/database/migrations.rs`:

```rust
fn migration_008_rename_tool_calls_to_contract_names(conn: &mut Connection) -> Result<()> {
    info!("registry.db migration 008: rename tool_calls.tool_name to the seven-tool contract");
    let tx = conn.transaction()?;
    for (old, new) in [
        ("fast_search", "search"),
        ("get_context", "context"),
        ("blast_radius", "impact"),
        ("manage_workspace", "workspace"),
        ("get_symbols", "inspect"),
        ("deep_dive", "inspect"),
        ("fast_refs", "trace"),
        ("call_path", "trace"),
    ] {
        tx.execute(
            "UPDATE tool_calls SET tool_name = ?1 WHERE tool_name = ?2",
            params![new, old],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO schema_version (version, applied_at) VALUES (8, strftime('%s','now'))",
        [],
    )?;
    tx.commit()?;
    Ok(())
}
```

   Register it as `if current < 8 { migration_008_rename_tool_calls_to_contract_names(conn)?; }`. Add `run_migrations_up_to(conn, max_version)` and make `run_migrations` call it with `i32::MAX`. Edit tool names (`edit_file`, `rewrite_symbol`, `rename_symbol`) are left as they are in history; they are no longer a tool and the dashboard ignores unknown names.
13. Tests: update every string literal in the files from the Task 1 list that names one of the four renamed tools (`grep -rln '"fast_search"\|"get_context"\|"blast_radius"\|"manage_workspace"' src crates xtask xtask-eval --include='*.rs'`), plus `/api/manage_workspace` in `src/tests/service/durable_roots.rs:38`, `src/tests/service/control.rs:55`, `src/tests/service/http_api.rs`, `src/tests/service/mcp_http.rs`, `src/tests/service/shim.rs`, and the fixture files `fixtures/search-quality/zero-hit-replay-task3.json:40` and `zero-hit-replay-task3-results.json:510`.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run --lib catalog_uses_contract_names_for_one_to_one_tools 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run --lib migration_008_renames_historical_tool_call_names 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "refactor(tools): rename search, context, impact, and workspace to the contract names"` and record the SHA.

**Acceptance criteria:**
- [ ] Both tests pass; `julie-server tool search --params '{"query":"x"}' --json` and `julie-server tool fast_search …` behave as: the first runs, the second returns the unknown-tool error naming the live `AVAILABLE_TOOLS`.
- [ ] `src/cli_tools/generic.rs` is gone and `grep -rn "AVAILABLE_TOOLS" src` finds only `src/request_engine/catalog.rs` and its users.
- [ ] `grep -rn "spillover" src crates xtask xtask-eval` returns nothing.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 3: `inspect`

**Files:**
- Create: `crates/julie-tools/src/inspect.rs`, `src/handler/tools/inspect.rs`, `crates/julie-tools/src/tests/inspect_tests.rs`
- Delete: `src/handler/tools/get_symbols.rs`, `src/handler/tools/deep_dive.rs`
- Modify: `crates/julie-tools/src/lib.rs` (add `pub mod inspect; pub use inspect::InspectTool;`), `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs` (re-export), `src/request_engine/catalog.rs` (replace the `get_symbols` and `deep_dive` rows with one `inspect` row), `src/request_engine/dispatch.rs`, `src/handler.rs:262-270,1346-1359`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs:56-75` (one `inspect_metadata` replaces `get_symbols_metadata` and `deep_dive_metadata`), `src/cli.rs:47,57` (`symbols` and `deep-dive` become one `inspect` subcommand), `src/cli_tools/subcommands.rs` (`InspectArgs` replaces `SymbolsArgs` and `DeepDiveArgs`), `src/cli_tools/commands.rs:183,367`, `src/tools/metrics/session.rs` (`Inspect` replaces `GetSymbols` and `DeepDive`), `src/dashboard/search_analysis.rs:8-17`
- Test: `crates/julie-tools/src/tests/inspect_tests.rs`, `src/tests/request_engine.rs`

**Interfaces:**
- Consumes: `GetSymbolsTool` (`crates/julie-tools/src/symbols/mod.rs:80`, fields `file_path`, `max_depth`, `target`, `limit`, `mode`, `workspace`) and its `call_tool`; `DeepDiveTool` (`crates/julie-tools/src/deep_dive/mod.rs:40`, fields `symbol`, `depth: DeepDiveDepth {Overview, Context, Full}`, `context_file`, `workspace`, `semantics`) and its `call_tool`; `handler.execute_get_symbols` and `handler.execute_deep_dive` (`src/handler/tools/{get_symbols,deep_dive}.rs:36`).
- Produces: `julie_tools::inspect::InspectTool { target, depth: InspectDepth, scope, limit, offset, workspace, semantics }` with `InspectTool::call_tool(&self, handler)`; `handler.execute_inspect(params)`; catalog row `"inspect"`; CLI `julie-server inspect <target> [--depth …] [--scope …]`. Task 5 adds the paging trailer to the file listing.

**Contract inputs:** the two consumed structs above; `SemanticRequirement::Symbols` for symbol targets (deep dive semantics), `None` for file targets.

**File ownership:** Create `crates/julie-tools/src/inspect.rs`, `src/handler/tools/inspect.rs`, `crates/julie-tools/src/tests/inspect_tests.rs`. Delete `src/handler/tools/{get_symbols,deep_dive}.rs`. Modify `crates/julie-tools/src/lib.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands}.rs`, `src/tools/metrics/session.rs`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 2's names.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/inspect_tests.rs`:

```rust
use crate::inspect::{InspectDepth, InspectTarget, InspectTool};

#[test]
fn inspect_defaults_to_summary_depth_and_no_offset() {
    let tool: InspectTool = serde_json::from_value(serde_json::json!({ "target": "Foo" })).unwrap();
    assert_eq!(tool.depth, InspectDepth::Summary);
    assert_eq!(tool.offset, 0);
    assert_eq!(tool.workspace.as_deref(), Some("primary"));
}

#[test]
fn inspect_classifies_a_path_as_a_file_target_and_a_name_as_a_symbol_target() {
    assert_eq!(InspectTool::classify("src/lib.rs"), InspectTarget::File);
    assert_eq!(InspectTool::classify("crates/x/src/mod.rs"), InspectTarget::File);
    assert_eq!(InspectTool::classify("Foo"), InspectTarget::Symbol);
    assert_eq!(InspectTool::classify("Foo::bar"), InspectTarget::Symbol);
    assert_eq!(InspectTool::classify("lib.rs"), InspectTarget::File);
}

#[test]
fn inspect_maps_depth_onto_deep_dive_depth() {
    use crate::deep_dive::DeepDiveDepth;
    assert_eq!(InspectDepth::Summary.deep_dive(), DeepDiveDepth::Overview);
    assert_eq!(InspectDepth::Overview.deep_dive(), DeepDiveDepth::Context);
    assert_eq!(InspectDepth::Full.deep_dive(), DeepDiveDepth::Full);
}

#[test]
fn inspect_rejects_an_unknown_depth() {
    let err = serde_json::from_value::<InspectTool>(serde_json::json!({
        "target": "Foo", "depth": "everything"
    }))
    .unwrap_err();
    assert!(err.to_string().contains("summary"), "{err}");
}
```

Register the module in `crates/julie-tools/src/tests/mod.rs`. In `src/tests/request_engine.rs` add:

```rust
#[test]
fn catalog_registers_inspect_and_not_its_two_predecessors() {
    use crate::request_engine::catalog::AVAILABLE_TOOLS;
    assert!(AVAILABLE_TOOLS.contains(&"inspect"));
    assert!(!AVAILABLE_TOOLS.contains(&"get_symbols"));
    assert!(!AVAILABLE_TOOLS.contains(&"deep_dive"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools inspect_defaults_to_summary_depth_and_no_offset 2>&1 | tail -10`
Expected: FAIL to compile (`crate::inspect` missing).

**Step 3: Write minimal implementation**

`crates/julie-tools/src/inspect.rs`:

```rust
//! `inspect`: one public tool over the file-structure listing (`symbols`) and
//! the symbol investigation (`deep_dive`).

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::mcp_compat::CallToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::deep_dive::{DeepDiveDepth, DeepDiveTool};
use crate::symbols::GetSymbolsTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum InspectDepth {
    /// Definition, signature, and doc plus a bounded caller/callee list. Default.
    #[default]
    Summary,
    /// Summary plus the implementation body.
    Overview,
    /// Everything: all references, test locations, and bodies.
    Full,
}

impl InspectDepth {
    pub fn deep_dive(self) -> DeepDiveDepth {
        match self {
            Self::Summary => DeepDiveDepth::Overview,
            Self::Overview => DeepDiveDepth::Context,
            Self::Full => DeepDiveDepth::Full,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Overview => "overview",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectTarget {
    File,
    Symbol,
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// Inspect a file or a symbol you can name. A file target lists its symbols.
/// A symbol target returns its definition; `depth` adds relations and bodies.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct InspectTool {
    /// File path (relative to the workspace root) or symbol name. Qualified names like `Type::method` work.
    pub target: String,
    /// summary (default): definition, signature, doc, bounded callers/callees. overview: adds the body. full: all references, tests, bodies.
    #[serde(default)]
    pub depth: InspectDepth,
    /// Partial file path to disambiguate a symbol name that appears in several files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Maximum symbols for a file listing (default 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Rows to skip in a file listing; use the value from the `next:` line.
    #[serde(default, deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub offset: u32,
    /// Workspace filter: "primary" (default) or a workspace ID.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Optional semantic mode override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantics: Option<julie_core::embeddings_contract::SemanticMode>,
}

impl InspectTool {
    pub fn classify(target: &str) -> InspectTarget {
        let looks_like_path = target.contains('/')
            || target.contains('\\')
            || std::path::Path::new(target)
                .extension()
                .is_some_and(|ext| !ext.is_empty() && !target.contains("::"));
        if looks_like_path {
            InspectTarget::File
        } else {
            InspectTarget::Symbol
        }
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        match Self::classify(&self.target) {
            InspectTarget::File => self.as_symbols().call_tool(handler).await,
            InspectTarget::Symbol => self.as_deep_dive().call_tool(handler).await,
        }
    }

    pub fn as_symbols(&self) -> GetSymbolsTool {
        GetSymbolsTool {
            file_path: self.target.clone(),
            max_depth: 1,
            target: None,
            limit: self.limit,
            mode: Some("structure".to_string()),
            workspace: self.workspace.clone(),
        }
    }

    pub fn as_deep_dive(&self) -> DeepDiveTool {
        DeepDiveTool {
            symbol: self.target.clone(),
            depth: self.depth.deep_dive(),
            context_file: self.scope.clone(),
            workspace: self.workspace.clone(),
            semantics: self.semantics,
        }
    }
}
```

`DeepDiveDepth` (`crates/julie-tools/src/deep_dive/mod.rs:8`) needs `#[derive(PartialEq, Eq)]` added for the depth-mapping test; add it. If `GetSymbolsTool` or `DeepDiveTool` has private fields or a different field set, use their public constructors; do not change their fields. `offset` is stored here and consumed in Task 5.

`src/handler/tools/inspect.rs` follows the shape of `src/handler/tools/get_symbols.rs` (rmcp `#[tool(name = "inspect", …)]`, `execute_inspect(params: InspectTool)`), records telemetry with `tool_targets::inspect_metadata(&params)` whose `target` sub-object carries `target_file_path` for file targets and `target_symbol_name` for symbol targets, and sets `annotations(title = "Inspect")`.

Catalog row:

```rust
    "inspect" => {
        variant: Inspect,
        type: crate::tools::InspectTool,
        description: "Inspect a file or symbol you can name. A file target lists its symbols; a symbol target returns definition, signature, doc, and a bounded caller/callee list. depth=overview adds the body; depth=full adds all references and tests. Use before reading a file. NOT for discovering which symbol matters (context) or full reference lists (trace).",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |p| match crate::tools::InspectTool::classify(&p.target) {
            crate::tools::inspect::InspectTarget::File => SemanticRequirement::None,
            crate::tools::inspect::InspectTarget::Symbol => SemanticRequirement::Symbols,
        },
    },
```

Dispatch arm: `DecodedTool::Inspect(mut p) => { p.semantics = Some(core_mode); handler.execute_inspect(p).await }`. Delete the `GetSymbols` and `DeepDive` arms, rows, handler files, router terms, `ToolKind` variants, `tool_targets` builders, and CLI subcommands; add `InspectArgs { target, depth, scope, limit, offset, workspace }` and the `inspect` subcommand. `USEFUL_ACTIONS` becomes `["inspect", "fast_refs", "call_path", "context"]`. Update test literals (`grep -rln '"get_symbols"\|"deep_dive"' src crates xtask --include='*.rs'`), rewriting each call site to `inspect` with `target` and, where the old call passed `mode="minimal"` and `target=<name>`, to `inspect(target=<name>, depth="overview")`.

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools inspect_ 2>&1 | tail -10`
Expected: PASS (4 tests).

Run: `cargo nextest run --lib catalog_registers_inspect_and_not_its_two_predecessors 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(tools): fold get_symbols and deep_dive into inspect"` and record the SHA.

**Acceptance criteria:**
- [ ] The five tests above pass; `julie-server inspect src/request_engine/catalog.rs --json` lists symbols; `julie-server inspect ToolCatalog --json` returns the definition with no body; `--depth overview` includes the body.
- [ ] `grep -rn "get_symbols\|deep_dive" src crates --include='*.rs'` matches only the internal module paths `crate::symbols`, `crate::deep_dive`, and their `execute_*` names.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 4: `trace`

**Files:**
- Create: `crates/julie-tools/src/navigation/trace.rs`, `src/handler/tools/trace.rs`, `crates/julie-tools/src/tests/trace_tests.rs`
- Delete: `src/handler/tools/fast_refs.rs`, `src/handler/tools/call_path.rs`
- Modify: `crates/julie-tools/src/navigation/mod.rs` (add `pub mod trace; pub use trace::TraceTool;`), `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs` (one `trace` row replaces `fast_refs` and `call_path`), `src/request_engine/dispatch.rs`, `src/handler.rs:262-270,1346-1359`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs:33-55`, `src/cli.rs:45,51`, `src/cli_tools/subcommands.rs` (`TraceArgs` replaces `RefsArgs` and `CallPathArgs`), `src/cli_tools/commands.rs:158,235`, `src/tools/metrics/session.rs`, `src/dashboard/search_analysis.rs:8-17`
- Test: `crates/julie-tools/src/tests/trace_tests.rs`, `src/tests/request_engine.rs`

**Interfaces:**
- Consumes: `FastRefsTool` (`crates/julie-tools/src/navigation/fast_refs.rs:42`: `symbol`, `include_definition`, `limit`, `workspace`, `reference_kind`, `semantics`) and `CallPathTool` (`crates/julie-tools/src/navigation/call_path.rs:34`: `from`, `to`, `max_hops`, `workspace`, `from_file_path`, `to_file_path`, `mode`), their `call_tool`, and `handler.execute_fast_refs_with_budget` / `handler.execute_call_path`.
- Produces: `julie_tools::navigation::TraceTool { target, mode: TraceMode {Refs, Path}, to, scope, to_scope, reference_kind, include_definition, path_kind, max_hops, limit, offset, workspace, semantics }`; `handler.execute_trace_with_budget(params, budget)`; catalog row `"trace"`; CLI `julie-server trace <target> [--mode refs|path] [--to …]`.

**Contract inputs:** the two consumed structs; `SemanticRequirement::None` for both modes (matches today's rows).

**File ownership:** Create `crates/julie-tools/src/navigation/trace.rs`, `src/handler/tools/trace.rs`, `crates/julie-tools/src/tests/trace_tests.rs`. Delete `src/handler/tools/{fast_refs,call_path}.rs`. Modify `crates/julie-tools/src/navigation/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/tools/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler.rs`, `src/handler/tools/mod.rs`, `src/handler/tool_targets.rs`, `src/cli.rs`, `src/cli_tools/{commands,subcommands}.rs`, `src/tools/metrics/session.rs`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 3's catalog.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/trace_tests.rs`:

```rust
use crate::navigation::trace::{TraceMode, TraceTool};

#[test]
fn trace_defaults_to_refs_mode() {
    let tool: TraceTool = serde_json::from_value(serde_json::json!({ "target": "Foo" })).unwrap();
    assert_eq!(tool.mode, TraceMode::Refs);
    assert!(tool.include_definition);
    assert_eq!(tool.limit, 10);
    assert_eq!(tool.offset, 0);
}

#[test]
fn trace_path_mode_requires_to() {
    let tool: TraceTool = serde_json::from_value(serde_json::json!({
        "target": "Foo", "mode": "path"
    }))
    .unwrap();
    let err = tool.validate().unwrap_err();
    assert!(err.to_string().contains("`to`"), "{err}");
}

#[test]
fn trace_path_mode_builds_a_call_path_request() {
    let tool: TraceTool = serde_json::from_value(serde_json::json!({
        "target": "a", "mode": "path", "to": "b", "max_hops": 4,
        "scope": "src/a.rs", "to_scope": "src/b.rs", "path_kind": "web"
    }))
    .unwrap();
    let call_path = tool.as_call_path().unwrap();
    assert_eq!(call_path.from, "a");
    assert_eq!(call_path.to, "b");
    assert_eq!(call_path.max_hops, 4);
    assert_eq!(call_path.from_file_path.as_deref(), Some("src/a.rs"));
    assert_eq!(call_path.to_file_path.as_deref(), Some("src/b.rs"));
    assert_eq!(call_path.mode.as_deref(), Some("web"));
}

#[test]
fn trace_refs_mode_builds_a_refs_request() {
    let tool: TraceTool = serde_json::from_value(serde_json::json!({
        "target": "a", "reference_kind": "call", "limit": 3, "include_definition": false
    }))
    .unwrap();
    let refs = tool.as_refs();
    assert_eq!(refs.symbol, "a");
    assert_eq!(refs.reference_kind.as_deref(), Some("call"));
    assert_eq!(refs.limit, 3);
    assert!(!refs.include_definition);
}
```

In `src/tests/request_engine.rs` add:

```rust
#[test]
fn catalog_registers_trace_and_not_its_two_predecessors() {
    use crate::request_engine::catalog::AVAILABLE_TOOLS;
    assert!(AVAILABLE_TOOLS.contains(&"trace"));
    assert!(!AVAILABLE_TOOLS.contains(&"fast_refs"));
    assert!(!AVAILABLE_TOOLS.contains(&"call_path"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools trace_defaults_to_refs_mode 2>&1 | tail -10`
Expected: FAIL to compile (`navigation::trace` missing).

**Step 3: Write minimal implementation**

`crates/julie-tools/src/navigation/trace.rs`:

```rust
//! `trace`: one public tool over reference listing (`fast_refs`) and shortest
//! call paths (`call_path`).

use anyhow::{Result, bail};
use julie_context::ToolContext;
use julie_core::mcp_compat::CallToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::call_path::CallPathTool;
use super::fast_refs::FastRefsTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TraceMode {
    /// Every reference to `target` with its enclosing symbol. Default.
    #[default]
    Refs,
    /// The shortest call path from `target` to `to`.
    Path,
}

fn default_true() -> bool {
    true
}

fn default_limit() -> u32 {
    10
}

fn default_max_hops() -> u32 {
    6
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// Follow a thread of code: references to a symbol, or the call path between two symbols.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TraceTool {
    /// Symbol name (qualified names like `Type::method` work).
    pub target: String,
    /// refs (default) lists references; path finds the shortest call path to `to`.
    #[serde(default)]
    pub mode: TraceMode,
    /// Destination symbol for mode=path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Partial file path that disambiguates `target`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Partial file path that disambiguates `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_scope: Option<String>,
    /// mode=refs only: call, variable_ref, type_usage, member_access, import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_kind: Option<String>,
    /// mode=refs only: include the definition row (default true).
    #[serde(default = "default_true", deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient")]
    pub include_definition: bool,
    /// mode=path only: call (default) follows call edges; web also follows derived http_call edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_kind: Option<String>,
    /// mode=path only: maximum hops, 1 through 32 (default 6).
    #[serde(default = "default_max_hops", deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub max_hops: u32,
    /// Maximum rows (default 10).
    #[serde(default = "default_limit", deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub limit: u32,
    /// Rows to skip; use the value from the `next:` line.
    #[serde(default, deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub offset: u32,
    /// Workspace filter: "primary" (default) or a workspace ID.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Optional semantic mode override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantics: Option<julie_core::embeddings_contract::SemanticMode>,
}

impl TraceTool {
    pub fn validate(&self) -> Result<()> {
        if self.mode == TraceMode::Path && self.to.as_deref().unwrap_or("").is_empty() {
            bail!("trace mode=path needs `to`: the symbol the path should reach");
        }
        Ok(())
    }

    pub fn as_refs(&self) -> FastRefsTool {
        FastRefsTool {
            symbol: self.target.clone(),
            include_definition: self.include_definition,
            limit: self.limit,
            workspace: self.workspace.clone(),
            reference_kind: self.reference_kind.clone(),
            semantics: self.semantics,
        }
    }

    pub fn as_call_path(&self) -> Result<CallPathTool> {
        self.validate()?;
        Ok(CallPathTool {
            from: self.target.clone(),
            to: self.to.clone().unwrap_or_default(),
            max_hops: self.max_hops,
            workspace: self.workspace.clone(),
            from_file_path: self.scope.clone(),
            to_file_path: self.to_scope.clone(),
            mode: self.path_kind.clone(),
        })
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        match self.mode {
            TraceMode::Refs => self.as_refs().call_tool(handler).await,
            TraceMode::Path => self.as_call_path()?.call_tool(handler).await,
        }
    }
}
```

If `FastRefsTool` or `CallPathTool` has private fields, add a `pub fn` constructor on that struct with the same field list; do not expose new behavior.

`src/handler/tools/trace.rs` follows `src/handler/tools/fast_refs.rs`, with `execute_trace_with_budget(params, budget)` that branches on `params.mode` to the existing `execute_fast_refs_with_budget(params.as_refs(), budget)` or `execute_call_path(params.as_call_path()?)`. Telemetry: `tool_targets::trace_metadata(&params)` with `target_symbol_name = params.target` and a `mode` key.

Catalog row:

```rust
    "trace" => {
        variant: Trace,
        type: crate::tools::navigation::TraceTool,
        description: "Follow a thread of code. mode=refs (default) lists exact references to target with their enclosing symbols; filter with reference_kind. mode=path finds the shortest call path from target to `to`. Page with offset. NOT for a symbol's own definition (inspect) or which tests to run (impact).",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
```

Dispatch arm: `DecodedTool::Trace(mut p) => { p.semantics = Some(core_mode); handler.execute_trace_with_budget(p, budget).await }`. Delete `FastRefs` and `CallPath` arms, rows, handler files, router terms, `ToolKind` variants, `tool_targets` builders, CLI subcommands `refs` and `call-path`; add `TraceArgs` and the `trace` subcommand. `USEFUL_ACTIONS` becomes `["inspect", "trace", "context"]`. Rewrite test literals (`grep -rln '"fast_refs"\|"call_path"' src crates xtask --include='*.rs'`).

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools trace_ 2>&1 | tail -10`
Expected: PASS (4 tests).

Run: `cargo nextest run --lib catalog_lists_exactly_the_seven_contract_tools 2>&1 | tail -10`
Expected: PASS. This is the first task where the Task 1 contract test goes green.

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(tools): fold fast_refs and call_path into trace"` and record the SHA.

**Acceptance criteria:**
- [ ] The six tests above pass; `julie-server trace ToolCatalog --json` lists references; `julie-server trace run_stdio_shim --mode path --to forward --json` returns a path.
- [ ] `catalog_lists_exactly_the_seven_contract_tools` passes and `AVAILABLE_TOOLS` is `["context","impact","inspect","patterns","search","trace","workspace"]`.
- [ ] `src/handler.rs:1346-1359` composes exactly seven routers and `xtask/tests/docs_contract_tests.rs` `public_tool_names()` returns those seven (run `cargo test -p xtask --test docs_contract_tests docs_contract_tests_public_surface_includes_patterns`).
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 5: `search` shape, compact output, and paging

**Files:**
- Modify: `crates/julie-tools/src/search/params.rs:14-190` (`FastSearchTool`: add `mode`, rename `backend` to `retrieval`, rename `return_format` to `format`, add `offset`), `crates/julie-tools/src/search/tool_execution.rs:200-260` (mode routing, format switch, paging), `crates/julie-tools/src/search/formatting.rs` (compact renderer), `crates/julie-tools/src/shared.rs:11-13` (replace `truncation_line` with `next_line`), `crates/julie-tools/src/impact/formatting.rs:100`, `crates/julie-tools/src/get_context/formatting.rs:191,252`, `crates/julie-tools/src/navigation/formatting.rs` (refs paging), `crates/julie-tools/src/inspect.rs` + `crates/julie-tools/src/symbols/formatting.rs` (file-listing paging), `src/request_engine/catalog.rs` (the `search` `semantics` closure reads `retrieval`), `src/handler/tools/search.rs`, `src/handler/search_telemetry.rs` (log `mode` and `retrieval`), `src/cli_tools/subcommands.rs` (`SearchArgs`), `docs/eval/semantic-value/run_scorecard.py` (send `retrieval` and `format`), `docs/eval/semantic-value/scorecard.toml` (`backends` → `retrievals`, `return_format` → `format`)
- Test: `crates/julie-tools/src/tests/search_params_tests.rs` (new), `crates/julie-tools/src/tests/formatting_tests.rs`, `crates/julie-tools/src/tests/search_promotion_tests.rs`

**Interfaces:**
- Consumes: `FastSearchTool` fields (`query`, `language`, `file_pattern`, `limit`, `context_lines`, `exclude_tests`, `backend: Option<SearchBackend>`, `workspace`, `return_format: String`, `semantics`), wrapper `FastSearchParams { search, regions }`; `text_search_impl(query, language, file_pattern, limit, workspace_ids, search_target: &str, context_lines, exclude_tests, handler)` in `crates/julie-tools/src/search/text_search.rs:224` where `search_target` accepts `"definitions"`; `execute_search_unified` in `crates/julie-tools/src/search/execution/mod.rs:70`; `SearchHit` kinds (symbol row versus file row).
- Produces: `search` parameters `mode: SearchMode {Auto, Symbol, File, Text}` (default `Auto`), `retrieval: Option<SearchBackend>` (JSON name `retrieval`, values `lexical|semantic|hybrid`), `format: SearchFormat {Compact, Full}` (default `Compact`), `offset: u32`; the shared trailer `julie_tools::shared::next_line(tool, args, next_offset) -> String`; compact renderer output shape below. Tasks 6 and 8 rely on `next_line` and on `format=compact` being the default.

**Contract inputs:** the compact shape, exactly:

```
<N> hits for "<query>" (<mode>, <retrieval>)
<path>:
  :<line> <Name> <kind>
  :<line> <Name> <kind>
<path>:<line> <Name> <kind>
next: search query="<query>" offset=<offset+kept>
```

A file that appears once renders on one line; a file that appears more than once renders as a group. File-row hits render as `<path>` alone. The `next:` line appears only when more rows exist. With `format=full` the current full renderer runs unchanged after the same header line.

**File ownership:** Modify `crates/julie-tools/src/search/{params,tool_execution,formatting}.rs`, `crates/julie-tools/src/shared.rs`, `crates/julie-tools/src/impact/formatting.rs`, `crates/julie-tools/src/get_context/formatting.rs`, `crates/julie-tools/src/navigation/{trace,formatting}.rs`, `crates/julie-tools/src/inspect.rs`, `crates/julie-tools/src/symbols/formatting.rs`, `src/request_engine/catalog.rs`, `src/handler/tools/search.rs`, `src/handler/search_telemetry.rs`, `src/cli_tools/subcommands.rs`, `docs/eval/semantic-value/run_scorecard.py`, `docs/eval/semantic-value/scorecard.toml`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Tasks 3 and 4's structs for the shared trailer.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/search_params_tests.rs`:

```rust
use crate::search::{FastSearchParams, SearchBackend, SearchFormat, SearchMode};
use crate::shared::next_line;

#[test]
fn search_defaults_to_auto_mode_compact_format_and_no_offset() {
    let p: FastSearchParams = serde_json::from_value(serde_json::json!({ "query": "x" })).unwrap();
    assert_eq!(p.search.mode, SearchMode::Auto);
    assert_eq!(p.search.format, SearchFormat::Compact);
    assert_eq!(p.search.retrieval, None);
    assert_eq!(p.search.offset, 0);
}

#[test]
fn search_reads_retrieval_and_rejects_backend() {
    let p: FastSearchParams =
        serde_json::from_value(serde_json::json!({ "query": "x", "retrieval": "lexical" })).unwrap();
    assert_eq!(p.search.retrieval, Some(SearchBackend::Lexical));
    let err = serde_json::from_value::<FastSearchParams>(serde_json::json!({
        "query": "x", "backend": "lexical"
    }))
    .unwrap_err();
    assert!(err.to_string().contains("retrieval"), "{err}");
}

#[test]
fn next_line_names_the_tool_and_the_next_offset() {
    assert_eq!(
        next_line("search", &[("query", "\"a b\"")], 16),
        "next: search query=\"a b\" offset=16"
    );
    assert_eq!(next_line("trace", &[("target", "Foo"), ("mode", "refs")], 10),
        "next: trace target=Foo mode=refs offset=10");
}
```

In `crates/julie-tools/src/tests/formatting_tests.rs` add a compact renderer test using the existing fixture helpers in that file (the file already builds `SearchHit` values for the full renderer; reuse that builder):

```rust
#[test]
fn compact_search_groups_repeated_files_and_appends_next_when_rows_remain() {
    let hits = vec![
        hit("src/a.rs", 10, "alpha", "function"),
        hit("src/a.rs", 20, "beta", "function"),
        hit("src/b.rs", 5, "gamma", "struct"),
    ];
    let text = crate::search::formatting::render_compact("q", "auto", "lexical", &hits, 0, 3, true);
    assert_eq!(
        text,
        "3 hits for \"q\" (auto, lexical)\nsrc/a.rs:\n  :10 alpha function\n  :20 beta function\nsrc/b.rs:5 gamma struct\nnext: search query=\"q\" offset=3"
    );
    let last_page = crate::search::formatting::render_compact("q", "auto", "lexical", &hits, 0, 3, false);
    assert!(!last_page.contains("next:"));
}
```

`hit(path, line, name, kind)` is the fixture builder already in `formatting_tests.rs`; if it has another name, use that name.

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools search_defaults_to_auto_mode_compact_format_and_no_offset 2>&1 | tail -10`
Expected: FAIL to compile (`SearchMode` missing).

**Step 3: Write minimal implementation**

1. `crates/julie-tools/src/search/params.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Mixed symbol, file, and line hits. Default.
    #[default]
    Auto,
    /// Symbol definitions only.
    Symbol,
    /// File paths only.
    File,
    /// Line matches with context; combine with `regions` to restrict to comments, docs, or strings.
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchFormat {
    /// One line per hit, grouped by file. Default.
    #[default]
    Compact,
    /// Code context and rich summaries.
    Full,
}
```

   On `FastSearchTool`: add `pub mode: SearchMode` (`#[serde(default)]`), rename `backend` to `retrieval` (field and JSON name; keep type `Option<SearchBackend>`), replace `return_format: String` with `format: SearchFormat`, add `pub offset: u32` with the lenient u32 deserializer. The custom `Deserialize` at `:104` must reject the old names `backend` and `return_format` with a message that names the replacement (`"unknown field `backend`; use `retrieval`"`). Update `Default` at `:186` and the doc comment at `:13`.
2. `crates/julie-tools/src/shared.rs`: replace `truncation_line` with

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

   Update the three `truncation_line` call sites: `impact/formatting.rs:100` becomes `next_line("impact", &impact_args, offset + kept)`, `get_context/formatting.rs:191,252` keep truncating without a trailer (context is budgeted, not paged; delete the line), and add trailers in `navigation/formatting.rs` (refs) and `symbols/formatting.rs` (file listing).
3. `crates/julie-tools/src/search/tool_execution.rs`: route on `mode`: `Symbol` calls `text_search_impl(…, "definitions", …)`; `File` runs the unified search and keeps only file-row hits; `Text` runs the unified search with `context_lines` and honors `regions`; `Auto` is today's path. Fetch `limit + offset` rows, skip `offset`, keep `limit`, and pass `more = fetched.len() > offset + limit` to the renderer. Read `format` instead of `return_format == "locations"`. Delete the `"locations"` string handling in `region_search.rs:126`.
4. `crates/julie-tools/src/search/formatting.rs`: add `render_compact(query, mode, retrieval, hits, offset, kept, more) -> String` producing the shape in **Contract inputs**. Render the retrieval label as `lexical`, `semantic`, `hybrid`, or `auto`.
5. `src/request_engine/catalog.rs`: the `search` `semantics` closure reads `p.search.retrieval`. `src/handler/search_telemetry.rs`: add `mode` and `retrieval` keys to the metadata. `src/cli_tools/subcommands.rs` `SearchArgs`: `--mode`, `--retrieval`, `--format`, `--offset`; delete `--return-format` and `--backend`.
6. `docs/eval/semantic-value/run_scorecard.py` and `scorecard.toml`: send `retrieval` and `format="compact"`; the parser reads `<path>:<line>` and `<path>:` group headers. Run one lexical dry run to confirm ranks (`JULIE_EMBEDDING_PROVIDER=none python3 docs/eval/semantic-value/run_scorecard.py --binary target/release/julie-server --retrieval lexical --no-write --timeout 300`) and record the top-5 count in the commit body.
7. Rewrite test literals: `backend` → `retrieval`, `return_format` → `format`, `"locations"` → `"compact"` across `src/tests` and `crates/julie-tools/src/tests` (`grep -rln 'return_format\|"backend"' src crates docs/eval --include='*.rs' --include='*.py' --include='*.toml' --include='*.json'`).

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools search_defaults_to_auto_mode_compact_format_and_no_offset 2>&1 | tail -10`
Expected: PASS

Run: `cargo nextest run -p julie-tools compact_search_groups_repeated_files_and_appends_next_when_rows_remain 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(search): contract shape with mode, retrieval, compact output, and offset paging"` and record the SHA.

**Acceptance criteria:**
- [ ] The four new tests pass; `julie-server search "bind_default_workspace" --json` returns the compact shape; `--offset 6` returns the next page; `--format full` returns today's full output.
- [ ] `grep -rn "truncation_line\|return_format\|\"locations\"" src crates docs/eval` returns nothing.
- [ ] `cargo xtask test dogfood` passes at this commit (lead runs it; worker reports ready).
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 6: `impact` shape

**Files:**
- Create: `crates/julie-tools/src/impact/params.rs`, `crates/julie-tools/src/tests/impact_params_tests.rs`
- Modify: `crates/julie-tools/src/impact/mod.rs:22-100` (`BlastRadiusTool` becomes the internal request; `ImpactTool` is the public struct), `crates/julie-tools/src/tests/mod.rs`, `src/request_engine/catalog.rs` (the `impact` row's `type` becomes `ImpactTool`), `src/request_engine/dispatch.rs`, `src/handler/tools/impact.rs`, `src/handler/tool_targets.rs:111-123`, `src/cli_tools/subcommands.rs` (`BlastRadiusArgs` → `ImpactArgs`), `src/cli_tools/commands.rs:263`
- Test: `crates/julie-tools/src/tests/impact_params_tests.rs`

**Interfaces:**
- Consumes: `BlastRadiusTool { symbol_ids: Vec<String>, file_paths: Vec<String>, max_depth: u32 = 2, limit: u32 = 12, include_tests: bool = true, format, workspace, mode }` and its `call_tool`; symbol-name resolution in `crates/julie-tools/src/navigation/resolution.rs` (the function `DeepDiveTool` uses to turn a name into symbol rows); `next_line` from Task 5.
- Produces: `ImpactTool { target: Option<String>, changed_paths: Vec<String>, git: bool, max_depth, limit, offset, include_tests, workspace }` with `ImpactTool::into_request(&self, handler) -> Result<BlastRadiusTool>`; no-argument form reads `git diff --name-only HEAD` plus untracked files (`git ls-files --others --exclude-standard`) in the workspace root.

**Contract inputs:** exactly one of `target`, `changed_paths`, `git=true`; none given means `git=true`.

**File ownership:** Create `crates/julie-tools/src/impact/params.rs`, `crates/julie-tools/src/tests/impact_params_tests.rs`. Modify `crates/julie-tools/src/impact/mod.rs`, `crates/julie-tools/src/tests/mod.rs`, `src/request_engine/catalog.rs`, `src/request_engine/dispatch.rs`, `src/handler/tools/impact.rs`, `src/handler/tool_targets.rs`, `src/cli_tools/{commands,subcommands}.rs`, tests listed in the task.

**Serialization required:** Yes

**Dependency reason:** Needs Task 5's trailer helper.

**Step 1: Write the failing test**

`crates/julie-tools/src/tests/impact_params_tests.rs`:

```rust
use crate::impact::params::{ImpactSeed, ImpactTool};

#[test]
fn impact_with_no_arguments_reads_the_git_diff() {
    let tool: ImpactTool = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(tool.seed().unwrap(), ImpactSeed::GitDiff);
}

#[test]
fn impact_target_and_changed_paths_are_exclusive() {
    let tool: ImpactTool = serde_json::from_value(serde_json::json!({
        "target": "Foo", "changed_paths": ["src/a.rs"]
    }))
    .unwrap();
    let err = tool.seed().unwrap_err();
    assert!(err.to_string().contains("one of"), "{err}");
}

#[test]
fn impact_target_seeds_by_symbol_name() {
    let tool: ImpactTool = serde_json::from_value(serde_json::json!({ "target": "Foo" })).unwrap();
    assert_eq!(tool.seed().unwrap(), ImpactSeed::Symbol("Foo".to_string()));
}

#[test]
fn impact_changed_files_parses_git_output() {
    let paths = crate::impact::params::changed_files_from_git_output(
        "src/a.rs\nsrc/b.rs\n",
        "new.rs\n",
    );
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs", "new.rs"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo nextest run -p julie-tools impact_with_no_arguments_reads_the_git_diff 2>&1 | tail -10`
Expected: FAIL to compile (`impact::params` missing).

**Step 3: Write minimal implementation**

`crates/julie-tools/src/impact/params.rs`:

```rust
//! Public `impact` parameters. Resolves to the internal blast-radius request.

use std::path::Path;

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::BlastRadiusTool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImpactSeed {
    GitDiff,
    Symbol(String),
    Paths(Vec<String>),
}

fn default_max_depth() -> u32 {
    2
}

fn default_limit() -> u32 {
    12
}

fn default_true() -> bool {
    true
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// What a change affects and which tests to run. With no arguments it reads the working-tree git diff.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ImpactTool {
    /// Symbol name to seed from (qualified names work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Changed file paths, relative to the workspace root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_paths: Vec<String>,
    /// Seed from the working-tree git diff (default when nothing else is given).
    #[serde(default, deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient")]
    pub git: bool,
    /// Relationship hops to follow (default 2).
    #[serde(default = "default_max_depth", deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub max_depth: u32,
    /// Maximum impacted rows (default 12).
    #[serde(default = "default_limit", deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub limit: u32,
    /// Rows to skip; use the value from the `next:` line.
    #[serde(default, deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient")]
    pub offset: u32,
    /// Include likely tests (default true).
    #[serde(default = "default_true", deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient")]
    pub include_tests: bool,
    /// Workspace filter: "primary" (default) or a workspace ID.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl ImpactTool {
    pub fn seed(&self) -> Result<ImpactSeed> {
        let given = usize::from(self.target.is_some())
            + usize::from(!self.changed_paths.is_empty())
            + usize::from(self.git);
        if given > 1 {
            bail!("impact takes one of target, changed_paths, or git=true");
        }
        Ok(match (&self.target, self.changed_paths.is_empty()) {
            (Some(name), _) => ImpactSeed::Symbol(name.clone()),
            (None, false) => ImpactSeed::Paths(self.changed_paths.clone()),
            (None, true) => ImpactSeed::GitDiff,
        })
    }

    pub fn into_request(&self, root: &Path, symbol_ids: Vec<String>) -> Result<BlastRadiusTool> {
        let (symbol_ids, file_paths) = match self.seed()? {
            ImpactSeed::Symbol(_) => (symbol_ids, Vec::new()),
            ImpactSeed::Paths(paths) => (Vec::new(), paths),
            ImpactSeed::GitDiff => (Vec::new(), changed_files(root)?),
        };
        Ok(BlastRadiusTool {
            symbol_ids,
            file_paths,
            max_depth: self.max_depth,
            limit: self.limit,
            include_tests: self.include_tests,
            format: None,
            workspace: self.workspace.clone(),
            mode: None,
        })
    }
}

fn changed_files(root: &Path) -> Result<Vec<String>> {
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

The handler (`src/handler/tools/impact.rs` `execute_impact`) resolves `ImpactSeed::Symbol(name)` to symbol ids with the same resolution call `DeepDiveTool` uses (`crates/julie-tools/src/navigation/resolution.rs`; pass `scope: None`), then calls `params.into_request(workspace_root, ids)?.call_tool(handler)`. When the name resolves to nothing, return the same not-found text `inspect` returns. Paging: fetch `limit + offset`, skip `offset`, and let `impact/formatting.rs` emit `next_line("impact", &[("target", …)] or [("git", "true")], offset + kept)` when more rows exist. Update the catalog row `type`, the dispatch arm (`DecodedTool::Impact(p) => handler.execute_impact(p).await`), `tool_targets::impact_metadata` (`target_symbol_name` or the seed kind), CLI `ImpactArgs`, and test literals (`symbol_ids` → `target`, `file_paths` → `changed_paths`).

**Step 4: Run test to verify it passes**

Run: `cargo nextest run -p julie-tools impact_ 2>&1 | tail -10`
Expected: PASS (4 tests).

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "feat(impact): seed from a symbol name, changed paths, or the git diff"` and record the SHA.

**Acceptance criteria:**
- [ ] The four tests pass; in a checkout with one edited file, `julie-server impact --json` lists impacted symbols and likely tests for that file; `julie-server impact --target ToolCatalog --json` seeds by name.
- [ ] `grep -rn "symbol_ids\|file_paths" src/tests crates/julie-tools/src/tests` matches only internal `BlastRadiusTool` tests.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 7: Guidance, hooks, skills, and docs

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md` (rewrite, at most 1,900 characters), `README.md` (43 tool-name occurrences), `docs/site/index.html:312-347,403-539` (four skill cards, twelve tool cards become seven), `docs/site/script.js` (six occurrences), `.claude/hooks/hooks.json`, `.claude/skills/{dead-code-audit,explore-area,impact-analysis,search-debug,web-research}/SKILL.md` (names, `allowed-tools`, examples), `xtask/tests/docs_contract_tests.rs:133-160,222-249` (seven cards; instructions must contain `search`, `inspect`, `trace`, `impact`, `patterns`, `regions`, `structural_facts`, `complexity_metrics`; add the 1,900-character test), `src/tests/core/workspace_init/instructions_paths.rs:34` (asserts `search`), `CLAUDE.md` and `AGENTS.md` (Quick Reference CLI names, "Adding a new MCP tool", `fast_search(...)` examples)
- Create: `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`, `.claude/skills/large-file/SKILL.md`
- Delete: `.claude/hooks/pretool-edit.cjs`
- Test: `xtask/tests/docs_contract_tests.rs`

**Interfaces:**
- Consumes: the seven catalog names and their parameter names from Tasks 2 through 6; `include_str!("../JULIE_AGENT_INSTRUCTIONS.md")` at `src/handler.rs:1335`; `xtask sync-plugin` (`xtask/src/sync_plugin.rs`) mirrors `.claude/skills` and diffs `.claude/hooks`.
- Produces: the guidance text agents see; the dev session hook; five rewritten skills plus one new skill. Task 8's harness README points at these.

**Contract inputs:** Miller's routing block (`~/source/miller/hooks/miller-routing-block.md`, read-only) and session hook (`~/source/miller/hooks/miller-session-hook.cjs`, read-only) are the models. Copy the hook's stdin parsing, envelope shape (`{hookSpecificOutput:{hookEventName, additionalContext}}`), fail-open behavior, and kill switch (`JULIE_SESSION_HOOKS=0`); drop the candidate-root appendix (the shim binds the working directory).

**File ownership:** Modify `JULIE_AGENT_INSTRUCTIONS.md`, `README.md`, `docs/site/index.html`, `docs/site/script.js`, `.claude/hooks/hooks.json`, `.claude/skills/{dead-code-audit,explore-area,impact-analysis,search-debug,web-research}/SKILL.md`, `xtask/tests/docs_contract_tests.rs`, `src/tests/core/workspace_init/instructions_paths.rs`, `CLAUDE.md`, `AGENTS.md`. Create `.claude/hooks/julie-routing-block.md`, `.claude/hooks/session-start.cjs`, `.claude/skills/large-file/SKILL.md`. Delete `.claude/hooks/pretool-edit.cjs`.

**Serialization required:** Yes

**Dependency reason:** Names must be final.

**Step 1: Write the failing test**

In `xtask/tests/docs_contract_tests.rs`:

```rust
#[test]
fn docs_contract_tests_public_surface_is_the_seven_tool_contract() {
    let names: Vec<String> = public_tool_names().into_iter().collect();
    assert_eq!(
        names,
        ["context", "impact", "inspect", "patterns", "search", "trace", "workspace"]
    );
}

#[test]
fn docs_contract_tests_agent_instructions_fit_the_server_instruction_budget() {
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    assert!(
        instructions.chars().count() <= 1900,
        "JULIE_AGENT_INSTRUCTIONS.md is {} characters; the ceiling is 1900",
        instructions.chars().count()
    );
    for name in ["search", "inspect", "context", "trace", "impact", "patterns", "workspace"] {
        assert!(instructions.contains(&format!("`{name}`")), "instructions must name {name}");
    }
    for old in ["fast_search", "get_symbols", "deep_dive", "fast_refs", "call_path", "blast_radius", "get_context", "manage_workspace", "edit_file", "rewrite_symbol", "rename_symbol"] {
        assert!(!instructions.contains(old), "instructions still name {old}");
    }
}

#[test]
fn docs_contract_tests_skills_and_site_use_contract_names_only() {
    let old = ["fast_search", "get_symbols", "deep_dive", "fast_refs", "call_path", "blast_radius", "get_context", "manage_workspace", "edit_file", "rewrite_symbol", "rename_symbol", "spillover_get"];
    for path in [
        "README.md",
        "docs/site/index.html",
        "docs/site/script.js",
        ".claude/hooks/julie-routing-block.md",
        ".claude/skills/dead-code-audit/SKILL.md",
        ".claude/skills/explore-area/SKILL.md",
        ".claude/skills/impact-analysis/SKILL.md",
        ".claude/skills/search-debug/SKILL.md",
        ".claude/skills/web-research/SKILL.md",
        ".claude/skills/large-file/SKILL.md",
    ] {
        let text = read_repo_file(path);
        for name in old {
            assert!(!text.contains(name), "{path} still names {name}");
        }
    }
    assert!(!std::path::Path::new(&repo_path(".claude/skills/editing")).exists());
}
```

`repo_path` is the helper `read_repo_file` uses to build its path; if it has another name, use that. Update `docs_contract_tests_extractor_enrichment_surfaces_are_documented` (`:228-249`) to look for `patterns`, `regions`, `structural_facts`, `complexity_metrics` in the instructions and README as it does today, and the site card count (`:133-160`) to seven.

**Step 2: Run test to verify it fails**

Run: `cargo test -p xtask --test docs_contract_tests docs_contract_tests_agent_instructions_fit_the_server_instruction_budget 2>&1 | tail -10`
Expected: FAIL (`9400 characters`).

**Step 3: Write minimal implementation**

1. `JULIE_AGENT_INSTRUCTIONS.md`, complete text (1,489 characters; keep it under 1,900 after any edit):

```markdown
# Julie - code intelligence for this workspace

One Julie call beats shell greps and full-file reads. Results come from a fresh index; do not re-verify with grep or Read.

## Rules

1. `search` before reading: ranked symbol, file, and line hits. `mode=symbol` for definitions, `mode=file` for paths, `retrieval=lexical` for zero vector work.
2. `inspect` a file for its symbols, or a symbol for its definition, before reading a whole file. Default depth is `summary`; ask for `overview` only when you need the body, `full` only for every reference.
3. `context` first in an unfamiliar area: a token-budgeted bundle of pivots and neighbours for a task.
4. `trace` for references (`mode=refs`) or the call path between two symbols (`mode=path`).
5. `impact` before a refactor and after edits: impacted symbols and likely tests. With no arguments it reads the working-tree git diff.
6. `patterns` for extracted code shapes: routes, config keys, SQL, document structure. Call with no arguments to list pattern ids.
7. `workspace` for index status, refresh, rebuild, health, and opening another checkout for cross-workspace calls.

Output is compact by default; a result that has more rows ends with a `next:` line holding the exact call for the next page. `format=full` adds code context. Julie has no edit tool: edit with your own editor, then run `impact`.

Every call takes `workspace`: omit it or pass `primary` for the checkout this session started in, or the id `workspace list` returns.
```

2. `.claude/hooks/julie-routing-block.md`: Miller's routing block rewritten for the seven tools and the four deviations (no edit, no content, `offset` paging, `workspace` default), at most 3,300 bytes. `.claude/hooks/session-start.cjs`: port of `miller-session-hook.cjs` with `ROUTING_BLOCK_FILE = 'julie-routing-block.md'`, kill switch `JULIE_SESSION_HOOKS`, events `session-start` and `subagent-start`, and no candidate-root block. `.claude/hooks/hooks.json`:

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

   Read `.claude/hooks/pretool-agent.cjs` and `session-start-tests.cjs`; delete `pretool-edit.cjs` (it steers to the deleted edit tools). Leave the other two unless they name a deleted tool; if they do, delete them too and say so in the commit body.
3. Skills: rewrite the five `SKILL.md` files so every example uses the new names and parameters (`inspect(target=…, depth=…)`, `trace(target=…, mode=…)`, `impact(target=… | changed_paths=… | none)`, `search(query=…, mode=…, retrieval=…)`), update `allowed-tools` to `mcp__julie__<name>` for the seven names, and remove every reference to `edit_file`, `rewrite_symbol`, `rename_symbol`. Delete `.claude/skills/editing/`. Create `.claude/skills/large-file/SKILL.md`:

```markdown
---
name: large-file
description: Use when you need to inspect, search, or quote a large text file such as a log, CI output, JSON dump, or generated report without reading the whole file into context. Routes to jq, rg, sed, head, and wc instead of a full read.
user-invocable: true
arguments: "<absolute path> [what you are looking for]"
---

# Large files without full reads

Julie indexes source code. Logs, CI output, and data dumps are not indexed. Never read such a file whole; measure it, then read only windows.

1. Size first: `wc -lc <file>`. Under 200 lines, read it. Otherwise continue.
2. Find the lines: `rg -n "<pattern>" <file> | head -40`. Add `-C 3` for context.
3. Read a window: `sed -n '<start>,<end>p' <file>`. Keep windows under 80 lines.
4. Structured data: `jq '<path>' <file>` for JSON, `yq` for YAML, `python3 -c 'import tomllib,sys;print(tomllib.load(open(sys.argv[1],"rb"))["<key>"])' <file>` for TOML.
5. Tail a live log: `tail -n 100 <file>`, then `rg` inside that tail.

Quote at most the lines that answer the question, with their line numbers.
```

4. `README.md`, `docs/site/index.html`, `docs/site/script.js`: replace the twelve tool cards with seven, delete the three edit cards and the editing skill card, add a large-file skill card, and update every name. `CLAUDE.md` and `AGENTS.md`: the Quick Reference and Development Workflow lists become `search`, `inspect`, `context`, `trace`, `impact`, `patterns`, `workspace` (plus `tool <name>`, `tools`, `service`, `dashboard`); the `fast_search(query="workspace routing", search_target="definitions", file_pattern="docs/**")` examples become `search(query="workspace routing", mode="symbol", file_pattern="docs/**")`; "Adding a new MCP tool" names `src/request_engine/catalog.rs` first. Keep both files identical (`hooks/pre-commit` enforces it).
5. Run `cargo xtask sync-plugin --dry-run` and paste its report into the commit body; do not run the live sync (the plugin repo is out of scope).

**Step 4: Run test to verify it passes**

Run: `cargo test -p xtask --test docs_contract_tests 2>&1 | tail -15`
Expected: all docs contract tests PASS.

Run: `cargo nextest run --lib instructions_paths 2>&1 | tail -10`
Expected: PASS

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "docs(contract): rewrite agent instructions, session hook, skills, and site for the seven-tool contract"` and record the SHA.

**Acceptance criteria:**
- [ ] The three new docs contract tests pass; `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters.
- [ ] `node .claude/hooks/session-start.cjs session-start < /dev/null` prints a JSON envelope whose `additionalContext` starts with the routing block; with `JULIE_SESSION_HOOKS=0` it prints nothing and exits 0.
- [ ] `grep -rn "fast_search\|get_symbols\|deep_dive\|fast_refs\|call_path\|blast_radius\|get_context\|manage_workspace\|edit_file\|rewrite_symbol\|rename_symbol" README.md docs/site .claude JULIE_AGENT_INSTRUCTIONS.md CLAUDE.md AGENTS.md` returns nothing.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 8: Head-to-head run against Miller

**Files:**
- Create: `docs/eval/head-to-head/run_matrix.py`, `docs/eval/head-to-head/mcp_client.py`, `docs/eval/head-to-head/cases.json`, `docs/eval/head-to-head/README.md`, `docs/eval/head-to-head/results/<timestamp>.json`, `docs/eval/head-to-head/results/<timestamp>.md`, `docs/findings/2026-09-1X-head-to-head-miller.md`
- Test: none in Rust; the harness self-checks (`python3 docs/eval/head-to-head/run_matrix.py --validate`)

**Interfaces:**
- Consumes: Miller's `scripts/bench-foundation-matrix.py` and `scripts/benchlib/mcp_client.py` (read-only sources at `~/source/miller`); Miller binary `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller` (build it with `dotnet build -c Release` in that repo if absent; do not edit that repo); Julie stdio shim `target/release/julie-server` run with `cwd=<repo>`; the ten public repos from `docs/eval/semantic-value/scorecard.toml` and their 23 search cases.
- Produces: `cases.json` rows `{id, repo, task_class, intent, julie:{tool,args}, miller:{tool,args}, expected:{path,anchor}, scoring:{mode}}` and a results pair plus the finding.

**Contract inputs:** the design's section 9 head-to-head; the frozen contract in `docs/plans/2026-09-07-julie-head-to-head-evaluation.md` (task classes, "never index answers", paired runs, report variation). This task runs the retrieval matrix only (search, inspect, trace, impact rows); agent episodes and the Rust `xtask-eval revival` harness stay deferred, and the finding says so.

**File ownership:** Create `docs/eval/head-to-head/{run_matrix.py,mcp_client.py,cases.json,README.md}`, `docs/eval/head-to-head/results/<timestamp>.{json,md}`, `docs/findings/2026-09-1X-head-to-head-miller.md`.

**Serialization required:** Yes

**Dependency reason:** Needs the finished contract.

**Step 1: Write the failing test**

`run_matrix.py --validate` must fail on a manifest with a missing repo, a row naming a tool outside the seven Julie names or Miller's `search|inspect|context|trace|impact|patterns`, or an `expected.path` that does not exist in the repo. Write the validator first with this self-check block at the bottom of `run_matrix.py`:

```python
if __name__ == "__main__" and "--self-check" in sys.argv:
    bad = {"schema_version": 1, "rows": [{"id": "x", "repo": "nope", "task_class": "retrieval.symbol",
           "julie": {"tool": "fast_search", "args": {}}, "miller": {"tool": "search", "args": {}},
           "expected": {"path": "missing.rs", "anchor": ""}, "scoring": {"mode": "path_top"}}]}
    errors = validate_manifest(bad, repos={})
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
2. `run_matrix.py`: a trimmed port of `bench-foundation-matrix.py` with these changes: binaries come from `--julie-bin` (default `target/release/julie-server`) and `--miller-bin` (default `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller`); repos come from `--repos-toml` (default `docs/eval/semantic-value/scorecard.toml`, read the `[[repos]]` table); Julie is spawned as `[julie_bin]` with `cwd=repo_path` (the shim binds `primary` to cwd); Miller is spawned as `[miller_bin, "serve"]` and every Miller row passes `workspace_id` from `workspace operation=open path=<repo>` once per repo; `SUPPORTED_JULIE_TOOLS = {"search","inspect","context","trace","impact","patterns"}`; scoring modes `path_top` (expected path is the first hit) and `path_top5`; per row record latency and response bytes for both products; output JSON and Markdown to `docs/eval/head-to-head/results/<UTC timestamp>.{json,md}` with a per-task-class table and a per-tool latency table. Keep `--validate`, `--self-check`, `--skip-miller`, `--skip-julie`, `--repos`, `--tasks`.
3. `cases.json`: 46 rows. The 23 search rows come from `scorecard.toml` (`julie: search query=<query> limit=5`, `miller: search query=<query> limit=5 format=json mode=auto`, `expected.path` = first `expected_any`, `scoring.mode = path_top5`, `task_class = retrieval.concept` or `retrieval.implementation` per the case's `category`). Add 23 more rows, one per search case, for the symbol named in the case's `intent` (`julie: inspect target=<Name> depth=summary`, `miller: inspect target=<Name> depth=summary format=json`, `scoring.mode = path_top`, `task_class = inspect.symbol`). Verify each expected path exists before committing (`--validate`).
4. `README.md`: how to run, how rows are scored, and the two statements from the frozen contract this run honors (cases were written before any candidate output was viewed; the manifest and results live under `docs/eval/`, which neither product indexes as a corpus because the corpus roots are the ten external repos).
5. Run: `python3 docs/eval/head-to-head/run_matrix.py --validate`, then the full run twice (`--out-dir` default), and keep the second pair (the first warms both indexes; say so in the finding). Total wall time under one hour.
6. `docs/findings/2026-09-1X-head-to-head-miller.md`: per task class top-1 and top-5 for both products, per tool p50 and p95 latency, response bytes, the rows where the products disagree with a one-line reason each (read the responses; do not guess), and an honest scope line: retrieval matrix only, one machine, fresh indexes, no agent episodes. No winner by construction: report the numbers and the trade-offs.

**Step 4: Run test to verify it passes**

Run: `python3 docs/eval/head-to-head/run_matrix.py --self-check`
Expected: `self-check ok`

Run: `python3 docs/eval/head-to-head/run_matrix.py --validate`
Expected: `46 rows valid`

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "docs(eval): head-to-head retrieval matrix against Miller on the fresh public corpus"` and record the SHA.

**Acceptance criteria:**
- [ ] `--self-check` and `--validate` pass; both result files exist and the finding cites them by path.
- [ ] Every Julie row calls one of the seven contract tools; no row calls a Miller `edit`, `content`, or `tests` tool.
- [ ] The finding reports both products' numbers per task class and per tool, names disagreement rows with reasons, and states the scope limits.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Task 9: Phase 6 gate finding

**Files:**
- Create: `docs/findings/2026-09-1X-machine-service-phase6-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`
- Modify: `docs/plans/2026-09-09-machine-service-design.md` section 14 item 6 (add `*Landed:*` with the commit range and the four deviations), `CLAUDE.md` and `AGENTS.md` "Last Updated" line and status
- Test: none new; the lead's gates

**Interfaces:**
- Consumes: the lead's gate results (`cargo xtask test dev`, `system`, `dogfood`, `full`, `fast` timing, `cargo test -p xtask --test docs_contract_tests`), `tokei` before and after, the head-to-head finding.
- Produces: the gate finding and the ledger.

**Contract inputs:** `docs/findings/2026-09-10-machine-service-phase3-gate.md` is the shape to follow; `docs/plans/verification-ledger-template.md` is the ledger shape.

**File ownership:** Create `docs/findings/2026-09-1X-machine-service-phase6-gate.md`, `docs/plans/2026-09-10-machine-service-phase6-ledger.md`. Modify `docs/plans/2026-09-09-machine-service-design.md`, `CLAUDE.md`, `AGENTS.md`.

**Serialization required:** Yes

**Dependency reason:** Measures the finished branch.

**Step 1: Collect the evidence**

The worker reports `STATUS ready for gates`. The lead runs, at the Task 8 commit, in this order and records each in the ledger: `cargo fmt --check`; `cargo clippy --workspace --all-targets`; `cargo xtask test dev`; `cargo xtask test system`; `cargo xtask test dogfood`; `cargo xtask test full`; `cargo xtask test fast` three times (median); `cargo test -p xtask --test docs_contract_tests`; `tokei --output json` (compare against the same command at the branch's merge base, `git merge-base main contract`); `julie-server service status` after `workspace open` on the julie and miller repos with semantics on (resident memory).

**Step 2: Write the finding**

Sections, in order: verdict (pass or fail); the seven tools with one line each; the four deviations from design section 9 and the owner decision date; gate table (each command, scope label, SHA, result); net lines table (before, after, delta, and the largest deletions); `fast` median and `full` wall; resident memory; head-to-head summary (numbers only, link to the Task 8 finding); telemetry check (`SELECT tool_name, COUNT(*) FROM tool_calls GROUP BY 1` after migration 008 shows only contract names plus the three retired edit names); follow-ups (plugin repo skill list and hooks, phase 7 CT, Rust revival harness).

**Step 3: Update the design and the two instruction files**

Design section 14 item 6: `*Landed:* commits <first>..<last> on branch contract. Deviations: no edit tool; no content tool (large-file skill); offset paging with a next: line; inspect defaults to summary.` `CLAUDE.md` and `AGENTS.md`: "Last Updated: 2026-09-1X | Status: Phase 6 contract (seven tools)".

**Step 4: Verify**

Run: `cargo test -p xtask --test docs_contract_tests 2>&1 | tail -5`
Expected: PASS (CLAUDE.md and AGENTS.md identical, names consistent).

**Step 5: Apply commit mode**

- `serial-worker-commit`: `git commit -m "docs(contract): phase 6 gate finding and verification ledger"` and record the SHA.

**Acceptance criteria:**
- [ ] The finding and ledger exist; every ledger row names a command, scope label, SHA, result, and timestamp.
- [ ] Net lines are negative against the merge base.
- [ ] `JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900 characters and `fast` median is under 10 s.
- [ ] Tests pass and the change is committed by the worker per commit mode.

---

## Verification Ledger

See `docs/plans/2026-09-10-machine-service-phase6-ledger.md` (created in Task 9).

## Open items the owner decides at approval

1. **Paging.** `spillover_get` was deleted in phase 2 and today's tools end with a one-line truncation notice. This plan restores paging as `offset` plus a `next:` line, which is stateless and cheap. If the truncation notice is enough, drop `offset` from Tasks 3 through 6 and keep `next_line` out.
2. **`inspect` default depth.** The plan defaults to `summary` (no body). Miller's telemetry shows agents ask for `full` 60% of the time; the guidance in Task 7 tells them to ask for `overview` only when they need the body. Julie's own `tool_calls` after two weeks will show whether that holds.
3. **Head-to-head scope.** Task 8 runs the retrieval matrix only. The agent-episode and Rust `xtask-eval revival` harness from `docs/plans/2026-09-07-julie-head-to-head-evaluation.md` stay deferred.
4. **Plugin repo.** `~/source/julie-plugin` skill list, hooks, and the `update-binaries.yml` skill loop are follow-ups after this branch merges; this plan does not touch that repo.
