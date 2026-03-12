# Test Runner and Tiering Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a repo-managed `cargo xtask test` runner with explicit tier and bucket definitions, then migrate Julie's test documentation to that single source of truth.

**Architecture:** Introduce a small `xtask` workspace member plus `.cargo/config.toml` alias so `cargo xtask ...` works everywhere. Store tier/bucket definitions in `xtask/test_tiers.toml`, parse them into typed Rust structures, and run bucket commands sequentially with progress, elapsed time, and timeout handling. Keep the first pass simple: no mass test-file reorganization, just a measured manifest that stops treating `--skip search_quality` as a real strategy.

**Tech Stack:** Rust workspace member (`xtask`), `std::process::Command`, `serde`, `toml`, `anyhow`, `clap`, markdown docs.

---

## File Map

- Modify: `Cargo.toml` - add the `xtask` workspace member.
- Create: `.cargo/config.toml` - add the `xtask` cargo alias.
- Create: `xtask/Cargo.toml` - declare the runner crate and dependencies.
- Create: `xtask/src/main.rs` - thin CLI entrypoint.
- Create: `xtask/src/lib.rs` - shared exports for parser/runner logic.
- Create: `xtask/src/cli.rs` - CLI argument parsing and command dispatch.
- Create: `xtask/src/manifest.rs` - typed manifest schema and loader.
- Create: `xtask/src/runner.rs` - sequential bucket execution, timing, and timeout logic.
- Create: `xtask/test_tiers.toml` - canonical tier/bucket manifest.
- Create: `xtask/tests/manifest_tests.rs` - manifest parser tests.
- Create: `xtask/tests/manifest_contract_tests.rs` - first-pass bucket membership tests.
- Create: `xtask/tests/runner_tests.rs` - runner behavior tests with a fake executor.
- Create: `xtask/tests/docs_contract_tests.rs` - docs/manifest contract tests.
- Modify: `CLAUDE.md` - replace raw tier commands with canonical runner guidance.
- Modify: `AGENTS.md` - require the repo runner by default.
- Modify: `README.md` - concise public-facing runner usage.

## Chunk 1: Bootstrap the `xtask` Runner and Manifest Loader

### Task 1: Create the `xtask` workspace member and manifest parser

**Files:**
- Modify: `Cargo.toml`
- Create: `.cargo/config.toml`
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Create: `xtask/src/lib.rs`
- Create: `xtask/src/manifest.rs`
- Create: `xtask/tests/manifest_tests.rs`

- [ ] **Step 1: Write a failing manifest parser test first**

Add a parser test that proves the manifest supports tiers, buckets, expected seconds, timeout seconds, and one or more commands per bucket.

```rust
#[test]
fn parses_tiers_and_buckets_from_toml() {
    let manifest = TestManifest::from_str(r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#).unwrap();

    assert_eq!(manifest.tiers["smoke"], vec!["cli"]);
    assert_eq!(manifest.buckets["cli"].expected_seconds, 1);
    assert_eq!(manifest.buckets["cli"].commands.len(), 1);
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test -p xtask manifest_tests 2>&1 | tail -20`
Expected: FAIL because the `xtask` package and manifest loader do not exist yet.

- [ ] **Step 3: Add the workspace member and cargo alias**

Update the workspace and alias so the final command shape works:

```toml
# Cargo.toml
[workspace]
members = [".", "crates/julie-extractors", "xtask"]
```

```toml
# .cargo/config.toml
[alias]
xtask = "run -p xtask --"
```

- [ ] **Step 4: Create the minimal `xtask` crate and typed manifest loader**

Use focused types rather than `serde_json::Value` soup:

```rust
#[derive(Debug, Deserialize)]
pub struct TestManifest {
    pub tiers: BTreeMap<String, Vec<String>>,
    pub buckets: BTreeMap<String, BucketConfig>,
}

#[derive(Debug, Deserialize)]
pub struct BucketConfig {
    pub expected_seconds: u64,
    pub timeout_seconds: u64,
    pub commands: Vec<String>,
}
```

Expose `TestManifest::load(path)` and `TestManifest::from_str(&str)` from `xtask/src/manifest.rs`.

Also add one shared helper so tests can load repo-root files reliably from the `xtask` crate:

```rust
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live under the workspace root")
        .to_path_buf()
}
```

- [ ] **Step 5: Re-run the parser tests and verify GREEN**

