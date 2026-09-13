# Plan 4: Installable releases and reliable agent guidance

**Status:** Local implementation, Linux qualification, and final repository gates are complete at `35a909eb`; external qualification remains pending for compatible upgrade, macOS/Windows, real-agent clients, CI dispatch, and publication. No publication has started or is authorized.
**Goal:** A normal user can install, get correct tool instructions, use lexical and semantic features, and upgrade with a predictable recovery path.
**Depends on:** Guidance/launcher tasks can start before Plans 1–3; native qualification uses their integrated release candidate.
**Execution:** Follow the [roadmap contract](2026-09-11-revival-roadmap.md). This plan spans Julie and `~/source/julie-plugin`; inspect and preserve both worktrees. Publication is a separate final approval boundary.

## Architecture quality

Keep the stdio shim, one machine service, native sidecar, and compiled instruction source. Install application binaries in immutable versioned directories. Retain explicit service restart on version mismatch; no automatic shutdown by every new client and no live handoff. Risks are high for Windows upgrades and release automation, medium for instruction propagation. Packaging checks operate on actual archives and fresh profiles, not only mocked launcher paths.

## Ownership and ordering

| Task | Julie ownership | Plugin ownership | Serialization / reason |
|---|---|---|---|
| 4A guidance | `JULIE_AGENT_INSTRUCTIONS.md`, `src/tools/workspace/commands/mod.rs`, `src/handler/tools/manage_workspace.rs`, `src/tests/request_engine.rs`, `xtask/tests/docs_contract_tests.rs`, existing affected `.claude/skills/*/SKILL.md`, `README.md`, `docs/OPERATIONS.md` | Mirrored instructions/skills; existing hook tests | First; defines install acceptance contract |
| 4B upgrades | `src/service/client.rs`, `src/main.rs` only if current restart is insufficient, `src/tests/service/client.rs`, `control.rs`, `xtask/src/dev_workflow.rs` including its inline unit tests, operations/release docs | `hooks/run.cjs`, `hooks/run.test.cjs`, `.gitignore`, `README.md` | After 4A; shared docs owned serially |
| 4C cold/offline | `crates/julie-pipeline/src/embeddings/native/launch.rs`, existing semantic runtime/store from Plan 1, `src/tests/integration/native_semantic_lifecycle.rs`, `src/tests/service/semantic_child.rs`, existing status output and operations docs | Documentation only | After Plan 1 and 4B by default; shared status/docs |
| 4D packaging | `.github/workflows/release.yml`, `.github/scripts/pack-release.sh`, proposed `.github/scripts/verify-release-archive.sh` only if shared checks justify a helper, existing release contract tests, release notes | `.github/workflows/update-binaries.yml`, `hooks/plugin-manifests.test.cjs` | After 4A–4C; packages final behavior |
| 4E qualification | Proposed `docs/findings/revival-install-qualification.md`, final `docs/release-notes/v8.0.0.md` and operations evidence | Final published-content checks | After integrated gates and four native artifacts |

Resolve exact test-file registrations before edits. No ownership of live `~/.codex` configuration, installed plugin caches, or the maintainer's `JULIE_HOME` is granted by this plan.

## 4A. Make the guidance contract consistent

**Inputs:** Compiled `AGENT_INSTRUCTIONS`, `mcp_tool` final schema rewrite, management operation routing, source instructions, and `sync-plugin`.
**Produces:** Consistent MCP initialization, schemas/examples, hooks, skills and installation docs.

Put these essentials in the first 512 characters of `JULIE_AGENT_INSTRUCTIONS.md`: scoped calls require an absolute path or registered ID in `workspace`; open the intended checkout by absolute path; start with search/context and inspect evidence before changing code. Preserve the current instruction-size budget. The full document remains authoritative; plugins repeat it rather than maintaining another routing text.

