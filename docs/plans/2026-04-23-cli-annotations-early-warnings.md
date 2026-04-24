# Julie Feature Expansion: CLI, Annotation Normalization, and Early Warning Signals

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Add a CLI that exposes Julie tools for shell workflows, autonomous agent loops, and CI automation; normalize decorators, annotations, and attributes into first-class symbol data; and use that data to improve test understanding and surface structural early-warning signals.

**Architecture:** The CLI must reuse Julie's existing tool structs and routing semantics, not create a second behavior stack. Annotation normalization creates one canonical marker layer across Python decorators, Java annotations, C# attributes, Rust attributes, and similar syntax in other languages, while preserving raw syntax for debugging and future UI. The reporting layer consumes normalized markers plus existing structural data such as identifiers, relationships, centrality, and test linkage to produce signal-oriented output. It does not claim taint tracking, exploit reachability proof, or exhaustive language security coverage.

**Tech Stack:** Rust, clap, serde_json, Axum, Tera, htmx, tree-sitter, Tantivy, existing Julie handler and adapter infrastructure

---

## Product Framing

### Why The CLI Matters

The CLI is useful on its own.

- It clears the current change code, rebuild, restart, test loop bottleneck by letting developers and agents run Julie tools from the shell against a debug build without going through a full MCP client restart path for each check.
- It gives autonomous agents a stable shell surface for local validation, fixture-driven checks, and comparison runs during implementation.
- It opens a clean automation path for CI, cron jobs, repository audits, and pipeline steps that need machine-readable JSON or Markdown output.
- It makes dogfooding stronger because Julie becomes usable both as an MCP server and as a direct terminal tool.

The catch is that this only pays off if the CLI reuses the same tool behavior. If the CLI forks request parsing, workspace routing, or output semantics, it becomes a drift factory.

### Why Annotation Normalization Matters

Normalized annotation markers are a core data layer, not a side quest.

- They improve test identification across frameworks that rely on annotations or attributes instead of file naming alone.
- They give Julie a cleaner understanding of endpoints, auth guards, middleware, fixtures, setup hooks, schedulers, and framework entry points.
- They support future ranking and report features without forcing path-based heuristics.

### Non-Goals

- Do not position this work as full security intelligence.
- Do not claim taint analysis, exploitability proof, or end-to-end vulnerability detection.
- Do not require exhaustive rule coverage across every embedded language in the first pass.
- Do not build a second transport stack for the CLI outside Julie's existing adapter and handler model.

---

## Plan A: CLI Interface For Agents, Developers, And Automation (Sessions 1-2)

### File Structure

```
src/
├── cli.rs                    # MODIFY: add CLI commands and global output flags
├── main.rs                   # MODIFY: route CLI commands to cli_tools
├── cli_tools/
│   ├── mod.rs                # CREATE: entry point and execution core
│   ├── subcommands.rs        # CREATE: clap definitions for named wrappers
│   ├── generic.rs            # CREATE: generic `tool <name> --params ...` execution
│   ├── daemon.rs             # CREATE: shared IPC client helper for CLI mode
│   └── output.rs             # CREATE: text, JSON, Markdown output
└── tests/
    └── cli/
        └── mod.rs            # CREATE: CLI integration tests
```

### Task A1: Define The CLI Surface

**Files:**
- Modify: `src/cli.rs`
- Create: `src/cli_tools/subcommands.rs`

**What to build:** Add a shell-first command surface for Julie tools. The generic tool path is the foundation. Named wrappers exist for the highest-frequency tools that agents and humans will call during local loops.

**Approach:**
- Add global flags to `Cli`: `--workspace`, `--json`, `--format <text|json|markdown>`, `--standalone`
- Add a generic top-level command:
  - `julie-server tool <name> --params '{"symbol":"main"}'`
- Add named wrappers for high-value tools:
  - `search`
  - `refs`
  - `symbols`
  - `context`
  - `blast-radius`
  - `workspace`