Run: `cargo test -p xtask manifest_tests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Commit the bootstrap slice**

```bash
git add Cargo.toml .cargo/config.toml xtask/Cargo.toml xtask/src/main.rs xtask/src/lib.rs xtask/src/manifest.rs xtask/tests/manifest_tests.rs
git commit -m "feat(testing): bootstrap xtask test manifest"
```

## Chunk 2: Implement the Runner CLI, Progress Output, and Timeout Handling

### Task 2: Build the `cargo xtask test` execution flow

**Files:**
- Modify: `xtask/src/main.rs`
- Modify: `xtask/src/lib.rs`
- Create: `xtask/src/cli.rs`
- Create: `xtask/src/runner.rs`
- Create: `xtask/tests/runner_tests.rs`

- [ ] **Step 1: Write failing runner tests for the final CLI contract**

Cover the boring but important contract:

```rust
#[test]
fn list_command_shows_tiers_buckets_and_timeouts() {
    let output = render_manifest_listing(&manifest);
    assert!(output.contains("smoke"));
    assert!(output.contains("workspace-init"));
    assert!(output.contains("timeout_seconds"));
}

#[test]
fn run_tier_executes_buckets_in_manifest_order() {
    let result = run_tier(&manifest, "dev", &fake_executor()).unwrap();
    assert_eq!(result.bucket_names, vec!["cli", "core-database", "tools-search"]);
}

#[test]
fn timeout_error_names_the_bucket_and_budget() {
    let err = run_bucket(&bucket, &timed_out_executor()).unwrap_err();
    assert!(err.to_string().contains("workspace-init"));
    assert!(err.to_string().contains("timeout"));
}

#[test]
fn summary_output_reports_total_elapsed_time() {
    let output = render_summary(&summary);
    assert!(output.contains("SUMMARY:"));
    assert!(output.contains("passed in"));
}

#[test]
fn bucket_output_has_start_and_end_markers() {
    let output = render_bucket_status("tools-search", BucketStatus::Passed, duration);
    assert!(output.contains("START tools-search"));
    assert!(output.contains("END tools-search"));
}
```

- [ ] **Step 2: Run the tests to verify RED**

Run: `cargo test -p xtask runner_tests 2>&1 | tail -20`
Expected: FAIL because the CLI contract and runner do not exist yet.

- [ ] **Step 3: Implement the CLI contract exactly once**

Support only this interface:

```text
cargo xtask test smoke
cargo xtask test dev
cargo xtask test system
cargo xtask test dogfood
cargo xtask test full
cargo xtask test list
cargo xtask test bucket workspace-init
```

Use a plain enum instead of stringly-typed branching everywhere:

```rust
pub enum TestCommand {
    List,
    Tier { name: String, timeout_multiplier: u64 },
    Bucket { name: String, timeout_multiplier: u64 },
}
```

- [ ] **Step 4: Implement sequential bucket execution with a fakeable executor**

Keep the runner testable by injecting command execution behind a trait:

```rust
pub trait CommandExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandOutcome>;
}
```

The real executor should:
- print `START` and `PASS`/`FAIL`/`TIMEOUT` markers
- measure elapsed time with `Instant`
- run each command in order for the bucket
- stop on the first failing command

- [ ] **Step 5: Implement the timeout model from the spec**

Read `timeout_seconds` from the manifest and support a `--timeout-multiplier <n>` override.

Timeout errors should include:
- bucket name
- timeout budget
- expected time
- raw command string

- [ ] **Step 6: Re-run runner tests and verify GREEN**

Run: `cargo test -p xtask runner_tests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Verify the CLI is wired up end-to-end**

Run:
- `cargo xtask test list 2>&1 | tail -20`
- `cargo xtask test bucket tools-search 2>&1 | tail -20`

Expected:
- list succeeds and prints tiers/buckets rather than Cargo alias or package errors
- the generic bucket path succeeds for a real bucket name and prints bucket-level progress output

- [ ] **Step 8: Commit the runner slice**

```bash
git add xtask/src/main.rs xtask/src/lib.rs xtask/src/cli.rs xtask/src/runner.rs xtask/tests/runner_tests.rs
git commit -m "feat(testing): add xtask test runner"
```

## Chunk 3: Populate the First-Pass Tier Manifest Exactly

### Task 3: Add the real bucket map with exact checked-in membership

**Files:**
- Create: `xtask/test_tiers.toml`
- Create: `xtask/tests/manifest_contract_tests.rs`

- [ ] **Step 1: Write a failing manifest contract test**

Add a test that locks the first-pass bucket placements from the spec.

```rust
#[test]
fn manifest_contains_the_known_first_pass_buckets() {
    let manifest = TestManifest::load(workspace_root().join("xtask/test_tiers.toml")).unwrap();
    assert!(manifest.tiers["smoke"].contains(&"tools-get-context".to_string()));
    assert!(manifest.tiers["system"].contains(&"workspace-init".to_string()));
    assert!(manifest.tiers["dogfood"].contains(&"search-quality".to_string()));
}
```

- [ ] **Step 2: Run the tests to verify RED**

Run: `cargo test -p xtask manifest_contract_tests 2>&1 | tail -20`
Expected: FAIL because the manifest does not exist yet.

