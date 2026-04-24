# Test Intelligence Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Replace Julie's test/fixture conflation and regex-based quality oracle with config-driven role classification and evidence-based quality assessment, then add security and test coverage signals on the trustworthy foundation.

**Architecture:** Extractors remain pure (no config changes). A new `classify_symbols_by_role` pipeline step runs post-extraction, before DB write, using TOML-driven `TestRoleConfig` to classify test roles authoritatively. `TestRole` enum is a shared type in the extractors crate; classification logic and `TestRoleConfig` live in the server crate. Evidence-based quality model queries identifier data (filtered on `kind = 'Call'`) against framework-specific `[test_evidence]` TOML config, with regex fallback at lower confidence. Quality tiers carry confidence scores that flow through test_linkage aggregation and gate the 30% test_weakness weight in change_risk. New helpers `is_scorable_test()` and `is_test_related()` replace raw `metadata.is_test` checks.

**Tech Stack:** Rust, serde, tree-sitter, TOML configs, SQLite (identifiers table), existing Julie pipeline infrastructure

**Design doc:** `docs/plans/2026-04-24-test-intelligence-foundation-design.md`

---

## Session 1: Foundation Types & Config

### Task 1: Define TestRole enum and TestAnnotationClasses config

**Files:**
- Modify: `crates/julie-extractors/src/base/types.rs` (add `TestRole` enum)
- Modify: `crates/julie-extractors/src/lib.rs` (re-export `TestRole`)
- Modify: `src/search/language_config.rs:73-88` (replace flat test/fixture with `TestAnnotationClasses`)
- Test: `src/search/language_config.rs` (update existing tests)

**What to build:** `TestRole` enum as a shared type in the extractors crate. `TestAnnotationClasses` struct replacing the flat `test: Vec<String>` / `fixture: Vec<String>` in `AnnotationClassesConfig`. No behavioral changes yet; this is type and config plumbing.

**Step 1: Write failing test**

Add to `src/search/language_config.rs` tests:

```rust
#[test]
fn test_annotation_classes_use_role_taxonomy() {
    let configs = LanguageConfigs::load_embedded();
    let csharp = configs.get("csharp").expect("csharp config");

    // xUnit
    assert!(csharp.annotation_classes.test.test_case.contains(&"fact".to_string()));
    // NUnit
    assert!(csharp.annotation_classes.test.test_case.contains(&"test".to_string()));
    // Fixtures are in fixture_setup, not test_case
    assert!(!csharp.annotation_classes.test.test_case.contains(&"setup".to_string()));
    assert!(csharp.annotation_classes.test.fixture_setup.contains(&"setup".to_string()));
    // Teardown
    assert!(csharp.annotation_classes.test.fixture_teardown.contains(&"teardown".to_string()));
    // Parameterized
    assert!(csharp.annotation_classes.test.parameterized_test.contains(&"theory".to_string()));
    // Containers
    assert!(csharp.annotation_classes.test.test_container.contains(&"testfixture".to_string()));
}
```

**Step 2: Add TestRole to extractors crate**

In `crates/julie-extractors/src/base/types.rs`, after the `AnnotationMarker` struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestRole {
    TestCase,
    ParameterizedTest,
    FixtureSetup,
    FixtureTeardown,
    TestContainer,
}