- Keep named wrappers thin. They are ergonomic aliases over the same underlying tool structs and output pipeline.
- Prefer top-level named commands plus the generic fallback. This matches the intended developer UX and keeps the generic command available for newer tools before a wrapper lands.

**Acceptance criteria:**
- [ ] `julie-server tool --help` shows `name` (positional) and `--params`
- [ ] `julie-server search --help` shows query (positional), `--target`, `--limit`, `--language`, `--file-pattern`, `--context-lines`, `--exclude-tests`
- [ ] `julie-server refs --help` shows symbol (positional), `--kind`, `--file-path`, `--file-pattern`, `--limit`
- [ ] `julie-server symbols --help` shows file_path (positional), `--mode`, `--target`, `--limit`, `--max-depth`
- [ ] `julie-server context --help` shows query (positional), `--budget`, `--max-hops`, `--entry-symbols`, `--prefer-tests`
- [ ] `julie-server blast-radius --help` shows `--rev`, `--files`, `--symbols`, `--format`
- [ ] `julie-server workspace --help` shows operation (positional), `--path`, `--force`, `--name`
- [ ] `julie-server --help` shows lifecycle commands (daemon, stop, status, restart) alongside tool commands
- [ ] Clap parsing is covered by unit tests for each subcommand

---

### Task A2: Build A Reusable CLI Execution Core

**Files:**
- Create: `src/cli_tools/mod.rs`
- Create: `src/cli_tools/daemon.rs`
- Modify: `src/main.rs`

**What to build:** A single execution path for CLI tool invocations with two modes: daemon-connected and standalone.

**Approach:**
- Add `pub async fn run_cli_tool(command, cli_flags) -> Result<()>` as the CLI entry point.
- **Daemon mode (default):**
  - Reuse the adapter and IPC handshake model.
  - Extract a small shared helper for CLI use from the existing adapter path, or build a tiny daemon client on top of the same `daemon_ipc_addr` and handshake rules.
  - Do not read the dashboard port file and do not invent a parallel transport.
- **Standalone mode (`--standalone`):**
  - Add a production constructor such as `JulieServerHandler::new_standalone(root: PathBuf)` or an equivalent helper. Do not build CLI behavior on `new_for_test()`.
  - Standalone mode owns its local workspace state and can initialize or refresh local `.julie/` data as needed.
  - Print mode and workspace diagnostics to stderr so shell pipelines stay clean.
- Reuse `resolve_workspace_root()` for workspace selection.
- Keep the execution contract the same in both modes: parse CLI args into a tool struct, call the tool, format the `CallToolResult`.

**Why this matters for the agent loop:**
- Agents can run a debug binary from the shell without restarting the MCP client to smoke-test a tool behavior.
- CI can call the same binary with `--json` or `--format markdown`.
- Humans get one command surface for both local experiments and automation.

**Acceptance criteria:**
- [ ] Daemon mode uses the IPC endpoint and existing handshake rules (not dashboard port)
- [ ] Standalone mode uses a production constructor (not `new_for_test()`)
- [ ] Both modes run the same tool structs and formatting pipeline (no forked behavior)
- [ ] `julie-server search "test" --standalone` against a workspace with no daemon running succeeds
- [ ] `julie-server search "test"` with daemon running routes through IPC
- [ ] Missing `.julie/` workspace produces: "workspace not indexed" with actionable guidance
- [ ] IPC connection failure falls back to standalone with a stderr warning
- [ ] Diagnostics (mode, workspace path, elapsed time) go to stderr, not stdout

---

### Task A3: Wire Named Wrappers And Generic Tool Execution

**Files:**
- Modify: `src/cli_tools/mod.rs`
- Create: `src/cli_tools/generic.rs`

**What to build:** Thin wrapper commands plus one generic executor for any registered tool.