- [ ] **Step 3: Populate `xtask/test_tiers.toml` with the concrete first pass**

Start with explicit bucket names and explicit command arrays. Keep it boring.

Example shape:

```toml
[tiers]
smoke = ["cli", "core-database", "core-embeddings", "tools-get-context"]
dev = ["cli", "core-database", "core-embeddings", "tools-get-context", "tools-search", "tools-workspace", "tools-misc", "core-fast"]
system = ["workspace-init", "integration"]
dogfood = ["search-quality"]
full = ["cli", "core-database", "core-embeddings", "tools-get-context", "tools-search", "tools-workspace", "tools-misc", "core-fast", "workspace-init", "integration", "search-quality"]

[buckets.workspace-init]
expected_seconds = 120
timeout_seconds = 360
commands = ["cargo test --lib tests::core::workspace_init -- --skip search_quality"]

[buckets.search-quality]
expected_seconds = 250
timeout_seconds = 750
commands = ["cargo test --lib search_quality"]
```

Concrete first-pass command mapping to include:

- `cli` -> `cargo test --lib tests::cli_tests`
- `core-database` -> `cargo test --lib tests::core::database -- --skip search_quality`
- `core-embeddings` -> exact commands for:
  - `cargo test --lib tests::core::embedding_provider -- --skip search_quality`
  - `cargo test --lib tests::core::embedding_metadata -- --skip search_quality`
  - `cargo test --lib tests::core::embedding_deps -- --skip search_quality`
  - `cargo test --lib tests::core::embedding_sidecar_protocol -- --skip search_quality`
  - `cargo test --lib tests::core::embedding_sidecar_provider -- --skip search_quality`
  - `cargo test --lib tests::core::windows_embedding_policy -- --skip search_quality`
  - `cargo test --lib tests::core::sidecar_supervisor_tests -- --skip search_quality`
  - `cargo test --lib tests::core::sidecar_embedding_tests -- --skip search_quality`
- `tools-get-context` -> exact commands for:
  - `cargo test --lib tests::tools::get_context_allocation_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_formatting_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_graph_expansion_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_pipeline_relevance_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_pipeline_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_quality_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_relevance_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_scoring_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::get_context_token_budget_tests -- --skip search_quality`
- `tools-search` -> exact commands for:
  - `cargo test --lib tests::tools::search -- --skip search_quality`
  - `cargo test --lib tests::tools::search_context_lines -- --skip search_quality`
  - `cargo test --lib tests::tools::text_search_tantivy -- --skip search_quality`
  - `cargo test --lib tests::tools::hybrid_search_tests -- --skip search_quality`
- `tools-workspace` -> exact command:
  - `cargo test --lib tests::tools::workspace -- --skip search_quality`
- `tools-misc` -> exact commands for:
  - `cargo test --lib tests::tools::get_symbols -- --skip search_quality`
  - `cargo test --lib tests::tools::get_symbols_reference_workspace -- --skip search_quality`
  - `cargo test --lib tests::tools::get_symbols_relative_paths -- --skip search_quality`
  - `cargo test --lib tests::tools::get_symbols_smart_read -- --skip search_quality`
  - `cargo test --lib tests::tools::get_symbols_target_filtering -- --skip search_quality`
  - `cargo test --lib tests::tools::get_symbols_token -- --skip search_quality`
  - `cargo test --lib tests::tools::smart_read -- --skip search_quality`
  - `cargo test --lib tests::tools::editing -- --skip search_quality`
  - `cargo test --lib tests::tools::deep_dive_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::refactoring -- --skip search_quality`
  - `cargo test --lib tests::tools::filtering_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::formatting_tests -- --skip search_quality`
  - `cargo test --lib tests::tools::reference_workspace_fast_refs_tests -- --skip search_quality`
  - `cargo test --lib tools::search::query_preprocessor::tests -- --skip search_quality`
- `core-fast` -> exact commands for:
  - `cargo test --lib tests::main_error_handling -- --skip search_quality`
  - `cargo test --lib tests::regression_prevention_tests -- --skip search_quality`
  - `cargo test --lib utils::paths::tests -- --skip search_quality`
  - `cargo test --lib utils::string_similarity::tests -- --skip search_quality`
  - `cargo test --lib watcher::filtering::tests -- --skip search_quality`
  - `cargo test --lib watcher::tests -- --skip search_quality`
  - `cargo test --lib tests::core::database_lightweight_query -- --skip search_quality`
  - `cargo test --lib tests::core::handler -- --skip search_quality`
  - `cargo test --lib tests::core::language -- --skip search_quality`
  - `cargo test --lib tests::core::memory_vectors -- --skip search_quality`
  - `cargo test --lib tests::core::paths -- --skip search_quality`
  - `cargo test --lib tests::core::tracing -- --skip search_quality`
  - `cargo test --lib tests::core::vector_storage -- --skip search_quality`
