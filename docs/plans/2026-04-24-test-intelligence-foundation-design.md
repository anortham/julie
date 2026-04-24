# Test Intelligence Foundation and Signals Phase 2

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Strengthen Julie's foundational test intelligence so that downstream consumers (change_risk, deep_dive, blast_radius, signals) operate on trustworthy data. Then extend the signals report with security and test coverage signals built on that foundation.

**Architecture:** Test role classification moves from hardcoded per-language lists to config-driven TOML vocabulary. Test quality assessment replaces regex-over-text pattern counting with an evidence model using identifier data matched against framework-specific config. Quality tiers carry explicit confidence scores so downstream consumers can gate on reliability. Signal surfaces are added only after the foundation is proven.

**Motivation:** Julie's previous attempt to surface test and security data (the codehealth skill) was scrapped because the data was unreliable. The current test intelligence chain has specific weaknesses: `is_test_symbol()` conflates tests and fixtures, `test_quality.rs` scores fixtures on assertion density (producing misleading tiers), and `change_risk.rs` gives 30% weight to quality tiers whose confidence is unassessed. Building new report surfaces on this foundation repeats the same mistake. Julie fills a niche between zero insight and dedicated security/test tooling. That niche only works if what Julie says is trustworthy.

**Design consensus:** Two independent models (Claude Opus 4.6 and GPT-5.4 xhigh via Codex CLI) converged on the same diagnosis: the test/fixture conflation is the root bug, regex quality tiers carry too much authority for what they measure, and foundation must be fixed before adding surfaces.

---

## Non-Goals

- **Not a security scanner.** Julie does not claim taint tracking, exploit reachability, or vulnerability detection. Signals are structural observations, not findings.
- **Not a test coverage tool.** Julie does not run tests, measure line/branch coverage, or replace language-specific coverage tooling. "No test linkage observed" is not the same as "untested."
- **Not exhaustive.** Some languages lack annotation syntax entirely (Go, C, Lua, Bash). Test detection there stays name/path-based. We do not invent annotation semantics for languages that don't have them.
- **Not backward-compatible.** The annotation_classes config schema changes. Old TOML configs need updating. No shim layer.

---

## Part 1: Annotation Role Taxonomy

### Problem

`AnnotationClassesConfig` currently has two flat categories for test-related annotations:

```rust
pub test: Vec<String>,    // "things related to testing"
pub fixture: Vec<String>, // "things related to fixtures"
```

These are too coarse. `is_test_symbol()` returns `true` for both `@Test` and `@BeforeEach`, which means:
- `test_quality.rs` analyzes fixture functions for assertion density
- A `@BeforeEach` setup method with zero asserts gets classified as "stub"
- That "stub" tier feeds a 30% weight in `change_risk` scoring
- The signal is wrong: a fixture with no asserts is correct, not deficient

### Design

Replace `test` and `fixture` with a role taxonomy that captures what annotations tell us about a symbol's purpose:

```rust
pub struct TestAnnotationClasses {
    /// Functions that assert behavior: @Test, [Fact], #[test]
    pub test_case: Vec<String>,

    /// Data-driven test variants: [Theory], @ParameterizedTest, @TestCase
    pub parameterized_test: Vec<String>,

    /// Pre-test lifecycle: @BeforeEach, [SetUp], @pytest.fixture
    pub fixture_setup: Vec<String>,

    /// Post-test lifecycle: @AfterEach, [TearDown]
    pub fixture_teardown: Vec<String>,

    /// Classes/modules that hold tests: [TestFixture], @TestClass
    pub test_container: Vec<String>,
}
```

This replaces the existing `test` and `fixture` fields in `AnnotationClassesConfig`. The struct name `TestAnnotationClasses` is a nested struct within `AnnotationClassesConfig`:

```rust
pub struct AnnotationClassesConfig {
    pub entrypoint: Vec<String>,
    pub auth: Vec<String>,
    pub auth_bypass: Vec<String>,
    pub middleware: Vec<String>,
    pub scheduler: Vec<String>,
    pub test: TestAnnotationClasses,  // replaces flat Vec<String>
}
```