impl TestRole {
    pub fn is_scorable(&self) -> bool {
        matches!(self, TestRole::TestCase | TestRole::ParameterizedTest)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TestRole::TestCase => "test_case",
            TestRole::ParameterizedTest => "parameterized_test",
            TestRole::FixtureSetup => "fixture_setup",
            TestRole::FixtureTeardown => "fixture_teardown",
            TestRole::TestContainer => "test_container",
        }
    }
}
```

Add to `crates/julie-extractors/src/lib.rs` re-exports:

```rust
pub use base::TestRole;
```

**Step 3: Replace AnnotationClassesConfig**

In `src/search/language_config.rs`, replace the flat `test`/`fixture` fields:

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestAnnotationClasses {
    #[serde(default)]
    pub test_case: Vec<String>,
    #[serde(default)]
    pub parameterized_test: Vec<String>,
    #[serde(default)]
    pub fixture_setup: Vec<String>,
    #[serde(default)]
    pub fixture_teardown: Vec<String>,
    #[serde(default)]
    pub test_container: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnnotationClassesConfig {
    #[serde(default)]
    pub entrypoint: Vec<String>,
    #[serde(default)]
    pub auth: Vec<String>,
    #[serde(default)]
    pub auth_bypass: Vec<String>,
    #[serde(default)]
    pub middleware: Vec<String>,
    #[serde(default)]
    pub scheduler: Vec<String>,
    #[serde(default)]
    pub test: TestAnnotationClasses,
}
```

**Step 4: Update all language TOML files**

Replace `[annotation_classes]` sections with role-classified entries per the design doc Part 2. Key changes:
- `test` and `fixture` flat lists become `[annotation_classes.test]` table with five role fields
- `@DataProvider` excluded from all languages (annotates data method, not test)
- See design doc for complete per-language specifications

**Step 5: Fix existing tests**

Update tests that reference `config.annotation_classes.test` (was `Vec<String>`, now `TestAnnotationClasses`) and `config.annotation_classes.fixture` (removed, split into fixture_setup/fixture_teardown).

**Step 6: Run tests, commit**

```bash
cargo nextest run --lib test_annotation_classes 2>&1 | tail -10
cargo nextest run --lib test_language_config 2>&1 | tail -10
```

```bash
git add crates/julie-extractors/src/base/types.rs crates/julie-extractors/src/lib.rs \
       src/search/language_config.rs languages/*.toml
git commit -m "feat: add TestRole enum and role-classified annotation config

TestRole (TestCase, ParameterizedTest, FixtureSetup, FixtureTeardown,
TestContainer) as shared type in extractors crate. AnnotationClassesConfig
replaces flat test/fixture with TestAnnotationClasses. All applicable
language TOMLs updated with comprehensive framework coverage."
```

---

### Task 2: Add TestRoleConfig and build from LanguageConfigs

**Files:**
- Create: `src/analysis/test_roles.rs` (TestRoleConfig, classify_test_role, helpers)
- Modify: `src/analysis/mod.rs` (add module)
- Modify: `src/search/language_config.rs` (add build_test_role_configs)
- Test: new test file or inline tests

**What to build:** `TestRoleConfig` struct (annotation-key sets per role), a builder that constructs configs from `LanguageConfigs`, and the `classify_test_role()` function that operates on fully-extracted `Symbol` structs.

**Step 1: Write failing tests**

```rust
#[test]
fn test_classify_csharp_fact_as_test_case() {
    let config = make_csharp_role_config();
    let symbol = make_symbol("MyTest", SymbolKind::Method, "src/Tests/Test.cs",
        vec![annotation_marker("fact")]);
    let role = classify_test_role(&symbol, Some(&config));
    assert_eq!(role, Some(TestRole::TestCase));
}

#[test]
fn test_classify_setup_as_fixture() {
    let config = make_csharp_role_config();
    let symbol = make_symbol("SetUp", SymbolKind::Method, "src/Tests/Test.cs",
        vec![annotation_marker("setup")]);
    let role = classify_test_role(&symbol, Some(&config));
    assert_eq!(role, Some(TestRole::FixtureSetup));
}

#[test]
fn test_classify_regular_method_returns_none() {
    let config = make_csharp_role_config();
    let symbol = make_symbol("Process", SymbolKind::Method, "src/Service.cs", vec![]);
    let role = classify_test_role(&symbol, Some(&config));
    assert_eq!(role, None);
}

#[test]
fn test_convention_fallback_for_go() {
    let symbol = make_symbol("TestFoo", SymbolKind::Function, "pkg/handler_test.go", vec![]);
    // No config for Go; falls back to is_test_symbol convention detection
    let role = classify_test_role(&symbol, None);
    assert_eq!(role, Some(TestRole::TestCase));
}

#[test]
fn test_is_scorable_true_for_test_case() {
    assert!(TestRole::TestCase.is_scorable());
    assert!(TestRole::ParameterizedTest.is_scorable());
}

#[test]
fn test_is_scorable_false_for_fixture() {
    assert!(!TestRole::FixtureSetup.is_scorable());
    assert!(!TestRole::FixtureTeardown.is_scorable());
    assert!(!TestRole::TestContainer.is_scorable());
}
```