- `integration` -> `cargo test --lib tests::integration -- --skip search_quality`
- `workspace-init` -> `cargo test --lib tests::core::workspace_init -- --skip search_quality`
- `search-quality` -> `cargo test --lib search_quality`

Do not leave fuzzy placeholders like `get_context*`, `remaining`, or `such as` in the checked-in manifest.

- [ ] **Step 4: Re-run the manifest contract tests and verify GREEN**

Run: `cargo test -p xtask manifest_contract_tests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit the manifest slice**

```bash
git add xtask/test_tiers.toml xtask/tests/manifest_contract_tests.rs
git commit -m "test: add explicit xtask tier manifest"
```

## Chunk 4: Measure the New Tiers and Tune the Manifest Before Publishing Docs

### Task 4: Verify the real runner behavior with measured timings

**Files:**
- Modify: `xtask/test_tiers.toml`

- [ ] **Step 1: Run the listing command and inspect the printed budgets**

Run: `cargo xtask test list`
Expected: each bucket shows commands, expected seconds, and timeout seconds.

- [ ] **Step 2: Run the smoke tier**

Run: `cargo xtask test smoke 2>&1 | tail -40`
Expected: PASS, with bucket-by-bucket progress output and total elapsed time under the intended smoke budget.

- [ ] **Step 3: Run the dev tier**

Run: `cargo xtask test dev 2>&1 | tail -40`
Expected: PASS, with visible per-bucket progress and total elapsed time within the intended day-to-day budget or with manifest adjustments made to reflect reality.

- [ ] **Step 4: Run the known slow outlier bucket directly**

Run: `cargo xtask test bucket workspace-init 2>&1 | tail -40`
Expected: bucket output makes it obvious that the suite is slow rather than silently hanging.

- [ ] **Step 5: Run the dogfood bucket through the new runner**

Run: `cargo xtask test dogfood 2>&1 | tail -40`
Expected: PASS, with the expensive `search_quality` path still runnable via the canonical interface.

- [ ] **Step 6: Run the full tier once**

Run: `cargo xtask test full 2>&1 | tail -40`
Expected: PASS, proving the canonical runner can execute the entire intended policy end-to-end.

- [ ] **Step 7: Tune `expected_seconds` or timeout budgets if measured reality differs materially**

Adjust the manifest when observed durations are clearly different from the initial estimates. Do not leave fake numbers in place.

- [ ] **Step 8: Commit the measured-budget cleanup**

```bash
git add xtask/test_tiers.toml
git commit -m "test: calibrate xtask tier budgets"
```

## Chunk 5: Migrate Docs After the Runner and Budgets Are Real

### Task 5: Rewrite the canonical docs and lock the contract

**Files:**
- Create: `xtask/tests/docs_contract_tests.rs`
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`
- Modify: `README.md`

- [ ] **Step 1: Write a failing docs contract test**

Use `workspace_root()` to read repo files and lock the runner-facing commands:

```rust
#[test]
fn docs_reference_canonical_xtask_commands() {
    let claude = std::fs::read_to_string(workspace_root().join("CLAUDE.md")).unwrap();
    let agents = std::fs::read_to_string(workspace_root().join("AGENTS.md")).unwrap();
    let readme = std::fs::read_to_string(workspace_root().join("README.md")).unwrap();

    assert!(claude.contains("cargo xtask test dev"));
    assert!(agents.contains("cargo xtask test dev"));
    assert!(readme.contains("cargo xtask test list"));
    assert!(!claude.contains("Fast | `cargo test --lib -- --skip search_quality` | ~15s"));
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test -p xtask docs_contract_tests 2>&1 | tail -20`
Expected: FAIL because the docs still point at stale raw cargo commands.

- [ ] **Step 3: Rewrite the docs to make the runner canonical**

Update the docs so they all agree:

- `CLAUDE.md`
  - explain `smoke`, `dev`, `system`, `dogfood`, `full`
  - make `cargo xtask test dev` the default after normal changes
  - keep raw cargo filters only for narrowing failures
- `AGENTS.md`
  - tell agents to default to `cargo xtask test dev`
  - stop prescribing the stale `--skip search_quality` command as the canonical fast tier
- `README.md`
  - give a short public section with `cargo xtask test dev`, `cargo xtask test dogfood`, and `cargo xtask test list`

- [ ] **Step 4: Re-run the docs contract tests and verify GREEN**

Run: `cargo test -p xtask docs_contract_tests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit the docs slice**

```bash
git add xtask/tests/docs_contract_tests.rs CLAUDE.md AGENTS.md README.md
git commit -m "docs(testing): make xtask runner canonical"
```