TOML representation:

```toml
[annotation_classes.test]
test_case = ["fact", "test", "testmethod"]
parameterized_test = ["theory", "testcase", "datatestmethod"]
fixture_setup = ["setup", "onetimesetup", "testinitialize", "classinitialize"]
fixture_teardown = ["teardown", "onetimeteardown", "testcleanup", "classcleanup"]
test_container = ["testfixture", "testclass", "collection"]
```

### Role Semantics

| Role | Quality scoring? | What it means |
|------|-----------------|---------------|
| `test_case` | Yes | This function asserts behavior. Assertion evidence is meaningful. |
| `parameterized_test` | Yes (adjusted) | Data-driven test. May have fewer explicit assertions per case because the framework iterates. |
| `fixture_setup` | No | Sets up test state. Zero assertions is correct behavior, not a deficiency. |
| `fixture_teardown` | No | Cleans up test state. Zero assertions is expected. |
| `test_container` | No (aggregate only) | Holds tests. Quality is aggregated from children, not measured directly. |

---

## Part 2: TOML Config Expansion

Every language with annotation syntax for test frameworks gets comprehensive role-classified entries. Languages without annotation-based test semantics (Go, Ruby, Swift, Elixir, JavaScript, TypeScript, C, C++, Lua, Zig, Bash, etc.) keep name/path-based detection unchanged.

### Java (JUnit 4, JUnit 5, TestNG)

```toml
[annotation_classes.test]
test_case = [
    "test",              # JUnit 4/5, TestNG
    "repeatedtest",      # JUnit 5
    "testfactory",       # JUnit 5
    "testtemplate",      # JUnit 5
]
parameterized_test = [
    "parameterizedtest", # JUnit 5
    "dataprovider",      # TestNG (on the data method; consumer has @Test)
]
fixture_setup = [
    "beforeeach",        # JUnit 5
    "beforeall",         # JUnit 5
    "before",            # JUnit 4
    "beforeclass",       # JUnit 4, TestNG
    "beforemethod",      # TestNG
    "beforesuite",       # TestNG
]
fixture_teardown = [
    "aftereach",         # JUnit 5
    "afterall",          # JUnit 5
    "after",             # JUnit 4
    "afterclass",        # JUnit 4, TestNG
    "aftermethod",       # TestNG
    "aftersuite",        # TestNG
]
test_container = [
    "nested",            # JUnit 5
]
```

### Kotlin (mirrors Java, plus kotlin.test)

```toml
[annotation_classes.test]
test_case = [
    "test",              # JUnit 4/5, TestNG, kotlin.test
    "repeatedtest",
    "testfactory",
    "testtemplate",
]
parameterized_test = [
    "parameterizedtest",
    "dataprovider",
]
fixture_setup = [
    "beforeeach", "beforeall", "before", "beforeclass",
    "beforemethod", "beforesuite",
    "beforetest",        # kotlin.test
]
fixture_teardown = [
    "aftereach", "afterall", "after", "afterclass",
    "aftermethod", "aftersuite",
    "aftertest",         # kotlin.test
]
test_container = ["nested"]
```

### C# (xUnit, NUnit, MSTest)

```toml
[annotation_classes.test]
test_case = [
    "fact",              # xUnit
    "test",              # NUnit
    "testmethod",        # MSTest
]
parameterized_test = [
    "theory",            # xUnit
    "testcase",          # NUnit
    "testcasesource",    # NUnit
    "datatestmethod",    # MSTest
]
fixture_setup = [
    "setup",             # NUnit
    "onetimesetup",      # NUnit
    "testinitialize",    # MSTest
    "classinitialize",   # MSTest
    "assemblyinitialize",# MSTest
]
fixture_teardown = [
    "teardown",          # NUnit
    "onetimeteardown",   # NUnit
    "testcleanup",       # MSTest
    "classcleanup",      # MSTest
    "assemblycleanup",   # MSTest
]
test_container = [
    "testfixture",       # NUnit
    "testclass",         # MSTest
    "collection",        # xUnit
    "collectiondefinition", # xUnit
]
```