Nine scoped MCP schemas already require workspace in `src/handler/mcp_adapter.rs:255`; preserve and test that final wire shape. Fix the actual management gap: health examples must pass `workspace_id`, while global list/status need no selector. Remove retired `register`/`stats` descriptions. An unbound health error must name the exact corrective call. Do not add a large alternative schema if ordinary field docs/examples plus runtime validation suffice.

Replace unconditional “never re-verify” with a bounded tool-first fallback for unavailable, stale, truncated, or insufficient evidence. Keep search-before-new-code and inspect-before-edit guidance. Prose/literal searching should name the existing lexical option; do not make agents guess retrieval internals.

**Tests:** Extend `mcp_catalog_requires_a_non_null_workspace_for_scoped_tools` and add `manage_workspace_health_accepts_workspace_id_and_rejects_unbound_call_with_guidance` using `RequestFixture`. Run the proposed exact root-package test RED/GREEN. Extend `docs_contract_tests_agent_instructions_fit_the_server_instruction_budget` to assert the first-512 contract and preserve the existing full-guidance test. For xtask integration tests use `cargo nextest run -p xtask --test docs_contract_tests <exact_name>`, not an incorrect `--lib` target.

Run `cargo xtask sync-plugin --dry-run` against the selected plugin checkout before mirroring. Inspect its deletion list to avoid overwriting unrelated distribution work. Existing sync and hook tests must verify initialization/source/hook payload content and real management examples, not merely that a file exists.

**Acceptance:**

- [x] A fresh no-hook MCP client receives essential routing and succeeds on its first explicit-workspace call.
- [x] All management examples use valid current operations and selectors.
- [x] Source, compiled initialization and mirrored plugin guidance agree; generated tool schemas retain required workspace.
- [x] Every affected skill call includes the correct selector and preserves paging arguments.
- [x] Documentation uses the pinned extractor's verified language inventory, not stale unsupported counts.

## 4B. Make upgrades version-safe

**Inputs:** `findArchive`, `prepareBinaryForLaunch`, service discovery/authentication, current `service restart`, and maintainer `dev-link` behavior.
**Proposed install contract:** Extract each archive into `bin/<target>/versions/<archive-basename>/`. Never overwrite the directory of a running older executable. Required server, sidecar and package metadata must all be present before that version is launchable. Failed extraction refuses that version; it must not launch a different version as fallback. Archives retain their current flat contents; version directories are created only during installation.

Use a staging subdirectory and promote only a fully verified extraction. Do not trust directory existence after a partial extraction, and do not use unchecked archive paths or symlinks to escape the chosen destination. Reuse the existing extraction marker for completion if needed; no new daemon coordination is introduced. Existing old-version directories can remain until a measured cleanup need exists.

The launcher does not stop the shared service automatically. A new shim encountering an old service reports both versions and an exact command using the newly extracted binary: `service restart`, followed by restarting old harness clients. Inspect the current restart implementation first; if it already shuts down a mismatched service using discovery/token and starts the current executable, add tests/docs only.

Reserve direct `bin/<target>/julie-server` and `bin/<target>/julie-semantic-sidecar` symlinks for the maintainer override, with platform executable suffixes. The launcher accepts this override only when both paths are symlinks to valid binaries; stale direct regular files are not an override. Ordinary users launch the verified version directory. `split_binary_names` currently names only the server; update it and the existing dev-link discovery/test fixture to support both binaries and the versioned layout. Keep release directories immutable and do not put development links in archives or generated plugin snapshots.

Existing inline xtask tests are `dev_link_creates_split_binary_symlinks_for_existing_cache_dir`, `dev_link_ignores_archives_directory`, `dev_link_replaces_real_binary_with_symlink`, `dev_link_is_idempotent`, and `dev_link_dry_run_does_not_modify_filesystem`. Add `dev_link_creates_server_and_sidecar_override_for_versioned_layout`; run `cargo nextest run -p xtask --lib dev_link_creates_server_and_sidecar_override_for_versioned_layout` RED/GREEN. Update stale in-process/advisory-restart documentation in the same source module. No ordinary release installation points at `target/release`.