**Approach:**
- For named commands, add `into_tool_struct()` conversions from clap args into the existing tool structs.
- Use `.call_tool(&handler)` as the common execution path. Do not special-case `execute_with_trace()` outside tools that need trace access for formatting or diagnostics.
- For the generic command, add a registry or match-based dispatcher that:
  - maps tool names to the tool struct type
  - deserializes `--params` JSON into that type
  - calls the tool through the shared execution path
- Cover all public tools through the generic path, including newer tools that do not yet have a named wrapper.

**Acceptance criteria:**
- [ ] `julie-server tool deep_dive --params '{"symbol":"main"}' --standalone` returns symbol investigation
- [ ] `julie-server tool rename_symbol --params '{"old_name":"foo","new_name":"bar","dry_run":true}' --standalone` returns dry-run preview
- [ ] `julie-server search "workspace" --standalone` returns results identical in structure to MCP `fast_search`
- [ ] `julie-server refs Symbol --standalone` returns reference locations
- [ ] `julie-server symbols src/main.rs --standalone` returns symbol listing
- [ ] `julie-server context "auth middleware" --standalone` returns context subgraph
- [ ] `julie-server blast-radius --rev HEAD~1..HEAD --standalone` returns impact analysis
- [ ] `julie-server workspace --operation stats --standalone` returns workspace stats
- [ ] All 12 MCP tools are reachable through the generic path
- [ ] Invalid tool name `julie-server tool nonexistent` lists all available tool names
- [ ] Invalid JSON `julie-server tool fast_search --params '{bad'` returns a clear parse error

---

### Task A4: Output Formatting And Exit Semantics

**Files:**
- Create: `src/cli_tools/output.rs`

**What to build:** Stable terminal and machine-readable output for local loops and automation.

**Approach:**
- Default text output prints the tool text payload as-is unless a wrapper has a small ergonomic formatter that improves terminal readability.
- `--json` serializes the full `CallToolResult`.
- `--format markdown` renders report-style output for CI artifacts, comments, and docs.
- Diagnostics such as mode, elapsed time, and workspace route go to stderr.
- Keep exit codes conventional:
  - `0` for success
  - non-zero for transport, parse, or tool execution failure

**Acceptance criteria:**
- [ ] `julie-server search "main" --standalone --json | jq .` produces valid JSON
- [ ] `julie-server search "main" --standalone --format markdown` produces fenced code blocks with headers
- [ ] Default text output prints tool payload to stdout with no diagnostic noise
- [ ] Timing and workspace info go to stderr only
- [ ] Exit code 0 on success, non-zero on transport/parse/tool failure
- [ ] `echo $?` after a failing command returns non-zero

---

### Task A5: End-To-End CLI Tests

**Files:**
- Create: `src/tests/cli/mod.rs`
- Modify: `src/main.rs`

**What to build:** Integration tests that prove the CLI is useful for autonomous agent work and automation.

**Approach:**
- Use `std::process::Command` to invoke the built binary.
- Cover standalone runs against a temp workspace and daemon-connected runs where practical.
- Add at least one test that exercises the generic tool path and one named wrapper path.
- Add a smoke test for JSON output and a smoke test for Markdown output.

**Acceptance criteria:**
- [ ] `cargo build && ./target/debug/julie-server tool fast_search --params '{"query":"fn main"}' --workspace . --standalone` works while an MCP session is active (no binary lock conflict)
- [ ] `./target/debug/julie-server search "fn main" --workspace . --standalone --json | jq '.content'` returns parseable JSON
- [ ] `./target/debug/julie-server --help` shows lifecycle commands plus all CLI tool commands
- [ ] At least one integration test exercises the generic tool path end-to-end
- [ ] At least one integration test exercises a named wrapper end-to-end
- [ ] At least one test verifies JSON output structure
- [ ] Integration tests are in the `cli` test bucket for xtask routing

---

## Plan B: Annotation Normalization As A First-Class Data Layer (Sessions 3-4)

### File Structure