### VB.NET (same frameworks as C#)

```toml
[annotation_classes.test]
test_case = ["fact", "test", "testmethod"]
parameterized_test = ["theory", "testcase", "testcasesource", "datatestmethod"]
fixture_setup = ["setup", "onetimesetup", "testinitialize", "classinitialize", "assemblyinitialize"]
fixture_teardown = ["teardown", "onetimeteardown", "testcleanup", "classcleanup", "assemblycleanup"]
test_container = ["testfixture", "testclass", "collection", "collectiondefinition"]
```

### Python (pytest, unittest)

```toml
[annotation_classes.test]
test_case = []
# Python tests are detected by name convention (test_ prefix), not annotation.
# pytest.mark.asyncio and pytest.mark.django_db are modifiers, not test markers.
parameterized_test = [
    "pytest.mark.parametrize",
]
fixture_setup = [
    "pytest.fixture",
]
fixture_teardown = []
# pytest fixtures handle teardown via yield/finalizer, no separate annotation.
test_container = []
```

### Rust

```toml
[annotation_classes.test]
test_case = [
    "test",              # std
    "tokio::test",       # tokio async tests
    "rstest",            # rstest framework
]
parameterized_test = []
# rstest handles parameterization but uses the same #[rstest] attribute.
fixture_setup = []
fixture_teardown = []
test_container = []
# Rust uses mod tests {} by convention, no annotation.
```

### PHP (PHPUnit)

```toml
[annotation_classes.test]
test_case = [
    "test",              # PHPUnit #[Test] or @test docblock
]
parameterized_test = [
    "dataprovider",      # PHPUnit #[DataProvider] or @dataProvider
]
fixture_setup = [
    "before",            # PHPUnit #[Before] or @before
    "beforeclass",       # PHPUnit #[BeforeClass] or @beforeClass
]
fixture_teardown = [
    "after",             # PHPUnit #[After] or @after
    "afterclass",        # PHPUnit #[AfterClass] or @afterClass
]
test_container = [
    "coversclass",       # PHPUnit #[CoversClass] (marks a test class)
]
```

### Scala (JUnit, ScalaTest, MUnit)

```toml
[annotation_classes.test]
test_case = [
    "test",              # JUnit-style
]
parameterized_test = [
    "parameterizedtest",
    "dataprovider",
]
fixture_setup = [
    "beforeeach", "beforeall", "before", "beforeclass",
]
fixture_teardown = [
    "aftereach", "afterall", "after", "afterclass",
]
test_container = []
# ScalaTest/MUnit use trait mixing, not annotations, for test containers.
```

### Dart

```toml
[annotation_classes.test]
test_case = [
    "istest",            # package:meta @isTest
]
parameterized_test = []
fixture_setup = []
fixture_teardown = []
test_container = [
    "istestgroup",       # package:meta @isTestGroup
]
```

### Razor (inherits C#)

```toml
[annotation_classes.test]
test_case = ["fact", "test", "testmethod"]
parameterized_test = ["theory", "testcase", "datatestmethod"]
fixture_setup = ["setup", "onetimesetup", "testinitialize", "classinitialize"]
fixture_teardown = ["teardown", "onetimeteardown", "testcleanup", "classcleanup"]
test_container = ["testfixture", "testclass", "collection"]
```

### Languages without test annotation support

The following languages use convention-based test detection (name patterns, file paths) and do not need `[annotation_classes.test]` sections: Go, JavaScript, TypeScript, Ruby, Swift, Elixir, C, C++, Lua, Zig, GDScript, Vue, QML, R, SQL, HTML, CSS, Bash, PowerShell, Markdown, JSON, TOML, YAML.

Test detection for these languages remains in `is_test_symbol()` using name/path heuristics, unchanged.

---

## Part 3: Config-Driven Test Classification

### Problem

`is_test_symbol()` in `crates/julie-extractors/src/test_detection.rs` hardcodes per-language annotation lists. The TOML configs define annotation_classes separately. Two sources of truth that don't talk to each other.

### Design: Config owns vocabulary, code owns policy