**Proposed Node tests:** `new archive extracts without overwriting running version`, `failed extraction never launches another archive version`, `partial extraction is never marked ready`, and `required sidecar is validated before launch`. Run each with `node --test --test-name-pattern='<exact name>' hooks/run.test.cjs` for RED/GREEN, then the full plugin gate.

**Proposed Rust test:** `restart_stops_mismatched_service_and_starts_current_binary`, through the existing service/CLI process fixture. It must execute the selected new binary, not only format a mismatch. Use root-package exact RED/GREEN; retain `version_mismatch_is_reported_with_both_versions` in `src/tests/service/client.rs` and `shutdown_stops_the_service_and_removes_the_record` in `src/tests/service/control.rs`.

**Acceptance:**

- [ ] Native Windows can hold the old executable open while the new archive extracts successfully elsewhere. Pending the final Plan 4E native qualification.
- [x] A corrupt/incomplete package fails with its actual extraction error and recovery action, never a misleading in-use explanation for every error.
- [x] Explicit restart stops the old service, waits for its exit, starts the selected version and leaves one service.
- [x] Surviving old shims receive an actionable incompatibility message; no automatic client termination or hidden downgrade occurs.
- [x] Maintainer dev-link remains tested and reversible.

## 4C. Qualify cold and offline semantics

**Inputs:** Native launch/prepare, current cache/model metadata, Plan 1's coverage policy, and existing mock-sidecar fixtures.
**Produces:** Lexical availability independent of model download and truthful semantic readiness across cold, offline and restart cases.

Use `compile_mock_sidecar`, `native_semantics_becomes_ready_without_client_restart`, `two_checkouts_share_one_embedding_child`, and `lexical_only_mode_never_spawns_the_child` as existing anchors. Add exact root-package tests `cold_model_prepare_does_not_block_lexical_search`, `offline_missing_model_degrades_with_actionable_status`, and `offline_warm_cache_reaches_semantic_ready`. Each is deterministic RED/GREEN with controlled prepare behavior; no network in unit tests.

Empty cache with network permits background preparation while lexical queries succeed. Empty cache offline preserves lexical service and reports model/cache/error/recovery; Required fails within its existing deadline. Warm cache offline reaches semantic readiness without network. Backfill resumes after restart, using Plan 1 coverage. Reuse existing in-flight preparation and retry state to avoid per-request launch/download storms.

The archive contains native binaries/libraries, not model weights. Document actual model size/cache location and the supported preparation mechanism after inspecting the pinned sidecar. State the plugin's Node prerequisite and the native archive alternative. Do not lower Node's declared minimum without compatibility evidence.

**Acceptance:**

- [x] Cold model work does not delay a lexical request behind download or provider initialization.
- [x] Offline empty cache gives a precise semantic limitation while ordinary navigation works.
- [x] Offline warm cache needs no network and reaches complete compatible coverage.
- [x] Repeated requests do not spawn multiple child processes or independent model preparations.
- [x] MCP error shape and terminal Required exit code are consistent; HTTP status is not confused with CLI exit code 4.
- [x] Real-sidecar cold/warm offline probes supplement mocks before qualification.

## 4D. Make release verification executable

Each native build job verifies its own archive before upload: expected binary names, sidecar libraries and metadata, valid paths/checksums, server version equal to the requested tag, sidecar executable identity, and a no-hook stdio initialize with nonempty correct instructions. Execute packaged files from an extracted temporary directory, not build-tree binaries. Runtime probing uses isolated homes and leaves no service process behind.

Replace the echo-only post-release step with downloaded-public-asset inspection. Enumerate all four required platform archives and checksums. Use an awaited reusable workflow rather than dispatch/poll correlation: add `workflow_call` alongside manual `workflow_dispatch` in the plugin workflow, with version, tag, reviewed `plugin_source_sha`, and the named publication-token secret. Julie calls that cross-repository workflow pinned to the full reviewed plugin commit SHA and passes the same SHA as input. The called workflow explicitly checks out `anortham/julie-plugin` at that SHA; an unqualified checkout inside a reused workflow would check out the caller repository. A failed child job must fail the caller job graph.