```
crates/julie-extractors/src/
├── base/
│   ├── types.rs                # MODIFY: add canonical annotation field to Symbol
│   └── annotations.rs          # CREATE: shared normalization helpers
├── python/
│   └── decorators.rs           # MODIFY: output canonical markers
├── typescript/
│   └── helpers.rs              # MODIFY: canonicalize existing decorator extraction
├── java/
│   └── methods.rs              # MODIFY: persist canonical annotations, not test-only locals
├── csharp/
│   ├── helpers.rs              # MODIFY: parse attribute lists into canonical markers
│   └── members.rs              # MODIFY: pass canonical markers to symbol creation and test detection
├── rust/
│   └── helpers.rs              # MODIFY: normalize attribute extraction
└── kotlin/
    └── helpers.rs              # MODIFY: normalize annotation extraction

src/
├── search/
│   ├── language_config.rs      # MODIFY: add annotation and signal config sections
│   └── indexer.rs              # MODIFY: index canonical annotation markers
├── database/
│   ├── schema.rs               # MODIFY: add annotation storage
│   └── queries.rs              # MODIFY: store and retrieve annotations
└── analysis/
    └── security_risk.rs        # MODIFY: consume normalized markers as structural signals
```

### Task B1: Define The Canonical Annotation Contract

**Files:**
- Modify: `crates/julie-extractors/src/base/types.rs`
- Create: `crates/julie-extractors/src/base/annotations.rs`

**What to build:** One canonical annotation marker layer across syntax forms.

**Approach:**
- Use **annotation markers** as the umbrella term for decorators, annotations, and attributes.
- Add a first-class canonical field to `Symbol`, for example `annotations: Vec<String>`.
- Preserve raw syntax in metadata, for example `metadata["raw_annotations"]`, so debugging and UI work can show what the source wrote.
- Define normalization rules once in shared helpers:
  - remove syntax wrappers such as `@`, `[]`, and `#[]`
  - remove argument lists and constructor args for canonical matching
  - preserve qualified names such as `app.route`, `tokio::test`, and `SpringBootTest`
  - preserve declaration order
  - dedupe repeated markers
  - do not lowercase stored display values; derive case-folded match keys at comparison time where needed
- Define the C# rule up front:
  - canonical matching strips an optional `Attribute` suffix from the match key, while raw syntax remains preserved

**Acceptance criteria:**
- [ ] `Symbol` struct has `pub annotations: Vec<String>` (or equivalent canonical field)
- [ ] `#[serde(default, skip_serializing_if = "Vec::is_empty")]` keeps serialization clean for symbols without annotations
- [ ] Raw syntax stored in `metadata["raw_annotations"]` as `Vec<String>` preserving original text
- [ ] Normalization helper: `@app.route("/api")` → canonical `"app.route"`, raw `"@app.route(\"/api\")"`
- [ ] Normalization helper: `[Authorize]` → canonical `"Authorize"`, raw `"[Authorize]"`
- [ ] Normalization helper: `#[tokio::test]` → canonical `"tokio::test"`, raw `"#[tokio::test]"`
- [ ] C# suffix rule: `[TestMethodAttribute]` → canonical match key `"TestMethod"`, display `"TestMethodAttribute"`
- [ ] Dedup: repeated markers on one symbol are stored once
- [ ] Declaration order preserved in the Vec
- [ ] Unit tests cover each normalization rule independently

---

### Task B2: Normalize Existing Extraction Instead Of Rewriting It

**Files:**
- Modify: Python, TypeScript, Java, C#, Rust, and Kotlin extractor helpers and symbol builders listed above

**What to build:** A rollout that acknowledges the current extractor state and turns scattered annotation handling into one consistent signal layer.

**Current state to absorb:**
- Python already extracts decorators and stores them in metadata.
- TypeScript already extracts decorators and injects them into signatures in places.
- Java already collects annotations for test detection.
- C# already captures attribute list text for test detection.
- Rust already walks preceding attributes.
- Kotlin already sees annotations inside modifier lists.