**Step 2: Implement src/analysis/test_roles.rs**

```rust
use std::collections::HashSet;
use crate::extractors::{Symbol, SymbolKind, TestRole};

#[derive(Debug, Clone, Default)]
pub struct TestRoleConfig {
    pub test_case: HashSet<String>,
    pub parameterized_test: HashSet<String>,
    pub fixture_setup: HashSet<String>,
    pub fixture_teardown: HashSet<String>,
    pub test_container: HashSet<String>,
}

impl TestRoleConfig {
    pub fn classify_annotation(&self, annotation_key: &str) -> Option<TestRole> {
        if self.test_case.contains(annotation_key) {
            Some(TestRole::TestCase)
        } else if self.parameterized_test.contains(annotation_key) {
            Some(TestRole::ParameterizedTest)
        } else if self.fixture_setup.contains(annotation_key) {
            Some(TestRole::FixtureSetup)
        } else if self.fixture_teardown.contains(annotation_key) {
            Some(TestRole::FixtureTeardown)
        } else if self.test_container.contains(annotation_key) {
            Some(TestRole::TestContainer)
        } else {
            None
        }
    }
}

fn is_callable(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor)
}

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Class | SymbolKind::Struct | SymbolKind::Module | SymbolKind::Namespace)
}

pub fn classify_test_role(
    symbol: &Symbol,
    role_config: Option<&TestRoleConfig>,
) -> Option<TestRole> {
    // Config-driven classification from annotations
    if let Some(config) = role_config {
        for annotation in &symbol.annotations {
            if let Some(role) = config.classify_annotation(&annotation.annotation_key) {
                if role == TestRole::TestContainer && is_container_kind(&symbol.kind) {
                    return Some(role);
                }
                if role != TestRole::TestContainer && is_callable(&symbol.kind) {
                    return Some(role);
                }
            }
        }
    }

    // Fall back to extractor's is_test decision (convention-based languages)
    let extractor_says_test = symbol.metadata.as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if extractor_says_test && is_callable(&symbol.kind) {
        return Some(TestRole::TestCase);
    }

    None
}

/// Classify all symbols in a batch and set metadata.test_role + metadata.is_test.
/// This is the authoritative classification step; it overrides extractor decisions.
pub fn classify_symbols_by_role(
    symbols: &mut [Symbol],
    role_configs: &std::collections::HashMap<String, TestRoleConfig>,
) {
    for symbol in symbols.iter_mut() {
        let config = role_configs.get(symbol.language.as_str());
        let role = classify_test_role(symbol, config);

        let metadata = symbol.metadata.get_or_insert_with(Default::default);

        if let Some(role) = role {
            metadata.insert("test_role".into(), serde_json::Value::String(role.as_str().into()));
            metadata.insert("is_test".into(), serde_json::Value::Bool(true));
        } else {
            metadata.remove("test_role");
            // Only clear is_test if extractor didn't set it either
            // (preserves convention-based detection for edge cases)
        }
    }
}

/// True for any test-related symbol (test, fixture, container).
/// Use for: excluding from production rankings, embeddings, search de-boosting.
pub fn is_test_related(symbol: &Symbol) -> bool {
    symbol.metadata.as_ref()
        .and_then(|m| m.get("test_role"))
        .is_some()
    || symbol.metadata.as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// True only for TestCase and ParameterizedTest.
/// Use for: quality scoring, test linkage coverage, risk assessment.
pub fn is_scorable_test(symbol: &Symbol) -> bool {
    symbol.metadata.as_ref()
        .and_then(|m| m.get("test_role"))
        .and_then(|v| v.as_str())
        .map(|role| matches!(role, "test_case" | "parameterized_test"))
        .unwrap_or(false)
}
```