These semantics are verified in GitHub's [reusable workflow guide](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows) and [workflow reuse reference](https://docs.github.com/en/actions/concepts/workflows-and-actions/reusing-workflow-configurations). Use the exact committed plugin SHA, not `main`, and verify the required token permissions before publication. No polling supervisor or optional CLI run-URL parsing is needed.

Retain the pinned workflow source commit under an immutable source tag before the plugin's orphan-main update replaces branch history. Creating that tag is part of the owner-approved publication step. A rerun must still be able to resolve its reviewed reusable workflow after newer distribution snapshots ship.

After downstream success, inspect the public plugin commit's version manifests, four archives, MCP declarations, instructions and hooks, and run its Node tests from the downloaded checkout. A missing/mismatched artifact or failed downstream update fails the overall release verification. Preserve substantive versioned release notes.

**Acceptance:**

- [x] Intentionally missing sidecar, mismatched version, malformed manifest and partial platform set each fail qualification tests.
- [ ] All native jobs execute their actual packaged server and prove instruction delivery.
- [ ] The parent awaits the exact reusable workflow job, propagates its failure and exposes its run evidence.
- [ ] Archives and generated plugin source contain no maintainer override symlinks; all four flat archive layouts remain valid.
- [x] Release notes identify verified platforms, semantic startup, upgrade steps and any remaining honest limitations.

## 4E. Run the installation and agent-use matrix

Produce a qualification table for Linux x64, macOS arm64, macOS Intel and Windows x64. Use fresh user profiles/homes; built packages must not discover maintainer caches. The current `win-test` helper allowlists only Miller and julie-extractors, so it cannot run Julie today. Use a nonpublishing GitHub `windows-latest` artifact/qualification job for the real executable-lock test. Adding Julie to the user-level guest helper would be a separate tooling task, not an assumed capability. Missing native/client access stays an explicit incomplete gate.

Use existing authorized CI capacity. If executing a native job requires publishing source first or incurs new charges, finish local artifacts/checks and obtain the smallest required push/spend authorization; do not infer it from “nonpublishing” in the workflow name.

Manual stdio startup, package extraction, shared-service reuse, restart and cached/offline semantics are required on every native target. Test Claude Code and Codex plugin installs on each supported native target where the client is available. Exercise Antigravity, OpenCode, Hermes and Cursor through their documented supported installation on at least one real supported host each. Record host/client versions and mark unsupported combinations explicitly rather than implying universal support.

For each advertised install path, ask a real agent to orient in a fixture checkout, inspect a symbol, request a second page and state the workspace. Capture initialization, available tool schemas, hook/subagent guidance where supported, and actual tool calls. Assert it selects Julie, passes selectors and follows the complete next-page call. A mock MCP client proves protocol delivery; it does not prove model adoption.

Official [Codex MCP guidance](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), verified during the assessment, confirms initialization instructions and the self-contained first-512 recommendation. Recheck official harness documentation at execution if versions or installation formats have changed.

**Acceptance:**

- [ ] All advertised native archives have fresh install and upgrade evidence, including the real Windows lock case. External qualification pending.
- [ ] Every advertised client route has recorded instruction delivery and an actual successful agent workflow. External qualification pending.
- [x] Linux fresh-profile second session shares the service without selecting a workspace from plugin launch cwd.
- [ ] Configuration removal/uninstall leaves the client usable and documents shared-service/cache ownership. External qualification pending.
- [x] A local release-candidate report lists exact SHA, versions, archive checksums, commands and unresolved gates.

## Publication order and authority

Implementation ends with reviewable commits and local/native evidence. Before publication, inventory all related worktrees and ensure intentional clean source states. The public plugin repository currently uses orphan history; do not perform an ordinary merge or force-push based only on ahead/behind counts.