**Approach:**
- **Python:** move existing decorator output into the canonical field and keep raw syntax in metadata.
- **TypeScript:** strip the `@` from the canonical field, stop relying on signature prefixes as the only persistence path, and pass canonical markers into test detection.
- **Java:** persist canonical annotation markers from modifier parsing, not transient local vectors only.
- **C#:** parse attribute list text into canonical markers, strip brackets, normalize optional `Attribute` suffixes for matching, and preserve raw list text.
- **Rust:** normalize attribute names such as `test`, `tokio::test`, and route-like macros into canonical markers; preserve any richer raw attribute text for future work.
- **Kotlin:** normalize `@Annotation(...)` style modifiers into canonical markers and preserve raw syntax.

**Language rollout scope:**
The first pass covers the 6 languages with existing partial extraction. The remaining 28 languages are planned per-language during session-level implementation planning. Languages without decorator/annotation/attribute syntax (JSON, TOML, YAML, Markdown, CSS, HTML, SQL, Regex, Bash, PowerShell) get no-op handling. Languages with annotation syntax but lower priority (PHP, Ruby, Swift, Go, Dart, etc.) are queued for a follow-up pass after the first 6 prove the pattern.

**Acceptance criteria:**
- [ ] Python: `@app.route("/api")` → canonical `["app.route"]`, raw preserved
- [ ] Python: multiple decorators on one function all captured in order
- [ ] TypeScript: `@Injectable()` → canonical `["Injectable"]`
- [ ] TypeScript: test detection receives canonical markers (not empty arrays)
- [ ] Java: `@GetMapping("/api")` → canonical `["GetMapping"]`
- [ ] Java: `@Override` (marker annotation, no args) → canonical `["Override"]`
- [ ] C#: `[HttpGet("api")]` → canonical `["HttpGet"]`, match key strips `Attribute` suffix
- [ ] C#: `[Authorize, Route("api")]` → canonical `["Authorize", "Route"]` (multi-attribute list)
- [ ] Rust: `#[tokio::test]` → canonical `["tokio::test"]`
- [ ] Rust: `#[derive(Debug, Clone)]` → canonical `["derive"]` (derive is the attribute; args are metadata)
- [ ] Kotlin: `@RequestMapping("/api")` → canonical `["RequestMapping"]`
- [ ] Existing extractor tests for all 6 languages pass without regression
- [ ] New SOURCE/CONTROL fixtures in `fixtures/` for each language's annotation extraction
- [ ] Cross-language test: parameterized, qualified, repeated, multi-marker cases

---

### Task B3: Persist And Index Canonical Annotation Markers

**Files:**
- Modify: `src/database/schema.rs`
- Modify: `src/database/queries.rs`
- Modify: `src/search/indexer.rs`

**What to build:** Storage and search for canonical annotation markers.

**Approach:**
- Add annotation storage to the symbols data model.
- Keep canonical markers directly queryable from the symbol record.
- Index canonical markers in Tantivy as exact-ish code identifiers, not stemmed prose.
- Add light query support for annotation-oriented lookup:
  - `@foo` in definition search can target annotation markers
  - a dedicated explicit target can wait if the shorthand is sufficient

**Acceptance criteria:**
- [ ] Canonical markers survive the full extract → persist → load → serialize roundtrip
- [ ] Database migration adds annotation storage; existing workspaces migrate cleanly
- [ ] Reindexing populates annotation markers in both SQLite and Tantivy
- [ ] `fast_search("@app.route", search_target="definitions")` finds Python Flask handlers
- [ ] `fast_search("@Test", search_target="definitions")` finds Java test methods
- [ ] `fast_search("@Authorize", search_target="definitions")` finds C# attributed members
- [ ] Normal definition search for `"MyClass"` is unaffected by annotation indexing
- [ ] Tantivy tokenizes annotation markers as exact keywords (not stemmed, not camelCase-split)

---

### Task B4: Improve Test Understanding With Canonical Markers