**Step 3: Add build_test_role_configs to LanguageConfigs**

In `src/search/language_config.rs`:

```rust
pub fn build_test_role_configs(&self) -> HashMap<String, crate::analysis::test_roles::TestRoleConfig> {
    self.configs.iter().map(|(lang, config)| {
        let tc = &config.annotation_classes.test;
        let role_config = crate::analysis::test_roles::TestRoleConfig {
            test_case: tc.test_case.iter().cloned().collect(),
            parameterized_test: tc.parameterized_test.iter().cloned().collect(),
            fixture_setup: tc.fixture_setup.iter().cloned().collect(),
            fixture_teardown: tc.fixture_teardown.iter().cloned().collect(),
            test_container: tc.test_container.iter().cloned().collect(),
        };
        (lang.clone(), role_config)
    }).collect()
}
```

**Step 4: Wire into pipeline**

In `src/tools/workspace/indexing/pipeline.rs`, in `persist_batch` or just before it, add:

```rust
let language_configs = handler.language_configs();
let role_configs = language_configs.build_test_role_configs();
crate::analysis::test_roles::classify_symbols_by_role(&mut batch.all_symbols, &role_configs);
```

Use `deep_dive(symbol="persist_batch")` to find the exact insertion point. The classification must happen AFTER extraction populates annotations and BEFORE symbols are written to the database.

**Step 5: Run tests, commit**

```bash
cargo nextest run --lib test_classify 2>&1 | tail -10
cargo nextest run --lib test_is_scorable 2>&1 | tail -10
```

```bash
git add src/analysis/test_roles.rs src/analysis/mod.rs src/search/language_config.rs \
       src/tools/workspace/indexing/pipeline.rs
git commit -m "feat(analysis): add post-extraction test role classification

classify_symbols_by_role runs in the pipeline after extraction, before
DB write. Config-driven role classification from TOML annotations,
with convention-based fallback from is_test_symbol. Adds is_scorable_test
and is_test_related helpers for downstream consumers."
```

---

## Session 2: Quality Model & Linkage

### Task 3: Add TestEvidenceConfig and update test_linkage

**Files:**
- Modify: `src/search/language_config.rs` (add TestEvidenceConfig)
- Modify: `languages/*.toml` (add `[test_evidence]` sections for verified languages)
- Modify: `src/analysis/test_linkage.rs` (use is_scorable_test, carry confidence)
- Test: new tests for evidence config loading, test_linkage with roles

**Step 1: Add TestEvidenceConfig**

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestEvidenceConfig {
    #[serde(default)]
    pub assertion_identifiers: Vec<String>,
    #[serde(default)]
    pub error_assertion_identifiers: Vec<String>,
    #[serde(default)]
    pub mock_identifiers: Vec<String>,
}
```

Add to `LanguageConfig`:
```rust
#[serde(default)]
pub test_evidence: TestEvidenceConfig,
```

**Step 2: Populate TOML files with test_evidence sections**

Per design doc Part 7. Before adding each language, verify identifier extraction:

```bash
# Verify assertion identifiers appear as Call-kind identifiers
./target/debug/julie-server search "assert_eq" --workspace . --standalone --json 2>/dev/null | head -5
```

**Step 3: Update test_linkage to use is_scorable_test**

In `src/analysis/test_linkage.rs`, the SQL queries filter on `json_extract(s_test.metadata, '$.is_test') = 1`. Update to also check `test_role`:

```sql
WHERE (json_extract(s_test.metadata, '$.test_role') IN ('test_case', 'parameterized_test')
       OR (json_extract(s_test.metadata, '$.is_test') = 1
           AND json_extract(s_test.metadata, '$.test_role') IS NULL))