Owner-approved publication order is: publish the reviewed v8 plugin launcher/workflow source; verify that exact public layout; publish/tag Julie's verified source; await server artifacts and the matching plugin update; perform fresh public-install smoke checks. Each push, history rewrite, tag and release needs existing explicit authority for the exact state. Do not tag early merely to obtain test artifacts; use local archives or a nonpublishing build-artifact workflow first.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Health requires a checkout selector and gives the exact open recovery call | `cargo nextest run -p julie --lib manage_workspace_health_accepts_workspace_id_and_rejects_unbound_call_with_guidance` | worker-red-green | `9bc5339f` | PASS: 1 passed | 2026-09-12T22:00Z | Worker ran the identical pre-commit tree. |
| Nine scoped MCP schemas require a non-null string workspace | `cargo nextest run -p julie --lib mcp_catalog_requires_a_non_null_workspace_for_scoped_tools` | worker-red-green | `9bc5339f` | PASS: 1 passed | 2026-09-12T22:00Z | Worker ran the identical pre-commit tree. |
| Compiled initialization fits the budget and carries the first-512 routing contract | `cargo nextest run -p xtask --test docs_contract_tests docs_contract_tests_agent_instructions_fit_the_server_instruction_budget` | worker-red-green | `9bc5339f` | PASS: 1 passed | 2026-09-12T22:00Z | Worker ran the identical pre-commit tree. |
| Deployment guidance matches selectors, skills and the pinned 37-language inventory | `cargo nextest run -p xtask --test docs_contract_tests docs_contract_tests_deployment_guidance_matches_runtime_contract` | worker-red-green | `9bc5339f` | PASS: 1 passed | 2026-09-12T22:22Z | Worker ran the identical pre-commit tree after lead corrections. |
| Mirrored plugin instructions and hooks remain valid | `node --test hooks/*.test.cjs` | dev | `1aa320a` | PASS: 22 passed | 2026-09-12T22:23Z | Lead ran the complete plugin gate. |
| Source skills and instructions exactly match the selected plugin mirror | `cargo xtask sync-plugin --plugin-root /home/murphy/.config/razorback/worktrees/julie-plugin/revival-deployment --dry-run` | dev | `9bc5339f` / `1aa320a` | PASS: 0 updates, 0 removals | 2026-09-12T22:23Z | None. |
| Selected archives install immutably, reject unsafe/incomplete packages, never downgrade, and reuse only a matching ready version | `node --test --test-name-pattern='new archive extracts without overwriting running version|failed extraction never launches another archive version|partial extraction is never marked ready|required sidecar is validated before launch|archive traversal is rejected before extraction|symlink package entries are rejected before ready|maintainer override rejects directory symlink targets' hooks/run.test.cjs` | worker-red-green | `cdc0690` | PASS: 7 passed, 0 skipped | 2026-09-12T22:52Z | Worker ran the identical pre-commit tree. |
| Plugin launcher and hooks remain integrated after versioned installation changes | `node --test hooks/*.test.cjs` | dev | `cdc0690` | PASS: 29 passed | 2026-09-12T22:54Z | Lead ran the identical pre-commit tree. |
| Dev-link creates the paired override for completed versioned layouts | `cargo nextest run -p xtask --lib dev_link_creates_server_and_sidecar_override_for_versioned_layout` | worker-red-green | `a8a64b33` | PASS: 1 passed | 2026-09-12T22:44Z | Worker ran the identical pre-commit tree. |
| Dev-restart reports the shared-service restart procedure | `cargo nextest run -p xtask --lib dev_restart_reports_shared_service_restart` | worker-red-green | `a8a64b33` | PASS: 1 passed | 2026-09-12T22:47Z | Worker ran the identical pre-commit tree. |
| Explicit restart replaces a mismatched service with the current executable and reaps the old process | `cargo nextest run -p julie --lib restart_stops_mismatched_service_and_starts_current_binary` | worker-red-green | `a8a64b33` | PASS: 1 passed | 2026-09-12T22:52Z | Worker ran the identical pre-commit tree after a plan-required debug fixture build. |
| Version mismatch reports both versions, the quoted current-binary restart command, and old-client recovery | `cargo nextest run -p julie --lib version_mismatch_exits_3_with_the_exact_message` | worker-red-green | `a8a64b33` | PASS: 1 passed | 2026-09-12T22:52Z | Worker ran the identical pre-commit tree. |
| Cold preparation returns a real lexical result without waiting and shares one prepare attempt | `cargo nextest run --lib cold_model_prepare_does_not_block_lexical_search` | worker-red-green | `ebe16b9a` | PASS: 1 passed | 2026-09-12T23:12Z | Controlled mock sidecar; Auto, Off, and provider-none paths have no extra preparation. |
| Offline missing model preserves actionable semantic status and retains Auto retry state | `cargo nextest run --lib offline_missing_model_degrades_with_actionable_status` | worker-red-green | `ebe16b9a` | PASS: 1 passed | 2026-09-12T23:12Z | Controlled offline prepare failure reports model, cache, root cause, and recovery. |
| Warm offline cache starts the compatible sidecar without preparation | `cargo nextest run --lib offline_warm_cache_reaches_semantic_ready` | worker-red-green | `ebe16b9a` | PASS: 1 passed | 2026-09-12T23:12Z | Controlled warm cache reaches semantic readiness without network I/O. |
| Real sidecar cold/warm offline behavior | Isolated sidecar 0.1.0 probe with blocked proxies | live | `ebe16b9a` | PASS: cold exit 1; warm ready CPU/384d | 2026-09-12T23:12Z | Scratch cache only; warm checksum `bf40c42a...`; no rebuild, network, or live cache. Existing `native_semantics_becomes_ready_without_client_restart` and Plan 1 persisted-coverage tests cover restart/backfill. |
| Release archive qualifier rejects missing sidecar, exact version mismatch, malformed/wrong-checksum manifests, invalid JSON-RPC initialization, and partial/wrong-checksum public assets | `cargo nextest run -p xtask --test toolchain_contract_tests release_qualification_rejects_invalid_archives_and_partial_public_assets` | worker-red-green | `87f333dd` | PASS: 1 passed | 2026-09-12T23:47Z | Offline synthetic archives exercise every negative branch and require valid JSON-RPC with nonempty workspace guidance; GitHub-hosted native execution remains pending. |
| Pinned reusable-workflow release contract and release-note sections | `cargo nextest run -p xtask --test toolchain_contract_tests release_workflow_qualifies_archives_and_awaits_the_pinned_plugin_workflow` | worker-red-green | `67242b1f` / `7d3996a7` | PASS: 1 passed | 2026-09-12T23:41Z | Confirms the immutable plugin SHA, Node prerequisite, explicit plugin test glob, and required notes headings/platform names. |
| Plugin reusable-workflow manifest contract | `node --test hooks/plugin-manifests.test.cjs` | worker-red-green | `7d3996a7` | PASS: 2 passed | 2026-09-12T23:41Z | Covers `workflow_call`, source pin, Node setup, exact archive names, and generated-snapshot guards. |
| Windows locked-executable extraction | Nonpublishing `windows-latest` qualification job in Plan 4E | full | `87f333dd` | PENDING | — | Local workflow is prepared; native Windows execution needs explicit push and runner-capacity authorization. |
| Nonpublishing Windows qualification workflow contract | `cargo nextest run -p xtask native_qualification_workflow_is_nonpublishing_and_exercises_windows_lock` | worker-red-green | `87f333dd` | PASS locally: workflow contract; native Windows execution PENDING | 2026-09-12T23:48Z | Requires checksum-bound archive verification, a fresh profile, running old packaged shim during separate-version extraction, cleanup, and attributable candidate/archive hashes. |
| Linux x64 current package and archive verifier | Repackage unchanged c487 binary with `pack-release.sh`, checksum, then `verify-release-archive.py` | live | binary `c487f43a`; wrapper/verifier `a90720f0` | PASS | 2026-09-13T01:35Z | Current archive SHA-256 `e9686f4e...`; top-level package identity and archive verifier pass. |
| Linux x64 fresh-profile shim, restart, and cached sidecar | Packaged initialize twice from distinct cwd; `service restart`; blocked-proxy cached-model prepare, daemon index, health poll, and required semantic search | live | `c487f43a` | PASS: shim reuse/restart/cleanup and full required semantic coverage | 2026-09-13T01:25Z | One-symbol scratch workspace reached one vector and a semantic hit. Block only HTTPS for this probe: HTTP/ALL proxying intercepts the shim's localhost service client. All scratch paths were deleted. |
| Linux x64 plugin fresh install | Disposable copy of plugin `7d3996a7` with only the current a907-wrapped c487 archive; `node hooks/run.cjs` initialize plus `manage_workspace status` | live | binary `c487f43a`; wrapper `a90720f0`; plugin `7d3996a7` | PASS: versioned extraction, initialization, management call, and packaged cleanup | 2026-09-13T01:35Z | No maintainer symlinks, cache, or profile were used. Real v7.18.0 archive SHA `0c7b3688...` has prior embedding-host layout; no compatible prior versioned v8 archive exists for upgrade proof. |
| Pre-qualification Plan 4 branch gate | `cargo xtask test full` | full | `c487f43a` | PASS: 5/5 phases; 2,303 workspace, 13 ignored CLI, fixture prepared, 69 dogfood | 2026-09-13T01:16Z | Subsequent verifier and manifest changes require a final frozen-tree gate. |
| Final frozen-tree Plan 4 repository gates | `cargo fmt --check`; `cargo clippy --workspace --all-targets`; `cargo xtask test full` | full | `35a909eb` | PASS: formatting, clippy, and full 5/5; 2,303 workspace, 13 ignored CLI, fixture ready, 69 dogfood | 2026-09-13T01:42Z | Final local repository gate. Plan 4E native/client acceptance rows remain independently pending. |
| Full Plan 4 branch gate | `cargo xtask test full` | full | `cbf0c743` | FAIL after 1,126 passes: `test_manage_workspace_health_cold_start_returns_index_first_guidance` and `test_manage_workspace_health_reports_initialized_when_not_degraded` omit Plan 4A-required workspace routing | 2026-09-12T23:56Z | Current-diff Plan 4A scope. This was the single permitted broad-gate retry; a third full run requires explicit owner decision. |
| Owner-authorized third Plan 4 branch gate | `cargo xtask test full` | full | `bd598299` | FAIL after 1,128 passes: `test_manage_workspace_health_reports_not_initialized_when_runtime_status_missing` and `test_manage_workspace_health_reports_unavailable_when_provider_missing` omit Plan 4A-required workspace routing | 2026-09-13T00:00Z | Current-diff Plan 4A scope. Read-only inventory found four further unbound health callers: part2 `test_manage_workspace_health_surfaces_embedding_runtime_status`, part2 `test_manage_workspace_health_reports_control_data_and_runtime_planes`, part3 `test_manage_workspace_health_uses_rebound_session_primary`, and `semantic_request_contract::adversarial_request_engine_dispatch_integration`. A fourth full run requires explicit owner approval. |
| Owner-authorized fourth Plan 4 branch gate | `cargo xtask test full` | full | `891e43b6` | FAIL after 2,259 passes: `dev_link_dry_run_does_not_modify_filesystem`, `dev_link_creates_split_binary_symlinks_for_existing_cache_dir`, `dev_link_ignores_archives_directory`, `dev_link_is_idempotent`, and `dev_link_replaces_real_binary_with_symlink` retain server-only expected counts after paired server/sidecar linking | 2026-09-13T00:00Z | Read-only diagnosis verified production `run_dev_link` already loops over both binaries. Complete follow-up is limited to the five stale test assertions/loops; a fifth full run requires explicit owner approval. |