**Files:**
- Modify: `crates/julie-extractors/src/test_detection.rs`
- Modify: language extractors that call `is_test_symbol`
- Modify: related test fixtures and ranking tests

**What to build:** Better test detection and test-aware ranking from canonical markers.

**Approach:**
- Make canonical annotation markers a first-class input to `is_test_symbol`.
- Use them for languages and frameworks where annotation markers are stronger than path naming:
  - Java `@Test`, `@ParameterizedTest`, `@Nested`
  - C# `[Test]`, `[Fact]`, `[Theory]`
  - Python `pytest.mark.*`, fixtures, patches
  - TypeScript and other frameworks with annotation-driven test semantics
- During rollout, keep existing path and doc heuristics as fallback, not the primary signal where markers exist.
- Add regression tests for languages that already collect some annotation data but do not persist or use it consistently.

**Acceptance criteria:**
- [ ] Java `@Test` and `@ParameterizedTest` annotated methods detected as test symbols
- [ ] C# `[Test]`, `[Fact]`, `[Theory]` attributed methods detected as test symbols
- [ ] Python `@pytest.mark.parametrize` and `@pytest.fixture` decorated symbols flagged correctly
- [ ] TypeScript decorator markers reach `is_test_symbol` (verified by test, not by absence of error)
- [ ] Path-based test detection still works for languages without annotation support (Go `_test.go`, Rust `src/tests/`)
- [ ] No regressions in existing `search_quality` dogfood tests
- [ ] `exclude_tests` filter in `fast_search` correctly excludes annotation-detected test symbols

---

### Task B5: Add Config-Driven Annotation Classes And Signal Classes

**Files:**
- Modify: `src/search/language_config.rs`
- Modify: a focused set of `languages/*.toml` files

**What to build:** Data-driven classification of canonical markers and structural signals.

**Approach:**
- Add an `[annotations]` section for canonical marker classes, for example:
  - `endpoint`
  - `auth`
  - `test`
  - `fixture`
  - `middleware`
  - `scheduler`
- Add a `[signals]` section for structural warning inputs, for example:
  - `input_markers`
  - `auth_markers`
  - `danger_markers`
  - `sink_patterns`
  - `trap_patterns`
- Start with the languages that have the strongest payoff:
  - Python
  - TypeScript
  - JavaScript
  - Java
  - C#
  - Rust
- Leave other languages empty by default. Empty is acceptable. Fake coverage is not.

**Acceptance criteria:**
- [ ] All 34 language configs load without errors (existing test at `language_config.rs:215-224` still passes)
- [ ] Missing `[annotations]` section defaults to empty `AnnotationConfig` (backward compatible)
- [ ] Missing `[signals]` section defaults to empty `SignalConfig` (backward compatible)
- [ ] Python config has populated `endpoint`, `auth`, `test`, `fixture`, `middleware` annotation classes
- [ ] TypeScript config has populated `endpoint`, `test`, `middleware` annotation classes
- [ ] Java config has populated `endpoint`, `auth`, `test` annotation classes plus `sink_patterns` signals
- [ ] C# config has populated `endpoint`, `auth`, `test` annotation classes plus `sink_patterns` signals
- [ ] Rust config has populated `test` annotation class plus `sink_patterns` signals
- [ ] JavaScript config has populated `sink_patterns` and `trap_patterns` signals (eval, innerHTML, dangerouslySetInnerHTML)
- [ ] Config test: loading a language config with both sections populated, verify all fields accessible
- [ ] Config test: loading a language config with neither section, verify empty defaults

---

## Plan C: Early Warning Signals And Reporting (Sessions 5-6)

### Framing

This plan produces **security signals** and **early warning output**.

It does not produce taint proofs.
It does not claim exploit reachability.
It does not try to outgrow Julie's structural graph into a scanner it is not.

### File Structure