**TOML configs** define what annotations mean (vocabulary):
- "The annotation key `fact` in C# means `test_case`"
- "The annotation key `setup` in C# means `fixture_setup`"

**`is_test_symbol()`** interprets vocabulary plus structural signals (policy):
- "This symbol has a `test_case` annotation, so it's a test"
- "This symbol is named `test_foo` and lives in a test directory, so it's a test"
- "This symbol has a `fixture_setup` annotation, so it's test-related but not a test case"

### Architecture

The extractors crate cannot access server-side TOML configs (it's a separate crate with no dependency on the server). Two options:

**Option A: Post-extraction role classification in the indexing pipeline.**
Extractors continue setting `metadata.is_test` via the current `is_test_symbol()` with its hardcoded lists. The indexing pipeline adds a new step that reads TOML configs and classifies `metadata.test_role` based on the symbol's annotation_keys. `test_quality` and downstream consumers use `test_role` instead of raw `is_test`.

**Option B: Pass annotation role config into extractors.**
Create a shared config module that both the extractors crate and the server crate can access. `is_test_symbol()` reads role definitions from this shared config instead of hardcoded arrays.

**Recommendation: Option B.** Option A creates a two-pass system where the first pass (is_test_symbol with hardcoded lists) can disagree with the second pass (config-driven roles). Option B has a single source of truth. The shared config is a small struct passed into extractors at initialization; it doesn't require the extractors to depend on the full server.

The shared config struct:

```rust
/// Passed into extractors at init time. Populated from TOML configs.
pub struct TestRoleConfig {
    pub test_case: HashSet<String>,
    pub parameterized_test: HashSet<String>,
    pub fixture_setup: HashSet<String>,
    pub fixture_teardown: HashSet<String>,
    pub test_container: HashSet<String>,
}
```

`is_test_symbol()` signature becomes:

```rust
pub fn classify_test_role(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    annotation_keys: &[String],
    doc_comment: Option<&str>,
    role_config: Option<&TestRoleConfig>,
) -> Option<TestRole>
```

Returns `None` for non-test symbols, `Some(TestRole::TestCase)`, `Some(TestRole::FixtureSetup)`, etc. for test-related symbols. For languages without config (role_config is None), falls back to name/path heuristics and returns `TestRole::TestCase` for anything that matches (preserving current behavior for convention-based languages).

The existing `is_test` metadata flag becomes derived: `is_test = test_role.is_some()`.

### Migration

1. Add `TestRoleConfig` and `TestRole` to the extractors crate
2. Rename `is_test_symbol()` to `classify_test_role()` with the new signature
3. Update all 16+ extractor call sites to pass the role config and store `test_role` in metadata
4. Keep `is_test` as a derived field for backward compat with queries
5. Update TOML configs with the new `[annotation_classes.test]` structure
6. Remove hardcoded annotation lists from `test_detection.rs`

---

## Part 4: Evidence-Based Quality Model

### Problem

`test_quality.rs` uses regex pattern matching on code body text to count assertions, mocks, and error-testing markers. Then it classifies tiers:

- 3+ assertions and error testing: "thorough"
- 2+ assertions: "adequate"
- 1 assertion: "thin"
- 0 assertions: "stub"

This is framework-blind, cannot distinguish tests from fixtures, and treats its output as ground truth with no confidence signal. A `@pytest.fixture` with zero assertions gets labeled "stub." A `@BeforeEach` that initializes test data gets labeled "stub." Both are wrong.

### Design: Evidence model with confidence

Replace the regex oracle with multiple evidence sources, each contributing a signal and a confidence level:

**Evidence source 1: Test role (from Part 3)**
- Symbol's `test_role` determines whether quality scoring applies at all
- `fixture_setup`, `fixture_teardown`, `test_container`: not scored for assertions. Quality is "n/a".
- `test_case`, `parameterized_test`: scored for assertion evidence

**Evidence source 2: Assertion evidence (from identifiers)**
- Query the identifiers table for assertion function calls within the test symbol's scope
- Match against framework-specific assertion identifiers from a new `[test_evidence]` TOML section:

```toml
[test_evidence]
assertion_identifiers = [
    "assert_eq", "assert_ne", "assert", "assert_matches",
    "debug_assert", "debug_assert_eq",
]
error_assertion_identifiers = [
    "should_err", "should_panic",
]
mock_identifiers = []
```

Per-language examples:

```toml
# Java
[test_evidence]
assertion_identifiers = [
    "assertequals", "asserttrue", "assertfalse", "assertnull",
    "assertnotnull", "assertthrows", "assertarrayequals",
    "assertthat", "assertdoesnotthrow",
    "verify", "verifynomoreinteractions",
]
error_assertion_identifiers = [
    "assertthrows", "expectederror",
]
mock_identifiers = [
    "mock", "spy", "when", "thenreturn", "verify",
]
```

```toml
# C#
[test_evidence]
assertion_identifiers = [
    "assert.equal", "assert.true", "assert.false", "assert.null",
    "assert.notnull", "assert.throws", "assert.throwsasync",
    "assert.contains", "assert.empty", "assert.istype",
    "assert.collection", "assert.single",
    "assert.that", "assert.istrue", "assert.isfalse",
    "assert.areequal", "assert.arenotequal",
    "should", "shouldbe", "shouldcontain",
]
error_assertion_identifiers = [
    "assert.throws", "assert.throwsasync",
    "assert.catch", "assert.throwsany",
]
mock_identifiers = [
    "mock", "substitute.for", "a.fake",
    "setup", "returns", "verifiable",
]
```

```toml
# Python
[test_evidence]
assertion_identifiers = [
    "assert", "assertequal", "asserttrue", "assertfalse",
    "assertin", "assertnotin", "assertisnone", "assertisnotnone",
    "assertraises", "assertwarns", "assertlogs",
    "pytest.raises", "pytest.warns", "pytest.approx",
]
error_assertion_identifiers = [
    "assertraises", "pytest.raises",
]
mock_identifiers = [
    "mock", "magicmock", "patch", "mock_open",
    "asyncmock", "create_autospec",
]
```

The identifier query:

```sql
SELECT COUNT(*) FROM identifiers
WHERE containing_symbol_id = ?test_id
AND LOWER(name) IN (?assertion_identifiers)
```

**Important caveat:** Identifier extraction coverage varies by language. Rust macro calls (`assert_eq!`) may not appear as identifiers depending on tree-sitter grammar handling. The design must verify identifier extraction for assertion calls per language before relying on it. Where identifier data is insufficient, the existing regex approach serves as a fallback with lower confidence.

**Evidence source 3: Linkage breadth (from test_linkage)**
- How many production symbols does this test reference?
- A test that touches one function is narrower than one that exercises a full workflow
- This data already exists in `test_linkage` metadata

**Evidence source 4: Body presence**
- Does the test have a code body at all?
- A function with `pass`, `...`, `TODO`, or an empty body is definitively a stub
- This check is simple and high-confidence

### Tier classification with confidence

```rust
pub struct TestQualityAssessment {
    pub tier: TestQualityTier,
    pub confidence: f32,       // 0.0 to 1.0
    pub evidence: QualityEvidence,
}

pub enum TestQualityTier {
    Thorough,   // Multiple assertions, error testing, good linkage breadth
    Adequate,   // Some assertions, reasonable coverage
    Thin,       // Minimal assertions
    Stub,       // No body, empty, or placeholder
    Unknown,    // Insufficient evidence to classify
    NotApplicable, // Fixture, teardown, container (not scored)
}

pub struct QualityEvidence {
    pub assertion_count: u32,
    pub assertion_source: EvidenceSource,  // Identifier, Regex, None
    pub has_error_testing: bool,
    pub mock_count: u32,
    pub body_lines: u32,
    pub linkage_breadth: u32,
}

pub enum EvidenceSource {
    Identifier,  // High confidence: matched against framework config
    Regex,       // Medium confidence: regex pattern on code text (fallback)
    None,        // No evidence available
}
```

Classification rules:

| Condition | Tier | Confidence |
|-----------|------|------------|
| Role is fixture/teardown/container | NotApplicable | 1.0 |
| No code body or placeholder body | Stub | 1.0 |
| No assertion evidence available | Unknown | 0.0 |
| Identifier-based: 3+ assertions + error testing | Thorough | 0.9 |
| Identifier-based: 2+ assertions | Adequate | 0.85 |
| Identifier-based: 1 assertion | Thin | 0.8 |
| Identifier-based: 0 assertions in test_case | Stub | 0.85 |
| Regex-based: 3+ assertions + error testing | Thorough | 0.5 |
| Regex-based: 2+ assertions | Adequate | 0.45 |
| Regex-based: 1 assertion | Thin | 0.4 |
| Regex-based: 0 assertions | Unknown | 0.3 |

Key difference from current approach: regex with 0 assertions produces `Unknown`, not `Stub`. We admit we don't know rather than claiming the test is deficient.

---

## Part 5: Confidence-Gated Change Risk

### Problem

`change_risk.rs` gives 30% weight to test weakness:

```rust
const W_CENTRALITY: f64 = 0.35;
const W_VISIBILITY: f64 = 0.25;
const W_TEST_WEAKNESS: f64 = 0.30;
const W_KIND: f64 = 0.10;
```

The `test_weakness_score` function maps quality tiers to scores, but does not consider confidence. A `Stub` tier from unreliable regex analysis gets the same treatment as a high-confidence `Stub` from an empty function body.

### Design

`test_weakness_score` incorporates confidence:

```rust
pub fn test_weakness_score(best_tier: Option<&str>, confidence: f64) -> f64 {
    let raw_weakness = match best_tier {
        None => 1.0,                  // No test linkage at all
        Some("stub") => 0.9,
        Some("thin") => 0.6,
        Some("adequate") => 0.3,
        Some("thorough") => 0.0,
        Some("unknown") => 0.5,       // Hedge: unknown is not great but not awful
        Some("n/a") => 0.5,           // Fixture-only coverage, uncertain
        _ => 0.5,
    };
    // Scale by confidence: low confidence pulls the weakness toward neutral (0.5)
    let neutral = 0.5;
    neutral + (raw_weakness - neutral) * confidence
}
```

When confidence is 1.0, the score is unmodified. When confidence is 0.0, the score collapses to 0.5 (neutral, neither hurting nor helping). This prevents low-confidence assessments from dominating the risk score.

The W_TEST_WEAKNESS weight (0.30) stays the same. The gating happens at the evidence level, not the weight level. If we later prove that identifier-based quality assessment is highly reliable, the full 30% weight takes effect naturally.

---

## Part 6: Signals Phase 2 (After Foundation)

These signal additions depend on Parts 1-5 being complete and validated. They are not implemented until the foundation is proven trustworthy.

### 6A. Scheduler Signal

New section in `EarlyWarningReport`: symbols with scheduler annotations (`@Scheduled`, `@celery.task`, `@periodic_task`, etc.). Already extracted and classified in TOML configs for Java, Kotlin, and Python. Structural fact, no guesswork.

Signal framing: "These symbols execute on a schedule without a request context. Verify they do not access user-specific data or credentials without appropriate authorization."

Expand scheduler annotations for additional languages where applicable:
- PHP: cron-related annotations if frameworks use them
- C#: Hangfire `[AutomaticRetry]`, Quartz `[DisallowConcurrentExecution]` if applicable

### 6B. Untested Entry Point Signal

Cross-reference `EntryPointSignal` (already computed) with `test_linkage` metadata (already computed). Report entry points with no observed test linkage.

Signal framing: "These API endpoints have no observed test linkage. This does not mean they are untested; it means Julie's structural analysis found no test symbols that reference them."

Acceptance criteria: only surface when test_linkage pipeline has run successfully and the entry point's language has reasonable identifier extraction coverage. Do not surface for languages where test_linkage has low structural coverage.

### 6C. High-Centrality Untested Signal

Top-N production symbols (by centrality score) with no test linkage. These are well-connected symbols where a bug has wide blast radius and no test safety net.

Signal framing: "These high-centrality symbols have no observed test coverage. Changes to them affect many other symbols."

---

## Part 7: Test Evidence TOML Section

Each language TOML gains a `[test_evidence]` section listing framework-specific function/method names used for assertions, error testing, and mocking. These are the identifiers that the evidence model matches against.

**This section requires verification against actual identifier extraction per language.** Before populating a language's `[test_evidence]`, verify that the relevant assertion function calls appear in Julie's identifiers table for that language. If they don't (e.g., Rust macro calls may not be extracted as identifiers), note the gap and rely on regex fallback with lower confidence.

### Verification approach

For each language with test_evidence entries:

1. Index a project that uses the framework
2. Find a test function that calls assertion functions
3. Query `SELECT name FROM identifiers WHERE containing_symbol_id = ?test_id`
4. Confirm that assertion function names appear in the results
5. If they don't, investigate whether the tree-sitter grammar or extractor needs adjustment, or flag the language for regex fallback

### Example entries (to be verified during implementation)

```toml
# rust.toml
[test_evidence]
assertion_identifiers = ["assert_eq", "assert_ne", "assert", "assert_matches", "debug_assert"]
error_assertion_identifiers = ["should_err", "should_panic"]
mock_identifiers = []

# python.toml
[test_evidence]
assertion_identifiers = [
    "assert", "assertequal", "asserttrue", "assertfalse",
    "assertin", "assertisnone", "assertraises",
]
error_assertion_identifiers = ["assertraises", "pytest.raises"]
mock_identifiers = ["mock", "magicmock", "patch", "mock_open"]

# java.toml
[test_evidence]
assertion_identifiers = [
    "assertequals", "asserttrue", "assertfalse", "assertnull",
    "assertnotnull", "assertthrows", "assertthat",
]
error_assertion_identifiers = ["assertthrows"]
mock_identifiers = ["mock", "spy", "when", "verify"]

# csharp.toml
[test_evidence]
assertion_identifiers = [
    "equal", "true", "false", "null", "notnull",
    "throws", "throwsasync", "contains", "empty",
    "istype", "collection", "single",
    "istrue", "isfalse", "areequal",
]
error_assertion_identifiers = ["throws", "throwsasync", "catch"]
mock_identifiers = ["mock", "setup", "returns", "verifiable"]
```

Note: C# assertion identifiers are listed without the `Assert.` prefix because Julie's identifier extraction typically captures the method name (`Equal`) rather than the qualified call (`Assert.Equal`). This must be verified during implementation.

---

## Architecture Summary

### Current flow

```
Extractor
  → is_test_symbol(hardcoded lists) → metadata.is_test = true/false
  → extract annotations → AnnotationMarker[]

Pipeline
  → test_quality(regex on code body) → metadata.test_quality.quality_tier
  → test_linkage(relationships + identifiers) → metadata.test_linkage
  → change_risk(centrality + visibility + test_weakness + kind) → metadata.change_risk
```

### Proposed flow

```
Extractor
  → classify_test_role(config-driven) → metadata.test_role = TestCase/FixtureSetup/...
  → metadata.is_test = test_role.is_some()   (derived, backward compat)
  → extract annotations → AnnotationMarker[]

Pipeline
  → test_quality_evidence(role-aware, identifier + fallback regex)
      → metadata.test_quality.tier + confidence + evidence
  → test_linkage(unchanged, uses is_test for gating)
  → change_risk(confidence-gated test_weakness) → metadata.change_risk
```

### File changes

| File | Change |
|------|--------|
| `src/search/language_config.rs` | Replace flat `test`/`fixture` with `TestAnnotationClasses` struct, add `TestEvidenceConfig` |
| `languages/*.toml` | Update all applicable configs with role taxonomy and test_evidence |
| `crates/julie-extractors/src/test_detection.rs` | Rename to `test_classification.rs`, `classify_test_role()` consuming `TestRoleConfig` |
| `crates/julie-extractors/src/base/types.rs` | Add `TestRole` enum, `TestRoleConfig` struct |
| All 16+ extractor files | Update call sites to pass role config, store `test_role` |
| `src/analysis/test_quality.rs` | Replace regex oracle with evidence model |
| `src/analysis/change_risk.rs` | Add confidence gating to `test_weakness_score` |
| `src/analysis/early_warnings.rs` | Add scheduler, untested entry point, high-centrality signals |
| `src/tools/workspace/indexing/pipeline.rs` | Pass `TestRoleConfig` to extractors, pass evidence config to test_quality |

---

## Implementation Sequence

1. **Annotation role taxonomy and TOML expansion** (Parts 1-2)
   - Define `TestAnnotationClasses`, `TestRole`, `TestRoleConfig`
   - Update all applicable language TOML configs
   - This is config and type work; no behavioral change yet

2. **Config-driven test classification** (Part 3)
   - Implement `classify_test_role()` replacing `is_test_symbol()`
   - Wire config through extractors
   - Verify: existing `is_test` behavior preserved, `test_role` added

3. **Evidence-based quality model** (Part 4)
   - Verify identifier extraction for assertion calls per language
   - Add `[test_evidence]` TOML sections for verified languages
   - Replace regex oracle with evidence model
   - Add confidence to quality tiers
   - Verify: fixtures no longer scored, `Unknown` replaces false `Stub`

4. **Confidence-gated change risk** (Part 5)
   - Update `test_weakness_score` to incorporate confidence
   - Verify: low-confidence tiers don't dominate risk scores

5. **Signal surfaces** (Part 6, after foundation validated)
   - Scheduler signal
   - Untested entry point signal
   - High-centrality untested signal

---

## Acceptance Criteria

### Part 1-2: Role Taxonomy and TOML Expansion
- [ ] `AnnotationClassesConfig` uses `TestAnnotationClasses` struct with five role fields
- [ ] All 10 applicable language TOMLs have comprehensive `[annotation_classes.test]` sections
- [ ] All three major C# frameworks (xUnit, NUnit, MSTest) fully represented
- [ ] All three major Java frameworks (JUnit 4, JUnit 5, TestNG) fully represented
- [ ] Python pytest.fixture classified as `fixture_setup`, not `test`
- [ ] Existing tests pass (TOML config parsing, annotation classification)

### Part 3: Config-Driven Classification
- [ ] `classify_test_role()` replaces `is_test_symbol()` across all extractor call sites
- [ ] `TestRoleConfig` populated from TOML configs and passed to extractors
- [ ] `metadata.test_role` set on all test-related symbols
- [ ] `metadata.is_test` derived from `test_role.is_some()` (backward compat)
- [ ] Languages without annotation config fall back to name/path heuristics
- [ ] Hardcoded annotation lists removed from `test_detection.rs`
- [ ] Existing test detection accuracy preserved (no regressions in fixture DB)

### Part 4: Evidence-Based Quality
- [ ] Identifier extraction verified for assertion calls in at least Rust, Python, Java, C#
- [ ] `[test_evidence]` sections added for verified languages
- [ ] `test_quality` skips fixture_setup, fixture_teardown, test_container roles
- [ ] `test_quality` produces `TestQualityAssessment` with tier + confidence + evidence
- [ ] `Unknown` tier used when evidence is insufficient (not false `Stub`)
- [ ] Regex fallback used for languages without verified identifier extraction
- [ ] deep_dive displays confidence alongside tier
- [ ] blast_radius respects new tier semantics

### Part 5: Confidence-Gated Risk
- [ ] `test_weakness_score` accepts confidence parameter
- [ ] Low-confidence tiers converge toward neutral (0.5)
- [ ] change_risk scores stable for high-confidence tiers
- [ ] No behavioral change for symbols with no test linkage (confidence irrelevant)

### Part 6: Signal Surfaces
- [ ] SchedulerSignal section in EarlyWarningReport with symbols bearing scheduler annotations
- [ ] Untested entry point signal cross-referencing entry points with test_linkage
- [ ] High-centrality untested signal for top-N production symbols
- [ ] All signal framing uses honest language: "observed," "no linkage found," not "untested" or "vulnerable"
- [ ] Signals only surface for languages with reasonable structural coverage