```

This uses scorable tests for linkage while maintaining backward compat for symbols without test_role (convention-based languages where is_test_symbol set is_test but no role classification happened).

**Step 4: Carry confidence through test_linkage**

When `test_linkage` aggregates quality info for a production symbol, include confidence:

```rust
let linkage_info = serde_json::json!({
    "test_count": test_count,
    "best_tier": best_tier,
    "worst_tier": worst_tier,
    "best_confidence": best_confidence,  // NEW: from test_quality.confidence
    "linked_tests": names,
    "linked_test_paths": paths,
    "evidence_sources": evidence_sources,
});
```

Read `best_confidence` from the linked test's `metadata.test_quality.confidence`. If not present (old data), default to `0.5`.

**Step 5: Run tests, commit**

```bash
cargo nextest run --lib test_evidence_config 2>&1 | tail -5
cargo nextest run --lib test_linkage 2>&1 | tail -10
```

```bash
git add src/search/language_config.rs src/analysis/test_linkage.rs languages/*.toml
git commit -m "feat: add test_evidence config, update test_linkage for scorable tests

TestEvidenceConfig with framework-specific assertion/mock/error identifiers.
test_linkage uses is_scorable_test (excludes fixtures from coverage).
Confidence carried through linkage aggregation for downstream consumption."
```

---

### Task 4: Rewrite test_quality with evidence model

**Files:**
- Modify: `src/analysis/test_quality.rs` (replace regex oracle)
- Modify: `src/analysis/mod.rs` (update re-exports)
- Test: `src/tests/analysis/test_quality_tests.rs` (rewrite tests)

**Step 1: Write failing tests**

```rust
#[test]
fn test_fixture_is_not_applicable() {
    let assessment = assess_test_quality(
        Some("fixture_setup"), None, 0, 0, 0, false,
    );
    assert_eq!(assessment.tier, TestQualityTier::NotApplicable);
    assert_eq!(assessment.confidence, 1.0);
}

#[test]
fn test_empty_body_is_stub() {
    let assessment = assess_test_quality(
        Some("test_case"), Some(""), 0, 0, 0, false,
    );
    assert_eq!(assessment.tier, TestQualityTier::Stub);
    assert_eq!(assessment.confidence, 1.0);
}

#[test]
fn test_identifier_thorough_high_confidence() {
    let assessment = assess_test_quality(
        Some("test_case"),
        Some("fn test_foo() { assert_eq!(a, b); assert_ne!(c, d); assert!(e); should_err(); }"),
        3, 1, 0, true,
    );
    assert_eq!(assessment.tier, TestQualityTier::Thorough);
    assert!(assessment.confidence >= 0.85);
}

#[test]
fn test_regex_zero_assertions_is_unknown() {
    let assessment = assess_test_quality(
        Some("test_case"),
        Some("fn test_foo() { let x = compute(); println!(\"{}\", x); }"),
        0, 0, 0, false, // no identifier evidence, will use regex fallback
    );
    // Regex finds zero assertions -> Unknown, not Stub
    assert_eq!(assessment.tier, TestQualityTier::Unknown);
}
```

**Step 2: Implement evidence model**

Replace `analyze_test_body` and `classify_tier` with `assess_test_quality`. Keep `strip_comments_and_strings` for regex fallback. Full implementation per the design doc Part 4, with these corrections from review:

- `TestQualityTier::NotApplicable` for fixtures/teardown/containers
- `TestQualityTier::Unknown` for regex-found-nothing (not false `Stub`)
- Confidence scores on every tier

**Step 3: Update compute_test_quality_metrics**

The pipeline function now:
1. Reads `test_role` from symbol metadata
2. Queries identifiers WHERE `kind = 'Call'` AND `containing_symbol_id = ?test_id`
3. Matches against `TestEvidenceConfig` assertion/error/mock identifier lists
4. Calls `assess_test_quality` with identifier evidence
5. Stores assessment with `confidence` field in metadata

The identifier query MUST filter on `kind = 'Call'`:

```rust
fn query_identifier_evidence(
    db: &SymbolDatabase,
    symbol_id: &str,
    config: &TestEvidenceConfig,
) -> Result<(u32, u32, u32, bool)> {
    if config.assertion_identifiers.is_empty() {
        return Ok((0, 0, 0, false));
    }

    let names: Vec<String> = db.conn
        .prepare("SELECT LOWER(name) FROM identifiers WHERE containing_symbol_id = ?1 AND kind = 'Call'")?
        .query_map([symbol_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    if names.is_empty() {
        return Ok((0, 0, 0, false));
    }

    let assertion_set: HashSet<&str> = config.assertion_identifiers.iter().map(|s| s.as_str()).collect();
    let error_set: HashSet<&str> = config.error_assertion_identifiers.iter().map(|s| s.as_str()).collect();
    let mock_set: HashSet<&str> = config.mock_identifiers.iter().map(|s| s.as_str()).collect();

    let assertions = names.iter().filter(|n| assertion_set.contains(n.as_str())).count() as u32;
    let errors = names.iter().filter(|n| error_set.contains(n.as_str())).count() as u32;
    let mocks = names.iter().filter(|n| mock_set.contains(n.as_str())).count() as u32;

    Ok((assertions, errors, mocks, true))
}
```

**Step 4: Update analyze_batch signature**

`compute_test_quality_metrics` now takes `language_configs: &LanguageConfigs` to access `TestEvidenceConfig`.

**Step 5: Run tests, commit**

```bash
cargo nextest run --lib test_fixture_is_not 2>&1 | tail -5
cargo nextest run --lib test_identifier_thorough 2>&1 | tail -5
cargo nextest run --lib test_regex_zero 2>&1 | tail -5
```

```bash
git add src/analysis/test_quality.rs src/analysis/mod.rs \
       src/tools/workspace/indexing/pipeline.rs src/tests/analysis/test_quality_tests.rs
git commit -m "feat(analysis): replace regex quality oracle with evidence model

Identifier-based assertion counting filtered on kind='Call' against
framework-specific TOML config. Regex as lower-confidence fallback.
Fixtures return NotApplicable. Unknown replaces false Stub for
regex-found-nothing. All tiers carry confidence scores."
```

---

## Session 3: Risk Gating & Consumer Updates

### Task 5: Confidence-gated change_risk

**Files:**
- Modify: `src/analysis/change_risk.rs:72-81` (update test_weakness_score)
- Modify: `src/analysis/change_risk.rs:115-238` (read confidence from test_linkage)
- Test: `src/tests/analysis/change_risk_tests.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn test_weakness_high_confidence_thorough() {
    let score = test_weakness_score(Some("thorough"), 0.9);
    // raw = 0.1, confidence 0.9: 0.5 + (0.1 - 0.5) * 0.9 = 0.14
    assert!((score - 0.14).abs() < 0.02);
}

#[test]
fn test_weakness_low_confidence_converges_to_neutral() {
    let score = test_weakness_score(Some("stub"), 0.0);
    assert!((score - 0.5).abs() < 0.01);
}

#[test]
fn test_weakness_unknown_tier() {
    let score = test_weakness_score(Some("unknown"), 0.3);
    // raw = 0.5, any confidence: 0.5 + (0.5 - 0.5) * 0.3 = 0.5
    assert!((score - 0.5).abs() < 0.01);
}

#[test]
fn test_weakness_no_linkage_full_penalty() {
    let score = test_weakness_score(None, 1.0);
    assert!((score - 1.0).abs() < 0.01);
}
```

**Step 2: Update test_weakness_score**

```rust
pub fn test_weakness_score(best_tier: Option<&str>, confidence: f64) -> f64 {
    let raw_weakness = match best_tier {
        None => 1.0,
        Some("stub") => 0.9,
        Some("thin") => 0.6,
        Some("adequate") => 0.3,
        Some("thorough") => 0.1,
        Some("unknown") => 0.5,
        Some("n/a") => 0.5,
        _ => 0.5,
    };
    let confidence = confidence.clamp(0.0, 1.0);
    let neutral = 0.5;
    neutral + (raw_weakness - neutral) * confidence
}
```

**Step 3: Read confidence from test_linkage metadata**

In `compute_change_risk_scores`, where it reads `best_tier` from `test_linkage`:

```rust
let best_tier = meta.get("test_linkage")
    .and_then(|tl| tl.get("best_tier"))
    .and_then(|v| v.as_str());

let confidence = meta.get("test_linkage")
    .and_then(|tl| tl.get("best_confidence"))
    .and_then(|v| v.as_f64())
    .unwrap_or(0.5); // Default: moderate confidence for old data without the field

let test_weak = test_weakness_score(best_tier, confidence);
```

**Step 4: Run tests, commit**

```bash
cargo nextest run --lib test_weakness 2>&1 | tail -10
cargo nextest run --lib test_compute_change_risk 2>&1 | tail -10
```

```bash
git add src/analysis/change_risk.rs src/tests/analysis/change_risk_tests.rs
git commit -m "feat(analysis): confidence-gate test weakness in change_risk

test_weakness_score takes confidence from test_linkage aggregation.
Low-confidence tiers converge toward neutral (0.5). thorough maps
to 0.1 not 0.0 since static evidence lowers but cannot erase risk."
```

---

### Task 6: Update server-side consumers

**Files:**
- Modify: `src/tools/impact/mod.rs:448` (use is_test_related)
- Modify: `src/tools/impact/ranking.rs:106` (use is_test_related)
- Modify: `src/tools/deep_dive/data.rs:322` (use is_test_related)
- Modify: `src/tools/deep_dive/formatting.rs:186` (display test_role)
- Modify: `src/embeddings/metadata.rs:86` (use is_test_related)

**What to build:** Replace raw `metadata.get("is_test")` checks with the new helpers. Display test_role in deep_dive output.

The three server-side `is_test_symbol` / `symbol_is_test` functions all check `metadata["is_test"]` and fall back to `is_test_path`. Replace them with:

```rust
use crate::analysis::test_roles::is_test_related;

fn is_test_symbol(symbol: &Symbol) -> bool {
    is_test_related(symbol) || is_test_path(&symbol.file_path)
}
```

Update deep_dive formatting to show role:

```rust
fn format_test_quality_info(metadata: &serde_json::Value) -> Option<String> {
    let tier = metadata.get("test_quality")
        .and_then(|tq| tq.get("quality_tier"))
        .and_then(|v| v.as_str())?;
    let role = metadata.get("test_role")
        .and_then(|v| v.as_str())
        .unwrap_or("test");
    let confidence = metadata.get("test_quality")
        .and_then(|tq| tq.get("confidence"))
        .and_then(|v| v.as_f64());
    match confidence {
        Some(c) => Some(format!("  [{}] [{} confidence:{:.0}%]", role, tier, c * 100.0)),
        None => Some(format!("  [{}] [{}]", role, tier)),
    }
}
```

**Commit**

```bash
git add src/tools/ src/embeddings/
git commit -m "refactor(tools): use is_test_related helper, display test_role in deep_dive"
```

---

## Session 4: Signal Surfaces

### Task 7: Add scheduler signal and bump cache schema

**Files:**
- Modify: `src/analysis/early_warnings.rs` (add SchedulerSignal, bump schema)
- Test: add scheduler signal tests

**Step 1: Add SchedulerSignal struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulerSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
    pub annotation_key: String,
    pub raw_text: Option<String>,
}
```

Add `scheduler_signals: Vec<SchedulerSignal>` to `EarlyWarningReport`.
Add `scheduler_signals: usize` to `ReportSummary`.

**Step 2: Add scheduler to AnnotationSets and build_report**

Add `scheduler: HashSet<String>` to `AnnotationSets`. Populate from `config.annotation_classes.scheduler`. In `build_report`, detect scheduler annotations alongside existing signals.

**Step 3: Bump cache schema version**

In `early_warnings.rs`, increment `DEFAULT_CONFIG_SCHEMA_VERSION` from `1` to `2`. This invalidates cached reports so new fields are populated.

**Step 4: Run tests, commit**

---

### Task 8: Add linkage gap signals

**Files:**
- Modify: `src/analysis/early_warnings.rs` (add two new signal types)
- Modify: `src/cli_tools/` (update signals CLI formatter)
- Test: add linkage gap signal tests

**Step 1: Add signal structs with honest names**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryPointLinkageGap {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub entry_annotation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HighCentralityLinkageGap {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub reference_score: f64,
}
```

**Step 2: Implement in build_report**

Entry point linkage gaps: cross-reference `entry_points` with `test_linkage` metadata. For each entry point, check if the symbol has `metadata.test_linkage`. If not, add to gaps.

High-centrality linkage gaps: query DB for top-N centrality non-test symbols without test_linkage. Use `collect_high_centrality_linkage_gaps()` function querying:

```sql
SELECT id, name, kind, language, file_path, start_line, reference_score
FROM symbols
WHERE reference_score > 0
  AND (json_extract(metadata, '$.is_test') IS NULL OR json_extract(metadata, '$.is_test') != 1)
  AND json_extract(metadata, '$.test_linkage') IS NULL
ORDER BY reference_score DESC
LIMIT ?1
```

**Step 3: Update CLI output with honest framing**

All signal sections must use "observed" / "no linkage found" language:
- "No test linkage observed for these API endpoints"
- "These high-centrality symbols have no observed test coverage"

**Step 4: Run full test tier**

```bash
cargo xtask test dev 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add src/analysis/early_warnings.rs src/analysis/mod.rs src/cli_tools/ src/tests/analysis/
git commit -m "feat(signals): add scheduler, entry point linkage gaps, high-centrality gaps

Three new signal sections: SchedulerSignal for annotated scheduled tasks,
EntryPointLinkageGap for API endpoints with no observed test linkage,
HighCentralityLinkageGap for high-centrality symbols with no coverage.
Cache schema version bumped to invalidate stale reports."
```

---

## Verification

After all sessions complete:

1. `cargo xtask test dev` passes
2. `cargo xtask test dogfood` passes (search quality unaffected)
3. `cargo clippy` clean
4. `cargo fmt` clean
5. Run `julie-server signals --workspace . --standalone --json` and verify:
   - Scheduler signals appear for annotated symbols
   - Entry point linkage gaps appear
   - No false "stub" tiers on fixture functions
6. Run `julie-server signals --workspace . --standalone --format markdown` and verify honest framing language
7. Verify fixtures are classified as `fixture_setup`/`fixture_teardown` (not `test_case`)
8. Verify `deep_dive` displays role + confidence alongside tier

---

## Task Dependency Graph

```
Task 1 (types + config) ──→ Task 2 (classify + helpers + pipeline)
                                      │
                     ┌────────────────┤
                     ▼                ▼
           Task 3 (evidence     Task 6 (consumers)
            + linkage)
                     │
                     ▼
           Task 4 (quality model)
                     │
                     ▼
           Task 5 (risk gating)
                     │
                     ▼
           Task 7 (scheduler) ──→ Task 8 (linkage gaps)
```

Tasks 1-2 are serial. Task 3 and Task 6 can run in parallel after Task 2. Task 4 depends on Task 3. Tasks 5, 7, 8 are serial after Task 4.