```
src/
├── analysis/
│   └── security_report.rs      # CREATE: structural signals report generation
├── cli_tools/
│   └── mod.rs                  # MODIFY: add signals report command
├── dashboard/
│   └── routes/
│       └── security.rs         # CREATE: dashboard route handlers
└── cli.rs                      # MODIFY: add report command variant

dashboard/templates/
├── security.html               # CREATE: main security signals page
├── security_surface.html       # CREATE: attack surface summary partial
├── security_auth_gaps.html     # CREATE: auth gap partial
├── security_sinks.html         # CREATE: structural sink signal partial
└── security_traps.html         # CREATE: language trap findings partial
```

### Task C1: Refactor `security_risk.rs` Into A Signal Engine

**Files:**
- Modify: `src/analysis/security_risk.rs`

**What to build:** Keep the current module as the structural signal engine for now, but move it from hardcoded heuristics toward config-driven marker and sink inputs.

**Approach:**
- Keep the existing scoring skeleton where it still makes sense.
- Replace hardcoded input and sink lists with config-driven signal inputs from the new `[signals]` sections.
- Add canonical annotation marker checks for:
  - entry points
  - auth guards
  - danger markers
- Keep the output language honest. These are structural risk signals, not verified vulnerabilities.
- Do not rename the module in the first pass if that adds churn without product value. Rename later if the output surface demands it.

**Acceptance criteria:**
- [ ] `EXECUTION_SINKS`, `DATABASE_SINKS`, `INPUT_PATTERNS`, `DI_EXCLUSION_PATTERNS` constants removed or replaced by config lookups
- [ ] Python symbols scored using Python-specific `sink_patterns` from `python.toml`
- [ ] C# DI exclusion patterns moved to `csharp.toml` (no more hardcoded C#-specific logic)
- [ ] Canonical annotation markers checked for `input_markers` and `auth_markers` from `[signals]` config
- [ ] A symbol with `@app.route` + `@login_required` scores lower risk than one with `@app.route` alone
- [ ] `SecurityRiskStats` output labels use "signal" language (not "vulnerability" or "finding")
- [ ] Unit tests verify scoring with mocked config inputs for at least 3 languages

---

### Task C2: Generate A Structural Early Warning Report

**Files:**
- Create: `src/analysis/security_report.rs`

**What to build:** A report that highlights structural warning signs from normalized markers and existing graph data.

**Approach:**
- Build a `SecuritySignalsReport` or similarly named struct containing:
  - `attack_surface`
  - `auth_gaps`
  - `sink_signals`
  - `trap_findings`
  - `generated_at`
- Derive entry points from canonical marker classes plus limited name-pattern fallbacks.
- Derive auth coverage from canonical auth markers.
- Derive sink signals from identifier and relationship evidence plus config sink patterns.
- Derive trap findings from pattern matches and configured warning patterns.
- Do not run endpoint × sink cartesian BFS across the whole workspace in V1.
- If `call_path` is used at all in this phase, use it only for a short candidate-path shortlist and label the output as a **candidate structural path**, not a proof of unsafe data flow.

**Acceptance criteria:**
- [ ] `generate_signals_report()` returns `SecuritySignalsReport` with all four sections populated for a test workspace
- [ ] Entry points identified by matching symbol annotations against `[signals].input_markers`
- [ ] Auth gaps: entry points where no annotation matches `[signals].auth_markers`
- [ ] Sink signals: identifiers/callees matching `[signals].sink_patterns` linked to calling symbols
- [ ] Trap findings: pattern matches from `[signals].trap_patterns` with location and safe alternative
- [ ] If `call_path` is used, output labels results as "candidate structural path" (not "reachable")
- [ ] Report generation completes in <30s for a workspace with ~5000 symbols
- [ ] Report struct is fully serializable (JSON roundtrip test)

---

### Task C3: Cache Reports Using Existing Revision State

**Files:**
- Modify: `src/analysis/security_report.rs`
- Modify: `src/database/schema.rs`

**What to build:** Cached report storage keyed to Julie's existing workspace revision state.

**Approach:**
- Store cached reports in a `security_reports` table.
- Key cache invalidation to canonical revision and projection state, not an invented parallel hash mechanism.
- Reuse existing workspace state where possible so the cache contract stays aligned with Julie's index lifecycle.
- Add a `--fresh` bypass for CLI use and a refresh button for the dashboard.

**Acceptance criteria:**
- [ ] `security_reports` table stores serialized report keyed to workspace_id + revision state
- [ ] Second report load from cache completes in <1s
- [ ] Cache invalidates when workspace index state changes (new files indexed, symbols updated)
- [ ] `--fresh` CLI flag bypasses cache and regenerates
- [ ] Dashboard "Refresh" button triggers fresh generation
- [ ] Dashboard shows "Last analyzed: {timestamp}" from cached report
- [ ] No parallel revision-tracking mechanism invented; reuses existing workspace state

---

### Task C4: Dashboard And CLI Output

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/cli_tools/mod.rs`
- Create: `src/dashboard/routes/security.rs`
- Modify: `src/dashboard/mod.rs`
- Modify: `src/dashboard/routes/mod.rs`
- Create: dashboard templates listed above

**What to build:** One report surface for humans and one for automation.

**Approach:**
- Add a CLI command such as `julie-server security-signals`.
- Support:
  - text output for terminal use
  - JSON output for automation
  - Markdown output for CI artifacts and review summaries
- Add a dashboard page such as `/security/{workspace_id}` that presents:
  - attack surface summary
  - auth gaps
  - sink signals
  - trap findings
- Keep labels honest:
  - use terms such as "signals", "gaps", "candidate paths", and "markers"
  - avoid language that implies proof

**Acceptance criteria:**
- [ ] `julie-server security-signals --workspace . --standalone` produces readable text with section headers
- [ ] `julie-server security-signals --workspace . --json --standalone` produces valid JSON parseable by `jq`
- [ ] `julie-server security-signals --workspace . --format markdown --standalone` produces CI-ready markdown
- [ ] `--file-pattern "src/api/**"` scopes analysis to matching files
- [ ] Dashboard `/security/{workspace_id}` renders all four sections (surface, auth gaps, sinks, traps)
- [ ] Dashboard handles empty state cleanly (no signals found for this workspace)
- [ ] Dashboard navigation includes "Signals" link alongside existing Intelligence link
- [ ] All output labels use "signal", "gap", "candidate" language (no "vulnerability" or "exploit")

---

## Verification

After all three plans are complete, verify the integration with the use cases that motivated the work.

1. **Agent loop:** `cargo build && ./target/debug/julie-server tool fast_search --params '{"query":"security"}' --workspace . --standalone` works during ordinary development without requiring an MCP client restart.

2. **Named wrapper loop:** `./target/debug/julie-server search "fn main" --workspace . --standalone --json` produces machine-readable output suitable for local agent checks.

3. **Annotation normalization:** index a multi-language project and verify that Python, TypeScript, Java, C#, Rust, and Kotlin symbols persist canonical annotation markers while keeping raw syntax available for inspection.

4. **Test understanding:** verify that canonical markers improve test detection in languages that rely on annotations or attributes, with no regression in path-based fallback detection.

5. **Signals report:** run the CLI report on a framework-heavy workspace and verify that it identifies entry points, auth gaps, sink signals, and trap findings without claiming vulnerability proof.

6. **Dashboard:** open the dashboard page and verify that all report sections load and refresh correctly.

7. **CI automation:** add a pipeline step that runs the CLI in JSON or Markdown mode and stores or posts the results.

---

## Notes For Future Sessions

- The current extractor state already contains useful pieces of this work. The job is normalization and consolidation, not greenfield invention.
- The CLI is worth doing even if the reporting work slips. It has direct value for autonomous agents, shell workflows, and CI.
- Annotation normalization is a prerequisite for reliable test understanding and for any future signal-oriented reporting.
- Keep the framing disciplined. If the implementation only supports structural warning signs, the product language must say that.
