# Test Intelligence Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Replace Julie's test/fixture conflation and regex-based quality oracle with config-driven role classification and evidence-based quality assessment, then add security and test coverage signals on the trustworthy foundation.

**Architecture:** `TestRole` enum and `TestRoleConfig` stored via `OnceLock` in the extractors crate, populated from TOML language configs at server startup. `classify_test_role()` replaces `is_test_symbol()`, setting `metadata.test_role` alongside backward-compat `metadata.is_test`. Evidence-based quality model queries identifier data against framework-specific `[test_evidence]` TOML config, with regex fallback at lower confidence. Quality tiers carry confidence scores that gate the 30% test_weakness weight in change_risk.

**Tech Stack:** Rust, serde, tree-sitter, TOML configs, SQLite (identifiers table), existing Julie pipeline infrastructure

**Design doc:** `docs/plans/2026-04-24-test-intelligence-foundation-design.md`

---

## Session 1: Foundation Types & Config

### Task 1: Define TestRole, TestRoleConfig, and global config store

**Files:**
- Create: `crates/julie-extractors/src/test_roles.rs`
- Modify: `crates/julie-extractors/src/lib.rs:41` (add module + re-export)
- Test: `crates/julie-extractors/src/tests/test_detection.rs` (add role tests at bottom)

**What to build:** The core types that everything else depends on. A `TestRole` enum, a `TestRoleConfig` struct holding annotation-key sets per role, and a `OnceLock`-backed global store so extractors can access role configs without signature changes to the 30+ function extraction pipeline.

**Step 1: Write failing tests**

Add to `crates/julie-extractors/src/tests/test_detection.rs`:

```rust
// --- Test role classification ---

use crate::test_roles::{TestRole, TestRoleConfig, init_test_role_configs, classify_test_role};
use std::collections::{HashMap, HashSet};

fn make_csharp_role_config() -> TestRoleConfig {
    TestRoleConfig {
        test_case: HashSet::from(["fact".into(), "test".into(), "testmethod".into()]),
        parameterized_test: HashSet::from(["theory".into(), "testcase".into(), "datatestmethod".into()]),
        fixture_setup: HashSet::from(["setup".into(), "onetimesetup".into(), "testinitialize".into()]),
        fixture_teardown: HashSet::from(["teardown".into(), "onetimeteardown".into(), "testcleanup".into()]),
        test_container: HashSet::from(["testfixture".into(), "testclass".into()]),
    }
}

#[test]
fn classify_csharp_fact_as_test_case() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "MyTest", "src/Tests/MyTests.cs",
        &SymbolKind::Method, &[s("fact")], None, Some(&config),
    );
    assert_eq!(role, Some(TestRole::TestCase));
}

#[test]
fn classify_csharp_setup_as_fixture() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "SetUp", "src/Tests/MyTests.cs",
        &SymbolKind::Method, &[s("setup")], None, Some(&config),
    );
    assert_eq!(role, Some(TestRole::FixtureSetup));
}

#[test]
fn classify_csharp_theory_as_parameterized() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "DataTest", "src/Tests/MyTests.cs",
        &SymbolKind::Method, &[s("theory")], None, Some(&config),
    );
    assert_eq!(role, Some(TestRole::ParameterizedTest));
}

#[test]
fn classify_csharp_teardown_as_fixture_teardown() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "Cleanup", "src/Tests/MyTests.cs",
        &SymbolKind::Method, &[s("teardown")], None, Some(&config),
    );
    assert_eq!(role, Some(TestRole::FixtureTeardown));
}

#[test]
fn classify_go_test_without_config_falls_back() {
    let role = classify_test_role(
        "go", "TestFoo", "pkg/handler_test.go",
        &SymbolKind::Function, &[], None, None,
    );
    assert_eq!(role, Some(TestRole::TestCase));
}

#[test]
fn classify_regular_function_returns_none() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "ProcessData", "src/Services/DataService.cs",
        &SymbolKind::Method, &[], None, Some(&config),
    );
    assert_eq!(role, None);
}

#[test]
fn classify_class_returns_none_not_callable() {
    let config = make_csharp_role_config();
    let role = classify_test_role(
        "csharp", "TestFixture", "src/Tests/MyTests.cs",
        &SymbolKind::Class, &[s("testfixture")], None, Some(&config),
    );
    // test_container applies to classes, but classify_test_role gates on callable kinds
    // for test_case/parameterized/fixture roles. test_container is the exception.
    assert_eq!(role, Some(TestRole::TestContainer));
}
```

**Step 2: Run tests, verify they fail**

```bash
cargo nextest run --lib classify_csharp_fact 2>&1 | tail -5
```
Expected: compilation error (module doesn't exist yet)

**Step 3: Implement test_roles.rs**

Create `crates/julie-extractors/src/test_roles.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::base::SymbolKind;

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

    pub fn is_test_related(&self, annotation_key: &str) -> bool {
        self.classify_annotation(annotation_key).is_some()
    }
}

static TEST_ROLE_CONFIGS: OnceLock<HashMap<String, TestRoleConfig>> = OnceLock::new();

pub fn init_test_role_configs(configs: HashMap<String, TestRoleConfig>) {
    TEST_ROLE_CONFIGS.set(configs).ok();
}

pub fn get_role_config(language: &str) -> Option<&'static TestRoleConfig> {
    TEST_ROLE_CONFIGS.get()?.get(language)
}

fn is_callable(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
    )
}

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Module | SymbolKind::Namespace
    )
}

pub fn classify_test_role(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    annotation_keys: &[String],
    doc_comment: Option<&str>,
    role_config: Option<&TestRoleConfig>,
) -> Option<TestRole> {
    // Try config-driven classification first
    if let Some(config) = role_config {
        // Check annotations against role config
        for key in annotation_keys {
            if let Some(role) = config.classify_annotation(key) {
                // test_container applies to container kinds
                if role == TestRole::TestContainer {
                    if is_container_kind(kind) {
                        return Some(role);
                    }
                    continue;
                }
                // All other roles require callable kinds
                if is_callable(kind) {
                    return Some(role);
                }
            }
        }
    }

    // Fall back to name/path heuristics for convention-based languages
    if is_callable(kind) {
        if let Some(role) = detect_by_convention(language, name, file_path, annotation_keys, doc_comment) {
            return Some(role);
        }
    }

    None
}

fn detect_by_convention(
    language: &str,
    name: &str,
    file_path: &str,
    annotation_keys: &[String],
    doc_comment: Option<&str>,
) -> Option<TestRole> {
    // Delegate to language-specific convention detection
    // These cover languages without annotation-based test frameworks
    let is_test = match language {
        "go" => detect_go(name, file_path),
        "javascript" | "typescript" => detect_js_ts(name, file_path),
        "ruby" => detect_ruby(name, file_path),
        "swift" => detect_swift(name, file_path),
        "elixir" => detect_elixir(name, file_path),
        "php" => detect_php_convention(name, file_path, doc_comment),
        "python" => detect_python_convention(name),
        "scala" => detect_scala_convention(name, file_path),
        _ => detect_generic(name, file_path),
    };
    if is_test { Some(TestRole::TestCase) } else { None }
}

// Convention-based detectors (moved from test_detection.rs, unchanged logic)

fn is_test_path(file_path: &str) -> bool {
    for segment in file_path.split('/') {
        match segment {
            "test" | "tests" | "Test" | "Tests" | "spec" | "Spec" | "__tests__" | "autotests" => {
                return true;
            }
            _ => {}
        }
        if segment.ends_with(".Tests") || segment.ends_with(".Test") {
            return true;
        }
    }
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    if file_name.ends_with("_test.go")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || file_name.starts_with("test_")
        || file_name.starts_with("tst_")
    {
        return true;
    }
    false
}

fn detect_go(name: &str, file_path: &str) -> bool {
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    (name.starts_with("Test") || name.starts_with("Fuzz") || name.starts_with("Example"))
        && file_name.ends_with("_test.go")
}

fn detect_js_ts(name: &str, file_path: &str) -> bool {
    let is_test_fn = matches!(name, "describe" | "it" | "test");
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    let in_test_file =
        file_name.contains(".test.") || file_name.contains(".spec.") || is_test_path(file_path);
    is_test_fn && in_test_file
}

fn detect_ruby(name: &str, file_path: &str) -> bool {
    name.starts_with("test_") && is_test_path(file_path)
}

fn detect_swift(name: &str, file_path: &str) -> bool {
    is_test_path(file_path)
        && (name.starts_with("test")
            || matches!(
                name,
                "setUp" | "tearDown" | "setUpWithError" | "tearDownWithError"
            ))
}

fn detect_elixir(name: &str, file_path: &str) -> bool {
    name.starts_with("test_") || name.starts_with("test ") || is_test_path(file_path)
}

fn detect_php_convention(name: &str, file_path: &str, doc_comment: Option<&str>) -> bool {
    if let Some(doc) = doc_comment {
        if doc.contains("@test") {
            return true;
        }
    }
    name.starts_with("test") && is_test_path(file_path)
}

fn detect_python_convention(name: &str) -> bool {
    if matches!(name, "setUp" | "tearDown" | "setUpClass" | "tearDownClass") {
        return true;
    }
    name.starts_with("test_")
}

fn detect_scala_convention(name: &str, file_path: &str) -> bool {
    if is_test_path(file_path) {
        return true;
    }
    name.starts_with("test")
}

fn detect_generic(name: &str, file_path: &str) -> bool {
    let has_test_name = name.starts_with("test_") || name.starts_with("Test");
    has_test_name && is_test_path(file_path)
}
```

**Step 4: Wire up the module**

In `crates/julie-extractors/src/lib.rs`, add after line 41 (`pub mod test_detection`):

```rust
pub mod test_roles;
```

And add to the re-exports (after line 90):

```rust
pub use test_roles::{TestRole, TestRoleConfig, classify_test_role, init_test_role_configs, get_role_config};
```

**Step 5: Run tests, verify they pass**

```bash
cargo nextest run --lib classify_csharp 2>&1 | tail -10
cargo nextest run --lib classify_go_test 2>&1 | tail -5
cargo nextest run --lib classify_regular 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/test_roles.rs crates/julie-extractors/src/lib.rs crates/julie-extractors/src/tests/test_detection.rs
git commit -m "feat(extractors): add TestRole enum, TestRoleConfig, and classify_test_role

Introduces config-driven test role classification with five roles:
TestCase, ParameterizedTest, FixtureSetup, FixtureTeardown, TestContainer.

Uses OnceLock global for config storage to avoid threading through
30+ extraction pipeline function signatures. Falls back to
convention-based detection for languages without annotation configs."
```

---

### Task 2: Update AnnotationClassesConfig and all language TOMLs

**Files:**
- Modify: `src/search/language_config.rs:73-88` (replace flat test/fixture with TestAnnotationClasses)
- Modify: `src/search/language_config.rs:248-456` (update tests)
- Modify: `languages/java.toml`
- Modify: `languages/kotlin.toml`
- Modify: `languages/csharp.toml`
- Modify: `languages/python.toml`
- Modify: `languages/rust.toml`
- Modify: `languages/typescript.toml`
- Modify: `languages/javascript.toml`
- Modify: `languages/php.toml` (if exists, otherwise create section)
- Modify: `languages/scala.toml` (if exists, otherwise create section)
- Modify: `languages/dart.toml` (if exists, otherwise create section)
- Modify: `languages/vbnet.toml` (if exists, otherwise create section)
- Modify: `languages/razor.toml` (if exists, otherwise create section)

**Step 1: Write failing test**

Update `src/search/language_config.rs` test `test_embedded_language_configs_include_expected_annotation_classes` to expect the new structure:

```rust
#[test]
fn test_annotation_classes_use_role_taxonomy() {
    let configs = LanguageConfigs::load_embedded();
    let csharp = configs.get("csharp").expect("csharp config");

    // xUnit
    assert!(csharp.annotation_classes.test.test_case.contains(&"fact".to_string()));
    // NUnit
    assert!(csharp.annotation_classes.test.test_case.contains(&"test".to_string()));
    // MSTest
    assert!(csharp.annotation_classes.test.test_case.contains(&"testmethod".to_string()));

    // Fixtures are NOT in test_case
    assert!(!csharp.annotation_classes.test.test_case.contains(&"setup".to_string()));

    // Fixtures are in fixture_setup
    assert!(csharp.annotation_classes.test.fixture_setup.contains(&"setup".to_string()));
    assert!(csharp.annotation_classes.test.fixture_setup.contains(&"onetimesetup".to_string()));

    // Teardown
    assert!(csharp.annotation_classes.test.fixture_teardown.contains(&"teardown".to_string()));

    // Parameterized
    assert!(csharp.annotation_classes.test.parameterized_test.contains(&"theory".to_string()));

    // Containers
    assert!(csharp.annotation_classes.test.test_container.contains(&"testfixture".to_string()));
}

#[test]
fn test_java_role_taxonomy_covers_junit4_and_5() {
    let configs = LanguageConfigs::load_embedded();
    let java = configs.get("java").expect("java config");

    // JUnit 5
    assert!(java.annotation_classes.test.test_case.contains(&"test".to_string()));
    assert!(java.annotation_classes.test.test_case.contains(&"repeatedtest".to_string()));
    assert!(java.annotation_classes.test.parameterized_test.contains(&"parameterizedtest".to_string()));

    // JUnit 4 fixtures
    assert!(java.annotation_classes.test.fixture_setup.contains(&"before".to_string()));
    assert!(java.annotation_classes.test.fixture_setup.contains(&"beforeclass".to_string()));

    // JUnit 5 fixtures
    assert!(java.annotation_classes.test.fixture_setup.contains(&"beforeeach".to_string()));
    assert!(java.annotation_classes.test.fixture_setup.contains(&"beforeall".to_string()));

    // Teardown (both versions)
    assert!(java.annotation_classes.test.fixture_teardown.contains(&"aftereach".to_string()));
    assert!(java.annotation_classes.test.fixture_teardown.contains(&"after".to_string()));
}

#[test]
fn test_python_fixture_classified_correctly() {
    let configs = LanguageConfigs::load_embedded();
    let python = configs.get("python").expect("python config");

    // pytest.fixture is fixture_setup, not test_case
    assert!(python.annotation_classes.test.fixture_setup.contains(&"pytest.fixture".to_string()));
    assert!(!python.annotation_classes.test.test_case.contains(&"pytest.fixture".to_string()));
}

#[test]
fn test_languages_without_test_annotations_have_empty_sections() {
    let configs = LanguageConfigs::load_embedded();
    let go = configs.get("go").expect("go config");

    assert!(go.annotation_classes.test.test_case.is_empty());
    assert!(go.annotation_classes.test.fixture_setup.is_empty());
}
```

**Step 2: Run test, verify it fails**

```bash
cargo nextest run --lib test_annotation_classes_use_role 2>&1 | tail -5
```

**Step 3: Update AnnotationClassesConfig in language_config.rs**

Replace `AnnotationClassesConfig` at `src/search/language_config.rs:73-88`:

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

Replace `[annotation_classes]` sections. Each language's complete `[annotation_classes]` and `[annotation_classes.test]` sections are specified in the design doc at `docs/plans/2026-04-24-test-intelligence-foundation-design.md` Part 2. Copy them verbatim.

For languages that currently have flat `test = [...]` and `fixture = [...]` fields, these must be replaced with the nested `[annotation_classes.test]` table.

Example for `languages/csharp.toml` - replace existing annotation_classes:

```toml
[annotation_classes]
entrypoint = [
    "apicontroller",
    "route",
    "httpget",
    "httppost",
    "httpput",
    "httpdelete",
    "httppatch",
]
auth = ["authorize"]
auth_bypass = ["allowanonymous"]

[annotation_classes.test]
test_case = ["fact", "test", "testmethod"]
parameterized_test = ["theory", "testcase", "testcasesource", "datatestmethod"]
fixture_setup = ["setup", "onetimesetup", "testinitialize", "classinitialize", "assemblyinitialize"]
fixture_teardown = ["teardown", "onetimeteardown", "testcleanup", "classcleanup", "assemblycleanup"]
test_container = ["testfixture", "testclass", "collection", "collectiondefinition"]
```

For languages WITHOUT test annotations (go, c, cpp, lua, zig, etc.), no `[annotation_classes.test]` section is needed; the `#[serde(default)]` produces empty vecs.

**Step 5: Update existing tests in language_config.rs**

The existing test `test_language_config_defaults_empty_annotation_sections` checks `config.annotation_classes.test.is_empty()` which won't compile since `test` is now `TestAnnotationClasses`, not `Vec<String>`. Update:

```rust
assert!(config.annotation_classes.test.test_case.is_empty());
assert!(config.annotation_classes.test.fixture_setup.is_empty());
```

The test `test_language_config_loads_populated_annotation_sections` and `test_embedded_language_configs_include_expected_annotation_classes` need similar updates. Replace assertions that check the old flat `test`/`fixture` fields with assertions against the new role fields.

**Step 6: Fix early_warnings.rs annotation_sets function**

The `annotation_sets` function in `src/analysis/early_warnings.rs:318-340` reads `config.annotation_classes.test` and `config.annotation_classes.fixture`. It only uses these fields indirectly through `AnnotationSets.auth` (not test/fixture). Verify it doesn't reference the old flat fields. Currently it does NOT read test/fixture, so no change needed here.

**Step 7: Run tests, verify they pass**

```bash
cargo nextest run --lib test_annotation_classes 2>&1 | tail -10
cargo nextest run --lib test_java_role 2>&1 | tail -5
cargo nextest run --lib test_python_fixture 2>&1 | tail -5
cargo nextest run --lib test_language_config 2>&1 | tail -10
```

**Step 8: Commit**

```bash
git add src/search/language_config.rs languages/*.toml
git commit -m "feat(config): replace flat test/fixture with role taxonomy

AnnotationClassesConfig.test is now TestAnnotationClasses with five
role fields: test_case, parameterized_test, fixture_setup,
fixture_teardown, test_container.

All 10 applicable language TOMLs updated with comprehensive
framework-specific role classifications covering xUnit/NUnit/MSTest
(C#), JUnit 4/5/TestNG (Java/Kotlin), pytest (Python), PHPUnit,
and framework-specific annotations for Rust, Scala, Dart, VB.NET."
```

---

### Task 3: Build TestRoleConfig from LanguageConfigs and init at startup

**Files:**
- Modify: `src/search/language_config.rs` (add `build_test_role_configs` method)
- Modify: `src/handler.rs` or `src/tools/workspace/indexing/pipeline.rs` (call init at startup)
- Test: `src/search/language_config.rs` (test the builder)

**Step 1: Write failing test**

```rust
#[test]
fn test_build_test_role_configs_from_language_configs() {
    let configs = LanguageConfigs::load_embedded();
    let role_configs = configs.build_test_role_configs();

    let csharp = role_configs.get("csharp").expect("csharp role config");
    assert!(csharp.test_case.contains("fact"));
    assert!(csharp.fixture_setup.contains("setup"));
    assert!(!csharp.test_case.contains("setup"));

    let java = role_configs.get("java").expect("java role config");
    assert!(java.test_case.contains("test"));
    assert!(java.fixture_setup.contains("beforeeach"));
    assert!(java.fixture_teardown.contains("aftereach"));

    // Go has no test annotations, should have empty config
    let go = role_configs.get("go").expect("go role config");
    assert!(go.test_case.is_empty());
}
```

**Step 2: Implement build_test_role_configs**

Add to `LanguageConfigs` impl in `src/search/language_config.rs`:

```rust
pub fn build_test_role_configs(&self) -> HashMap<String, julie_extractors::TestRoleConfig> {
    self.configs
        .iter()
        .map(|(lang, config)| {
            let tc = &config.annotation_classes.test;
            let role_config = julie_extractors::TestRoleConfig {
                test_case: tc.test_case.iter().cloned().collect(),
                parameterized_test: tc.parameterized_test.iter().cloned().collect(),
                fixture_setup: tc.fixture_setup.iter().cloned().collect(),
                fixture_teardown: tc.fixture_teardown.iter().cloned().collect(),
                test_container: tc.test_container.iter().cloned().collect(),
            };
            (lang.clone(), role_config)
        })
        .collect()
}
```

**Step 3: Init at server startup**

Find where `LanguageConfigs::load_embedded()` is called in the server (likely `handler.rs` or during `JulieServerHandler` construction). After loading, add:

```rust
let role_configs = language_configs.build_test_role_configs();
julie_extractors::init_test_role_configs(role_configs);
```

Use `deep_dive(symbol="load_embedded")` to find the exact call site and add the init there.

**Step 4: Run tests, verify pass**

```bash
cargo nextest run --lib test_build_test_role 2>&1 | tail -10
```

**Step 5: Commit**

```bash
git add src/search/language_config.rs src/handler.rs
git commit -m "feat(config): build TestRoleConfig from TOML and init at startup

LanguageConfigs::build_test_role_configs() converts TOML role
taxonomy into the extractors crate's TestRoleConfig format.
Initialized via OnceLock at server startup."
```

---

## Session 2: Config-Driven Classification

### Task 4: Replace is_test_symbol with classify_test_role in all extractors

**Files:**
- Modify: 21 extractor files (see list below)
- Modify: `crates/julie-extractors/src/test_detection.rs` (deprecate, re-export compat shim)
- Test: existing tests in `crates/julie-extractors/src/tests/test_detection.rs`

**Extractor files to update** (each has `use crate::test_detection::is_test_symbol` and 1-3 call sites):

1. `crates/julie-extractors/src/bash/functions.rs:6,27`
2. `crates/julie-extractors/src/java/methods.rs:4,84,155`
3. `crates/julie-extractors/src/php/functions.rs:5,99`
4. `crates/julie-extractors/src/swift/callables.rs:2,81`
5. `crates/julie-extractors/src/lua/functions.rs:10,98`
6. `crates/julie-extractors/src/gdscript/functions.rs:4,109`
7. `crates/julie-extractors/src/vue/script.rs:11,95`
8. `crates/julie-extractors/src/javascript/functions.rs:7,70,149`
9. `crates/julie-extractors/src/python/functions.rs:6,77`
10. `crates/julie-extractors/src/rust/functions.rs:12,118`
11. `crates/julie-extractors/src/ruby/symbols.rs:6,161`
12. `crates/julie-extractors/src/scala/declarations.rs:7,90`
13. `crates/julie-extractors/src/go/functions.rs:2,61,164`
14. `crates/julie-extractors/src/csharp/members.rs:5,73,135,187`
15. `crates/julie-extractors/src/kotlin/declarations.rs:8,109,189`
16. `crates/julie-extractors/src/dart/functions.rs:11,99,200,298`
17. `crates/julie-extractors/src/qml/mod.rs:216`
18. `crates/julie-extractors/src/elixir/calls.rs:120`
19. `crates/julie-extractors/src/razor/csharp.rs:258`
20. `crates/julie-extractors/src/razor/stubs.rs:145`
21. `crates/julie-extractors/src/zig/functions.rs:40`

**The pattern for each file is identical.** Here's the transformation using `rust/functions.rs` as the example:

Before:
```rust
use crate::test_detection::is_test_symbol;
// ...
if is_test_symbol(
    "rust",
    &name,
    &base.file_path,
    &kind,
    &annotation_keys,
    None,
) {
    metadata.insert("is_test".to_string(), Value::Bool(true));
}
```

After:
```rust
use crate::test_roles::{classify_test_role, get_role_config};
// ...
if let Some(role) = classify_test_role(
    "rust",
    &name,
    &base.file_path,
    &kind,
    &annotation_keys,
    None,
    get_role_config("rust"),
) {
    metadata.insert("is_test".to_string(), Value::Bool(true));
    metadata.insert("test_role".to_string(), Value::String(role.as_str().to_string()));
}
```

Apply this transformation to all 21 files. The language string passed to `get_role_config` must match the language string passed as the first argument (already present in each call site).

**For files that pass `doc_comment`** (php/functions.rs), keep passing it:

```rust
if let Some(role) = classify_test_role(
    "php",
    &name,
    &base.file_path,
    &kind,
    &annotation_keys,
    doc_comment.as_deref(),
    get_role_config("php"),
) {
```

**Step: Add backward-compat shim to test_detection.rs**

Keep `test_detection.rs` but replace the body with a delegation:

```rust
use crate::base::SymbolKind;
use crate::test_roles::{classify_test_role, get_role_config};

pub fn is_test_symbol(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    annotation_keys: &[String],
    doc_comment: Option<&str>,
) -> bool {
    classify_test_role(
        language,
        name,
        file_path,
        kind,
        annotation_keys,
        doc_comment,
        get_role_config(language),
    )
    .is_some()
}
```

This preserves backward compatibility for any code that still calls `is_test_symbol` (server-side `is_test_symbol_for_embedding`, etc.) while delegating to the new classify_test_role.

**Step: Run full test suite for extractors**

```bash
cargo nextest run --lib tests::test_detection 2>&1 | tail -20
```

All existing tests should pass since `classify_test_role` preserves the same detection behavior.

**Step: Commit**

```bash
git add crates/julie-extractors/
git commit -m "refactor(extractors): replace is_test_symbol with classify_test_role

All 21 extractor files now call classify_test_role and store both
metadata.is_test (backward compat) and metadata.test_role (new).
is_test_symbol retained as thin shim delegating to classify_test_role.
Config-driven annotation classification for languages with TOML
configs, convention-based fallback for others."
```

---

### Task 5: Update server-side is_test consumers and verify pipeline

**Files:**
- Modify: `src/tools/impact/mod.rs:448` (add test_role awareness)
- Modify: `src/tools/impact/ranking.rs:106` (add test_role awareness)
- Modify: `src/tools/deep_dive/data.rs:322` (add test_role awareness)
- Modify: `src/tools/deep_dive/formatting.rs:186` (display test_role)
- Test: existing tests + new integration test

**What to build:** Server-side consumers currently read `metadata["is_test"]`. They continue to work unchanged (backward compat). But add `test_role` awareness where it matters:

- `deep_dive` formatting: show the role (e.g., "[test_case]" or "[fixture_setup]") instead of generic "[test]"
- `impact/ranking.rs`: separate test_case from fixture in impact sorting

**Step 1: Update deep_dive formatting**

In `src/tools/deep_dive/formatting.rs`, find `format_test_quality_info` (line 186). Update to include role:

```rust
fn format_test_quality_info(metadata: &serde_json::Value) -> Option<String> {
    let tier = metadata
        .get("test_quality")
        .and_then(|tq| tq.get("quality_tier"))
        .and_then(|v| v.as_str())?;
    let role = metadata
        .get("test_role")
        .and_then(|v| v.as_str())
        .unwrap_or("test");
    Some(format!("  [{}] [{}]", role, tier))
}
```

**Step 2: Verify pipeline end-to-end**

Write an integration test or use the fixture database to verify:
1. Index a file containing test functions and fixtures
2. Verify `metadata.test_role` is set correctly on extracted symbols
3. Verify `metadata.is_test` is still set for backward compat

**Step 3: Run `cargo xtask test changed`**

```bash
cargo xtask test changed 2>&1 | tail -20
```

**Step 4: Commit**

```bash
git add src/tools/
git commit -m "feat(tools): add test_role awareness to deep_dive and impact

deep_dive displays test role alongside quality tier.
Server-side is_test consumers continue working via backward-compat
metadata.is_test field."
```

---

## Session 3: Evidence-Based Quality Model

### Task 6: Add TestEvidenceConfig to language configs and TOML files

**Files:**
- Modify: `src/search/language_config.rs` (add TestEvidenceConfig struct)
- Modify: `languages/rust.toml`, `languages/python.toml`, `languages/java.toml`, `languages/csharp.toml`, `languages/kotlin.toml` (add `[test_evidence]` sections)

**Step 1: Write failing test**

```rust
#[test]
fn test_evidence_config_loads_from_toml() {
    let configs = LanguageConfigs::load_embedded();
    let rust = configs.get("rust").expect("rust config");
    assert!(!rust.test_evidence.assertion_identifiers.is_empty());
    assert!(rust.test_evidence.assertion_identifiers.contains(&"assert_eq".to_string()));
}
```

**Step 2: Add TestEvidenceConfig**

In `src/search/language_config.rs`, after `EarlyWarningConfig`:

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

**Step 3: Populate TOML files**

Add `[test_evidence]` sections per the design doc Part 7. Start with the 5 most-used languages. Before populating each language, verify identifier extraction:

```bash
# Verify Rust assertion identifiers are extracted
./target/debug/julie-server search "assert_eq" --target definitions --workspace . --standalone --json 2>/dev/null | head -5
```

The verification step is critical: if `assert_eq` doesn't appear as an identifier for Rust (it's a macro), document this gap and use regex fallback for Rust.

**Step 4: Commit**

```bash
git add src/search/language_config.rs languages/*.toml
git commit -m "feat(config): add test_evidence TOML sections for assertion identifiers

TestEvidenceConfig with assertion_identifiers, error_assertion_identifiers,
and mock_identifiers. Populated for Rust, Python, Java, C#, Kotlin.
Identifier extraction verified per language."
```

---

### Task 7: Rewrite test_quality.rs with evidence model

**Files:**
- Modify: `src/analysis/test_quality.rs` (replace regex oracle)
- Modify: `src/analysis/mod.rs` (update re-exports)
- Test: `src/tests/analysis/test_quality_tests.rs` (rewrite tests)

**Step 1: Write failing tests for the new model**

```rust
use crate::analysis::test_quality::{TestQualityAssessment, TestQualityTier, EvidenceSource};

#[test]
fn test_fixture_is_not_applicable() {
    let assessment = analyze_with_role(
        "fixture_setup",  // role
        "fn setUp() { db.reset(); }",  // body
        0,  // assertion_identifier_count
        EvidenceSource::None,
    );
    assert_eq!(assessment.tier, TestQualityTier::NotApplicable);
    assert_eq!(assessment.confidence, 1.0);
}

#[test]
fn test_empty_body_is_stub() {
    let assessment = analyze_with_role(
        "test_case",
        "",
        0,
        EvidenceSource::None,
    );
    assert_eq!(assessment.tier, TestQualityTier::Stub);
    assert_eq!(assessment.confidence, 1.0);
}

#[test]
fn test_identifier_based_thorough() {
    let assessment = analyze_with_role(
        "test_case",
        "fn test_foo() { assert_eq!(a, b); assert_ne!(c, d); assert!(e); should_err(); }",
        3,  // 3 assertion identifiers found
        EvidenceSource::Identifier,
    );
    assert_eq!(assessment.tier, TestQualityTier::Thorough);
    assert!(assessment.confidence >= 0.85);
}

#[test]
fn test_regex_zero_assertions_is_unknown_not_stub() {
    let assessment = analyze_with_role(
        "test_case",
        "fn test_foo() { let x = compute(); println!(\"{}\", x); }",
        0,
        EvidenceSource::Regex,
    );
    assert_eq!(assessment.tier, TestQualityTier::Unknown);
}
```

**Step 2: Implement the new evidence model**

Replace the core of `src/analysis/test_quality.rs`. Keep `strip_comments_and_strings` (it's well-implemented and needed for regex fallback). Replace `analyze_test_body` and `classify_tier`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestQualityAssessment {
    pub tier: TestQualityTier,
    pub confidence: f32,
    pub evidence: QualityEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TestQualityTier {
    Thorough,
    Adequate,
    Thin,
    Stub,
    Unknown,
    NotApplicable,
}

impl TestQualityTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Thorough => "thorough",
            Self::Adequate => "adequate",
            Self::Thin => "thin",
            Self::Stub => "stub",
            Self::Unknown => "unknown",
            Self::NotApplicable => "n/a",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityEvidence {
    pub assertion_count: u32,
    pub assertion_source: EvidenceSource,
    pub has_error_testing: bool,
    pub mock_count: u32,
    pub body_lines: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    Identifier,
    Regex,
    None,
}

pub fn assess_test_quality(
    test_role: Option<&str>,
    body: Option<&str>,
    identifier_assertion_count: u32,
    identifier_error_count: u32,
    identifier_mock_count: u32,
    has_identifier_evidence: bool,
) -> TestQualityAssessment {
    // Fixtures and containers are not scored
    if matches!(test_role, Some("fixture_setup") | Some("fixture_teardown") | Some("test_container")) {
        return TestQualityAssessment {
            tier: TestQualityTier::NotApplicable,
            confidence: 1.0,
            evidence: QualityEvidence {
                assertion_count: 0,
                assertion_source: EvidenceSource::None,
                has_error_testing: false,
                mock_count: 0,
                body_lines: 0,
            },
        };
    }

    // No body or placeholder body
    let body = match body {
        Some(b) if !is_placeholder_body(b) => b,
        _ => {
            return TestQualityAssessment {
                tier: TestQualityTier::Stub,
                confidence: 1.0,
                evidence: QualityEvidence {
                    assertion_count: 0,
                    assertion_source: EvidenceSource::None,
                    has_error_testing: false,
                    mock_count: 0,
                    body_lines: 0,
                },
            };
        }
    };

    let body_lines = body.lines().count() as u32;

    // Prefer identifier evidence over regex
    if has_identifier_evidence {
        let has_error = identifier_error_count > 0;
        let tier = classify_from_counts(identifier_assertion_count, has_error);
        let confidence = match tier {
            TestQualityTier::Thorough => 0.9,
            TestQualityTier::Adequate => 0.85,
            TestQualityTier::Thin => 0.8,
            TestQualityTier::Stub => 0.85,
            _ => 0.5,
        };
        return TestQualityAssessment {
            tier,
            confidence,
            evidence: QualityEvidence {
                assertion_count: identifier_assertion_count,
                assertion_source: EvidenceSource::Identifier,
                has_error_testing: has_error,
                mock_count: identifier_mock_count,
                body_lines,
            },
        };
    }

    // Regex fallback
    let stripped = strip_comments_and_strings(body);
    let regex_assertions = count_pattern_matches(&stripped, assertion_patterns());
    let regex_errors = count_pattern_matches(&stripped, error_testing_patterns()) > 0;
    let regex_mocks = count_pattern_matches(&stripped, mock_patterns());

    if regex_assertions == 0 {
        // Regex found nothing; might be a custom assertion library we don't know about
        return TestQualityAssessment {
            tier: TestQualityTier::Unknown,
            confidence: 0.3,
            evidence: QualityEvidence {
                assertion_count: 0,
                assertion_source: EvidenceSource::Regex,
                has_error_testing: false,
                mock_count: regex_mocks as u32,
                body_lines,
            },
        };
    }

    let tier = classify_from_counts(regex_assertions as u32, regex_errors);
    let confidence = match tier {
        TestQualityTier::Thorough => 0.5,
        TestQualityTier::Adequate => 0.45,
        TestQualityTier::Thin => 0.4,
        _ => 0.3,
    };
    TestQualityAssessment {
        tier,
        confidence,
        evidence: QualityEvidence {
            assertion_count: regex_assertions as u32,
            assertion_source: EvidenceSource::Regex,
            has_error_testing: regex_errors,
            mock_count: regex_mocks as u32,
            body_lines,
        },
    }
}

fn classify_from_counts(assertion_count: u32, has_error_testing: bool) -> TestQualityTier {
    if assertion_count >= 3 && has_error_testing {
        TestQualityTier::Thorough
    } else if assertion_count >= 2 {
        TestQualityTier::Adequate
    } else if assertion_count >= 1 {
        TestQualityTier::Thin
    } else {
        TestQualityTier::Stub
    }
}

fn is_placeholder_body(body: &str) -> bool {
    let trimmed = body.trim();
    trimmed.is_empty()
        || trimmed == "pass"
        || trimmed == "..."
        || trimmed.starts_with("todo!(")
        || trimmed.starts_with("unimplemented!(")
        || trimmed.starts_with("// TODO")
        || trimmed.starts_with("# TODO")
}
```

**Step 3: Update compute_test_quality_metrics**

The pipeline function `compute_test_quality_metrics` needs to:
1. Read `test_role` from symbol metadata (not just `is_test`)
2. Query identifier counts from the database for each test
3. Call `assess_test_quality` with identifier evidence
4. Store the new assessment format in metadata

```rust
pub fn compute_test_quality_metrics(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
) -> Result<TestQualityStats> {
    let mut stats = TestQualityStats::default();

    let mut stmt = db.conn.prepare(
        "SELECT id, code_context, metadata, language FROM symbols
         WHERE json_extract(metadata, '$.is_test') = 1",
    )?;

    let rows: Vec<(String, Option<String>, Option<String>, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, code_context, metadata_str, language) in &rows {
            let test_role = metadata_str
                .as_ref()
                .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                .and_then(|v| v.get("test_role")?.as_str().map(String::from));

            // Query identifier evidence
            let evidence_config = language_configs
                .get(language)
                .map(|c| &c.test_evidence);

            let (id_assertions, id_errors, id_mocks, has_id_evidence) =
                if let Some(config) = evidence_config {
                    query_identifier_evidence(db, id, config)?
                } else {
                    (0, 0, 0, false)
                };

            let assessment = assess_test_quality(
                test_role.as_deref(),
                code_context.as_deref(),
                id_assertions,
                id_errors,
                id_mocks,
                has_id_evidence,
            );

            // Update stats
            stats.total_tests += 1;
            match assessment.tier {
                TestQualityTier::Thorough => stats.thorough += 1,
                TestQualityTier::Adequate => stats.adequate += 1,
                TestQualityTier::Thin => stats.thin += 1,
                TestQualityTier::Stub => stats.stub += 1,
                TestQualityTier::Unknown | TestQualityTier::NotApplicable => {}
            }

            // Merge assessment into metadata
            let quality_json = serde_json::json!({
                "quality_tier": assessment.tier.as_str(),
                "confidence": assessment.confidence,
                "assertion_count": assessment.evidence.assertion_count,
                "assertion_source": format!("{:?}", assessment.evidence.assertion_source).to_lowercase(),
                "has_error_testing": assessment.evidence.has_error_testing,
                "mock_count": assessment.evidence.mock_count,
                "body_lines": assessment.evidence.body_lines,
            });

            db.conn.execute(
                "UPDATE symbols SET metadata = json_set(
                    COALESCE(metadata, '{}'),
                    '$.test_quality', json(?1)
                ) WHERE id = ?2",
                rusqlite::params![quality_json.to_string(), id],
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => db.conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    Ok(stats)
}

fn query_identifier_evidence(
    db: &SymbolDatabase,
    symbol_id: &str,
    config: &TestEvidenceConfig,
) -> Result<(u32, u32, u32, bool)> {
    if config.assertion_identifiers.is_empty() {
        return Ok((0, 0, 0, false));
    }

    let names: Vec<String> = db
        .conn
        .prepare("SELECT LOWER(name) FROM identifiers WHERE containing_symbol_id = ?1")?
        .query_map([symbol_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    if names.is_empty() {
        return Ok((0, 0, 0, false));
    }

    let assertion_set: std::collections::HashSet<&str> =
        config.assertion_identifiers.iter().map(|s| s.as_str()).collect();
    let error_set: std::collections::HashSet<&str> =
        config.error_assertion_identifiers.iter().map(|s| s.as_str()).collect();
    let mock_set: std::collections::HashSet<&str> =
        config.mock_identifiers.iter().map(|s| s.as_str()).collect();

    let assertions = names.iter().filter(|n| assertion_set.contains(n.as_str())).count() as u32;
    let errors = names.iter().filter(|n| error_set.contains(n.as_str())).count() as u32;
    let mocks = names.iter().filter(|n| mock_set.contains(n.as_str())).count() as u32;

    Ok((assertions, errors, mocks, true))
}
```

**Step 4: Update analyze_batch to pass language_configs**

In `src/tools/workspace/indexing/pipeline.rs:627`, `compute_test_quality_metrics` now takes `language_configs`:

```rust
let language_configs = handler.language_configs();
if let Err(e) = crate::analysis::compute_test_quality_metrics(&db_lock, &language_configs) {
    warn!("Failed to compute test quality metrics: {}", e);
}
```

Find how `handler` exposes language_configs (use `deep_dive(symbol="language_configs")`) and wire it through.

**Step 5: Run tests**

```bash
cargo nextest run --lib test_fixture_is_not 2>&1 | tail -5
cargo nextest run --lib test_empty_body_is 2>&1 | tail -5
cargo nextest run --lib test_identifier_based 2>&1 | tail -5
cargo nextest run --lib test_regex_zero 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src/analysis/test_quality.rs src/analysis/mod.rs src/tools/workspace/indexing/pipeline.rs src/tests/analysis/test_quality_tests.rs
git commit -m "feat(analysis): replace regex quality oracle with evidence model

test_quality now uses identifier-based assertion counting against
framework-specific config, with regex as lower-confidence fallback.
Fixtures return NotApplicable instead of false Stub. Unknown tier
replaces false Stub when regex finds zero assertions. All tiers
carry confidence scores."
```

---

## Session 4: Risk Gating & Signal Surfaces

### Task 8: Confidence-gated change_risk

**Files:**
- Modify: `src/analysis/change_risk.rs:72-81` (update test_weakness_score)
- Modify: `src/analysis/change_risk.rs:115-238` (read confidence from metadata)
- Test: `src/tests/analysis/change_risk_tests.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_weakness_score_with_high_confidence() {
    let score = test_weakness_score(Some("thorough"), 0.9);
    assert!((score - 0.1).abs() < 0.05); // Near the raw 0.1 for thorough
}

#[test]
fn test_weakness_score_with_low_confidence_converges_to_neutral() {
    let score = test_weakness_score(Some("stub"), 0.0);
    assert!((score - 0.5).abs() < 0.01); // Should be neutral
}

#[test]
fn test_weakness_score_unknown_tier() {
    let score = test_weakness_score(Some("unknown"), 0.3);
    // raw_weakness for unknown = 0.5, confidence 0.3
    // result = 0.5 + (0.5 - 0.5) * 0.3 = 0.5
    assert!((score - 0.5).abs() < 0.01);
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
    let neutral = 0.5;
    neutral + (raw_weakness - neutral) * confidence
}
```

**Step 3: Update compute_change_risk_scores to read confidence**

In the query that reads `test_linkage` metadata (around line 180), also read `test_quality.confidence`:

```rust
let confidence: f64 = meta
    .get("test_linkage")
    .and_then(|tl| {
        // Get linked test quality confidence
        // For now, use 1.0 if tier is known, 0.5 for unknown
        let tier = tl.get("best_tier")?.as_str()?;
        match tier {
            "thorough" | "adequate" | "thin" | "stub" => Some(0.8),
            "unknown" => Some(0.3),
            _ => Some(0.5),
        }
    })
    .unwrap_or(1.0); // No linkage at all: full weight on "untested"

let test_weak = test_weakness_score(best_tier, confidence);
```

**Step 4: Run tests**

```bash
cargo nextest run --lib test_weakness_score 2>&1 | tail -10
cargo nextest run --lib test_compute_change_risk 2>&1 | tail -10
```

**Step 5: Commit**

```bash
git add src/analysis/change_risk.rs src/tests/analysis/change_risk_tests.rs
git commit -m "feat(analysis): confidence-gate test weakness in change_risk

test_weakness_score takes confidence parameter. Low-confidence tiers
converge toward neutral (0.5), preventing unreliable quality
assessments from dominating risk scores."
```

---

### Task 9: Add scheduler signal to early_warnings

**Files:**
- Modify: `src/analysis/early_warnings.rs` (add SchedulerSignal section)
- Test: `src/tests/analysis/` (add scheduler signal tests)

**Step 1: Write failing test**

```rust
#[test]
fn test_scheduler_signal_from_annotation() {
    // Build a symbol with a scheduler annotation
    let symbol = test_symbol("process_daily", "java", "src/jobs/DailyJob.java",
        vec![annotation("scheduled", "Scheduled")]);
    // ... setup db, insert, generate report
    assert!(!report.scheduler_signals.is_empty());
    assert_eq!(report.scheduler_signals[0].symbol_name, "process_daily");
}
```

**Step 2: Add SchedulerSignal to report structs**

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

**Step 3: Add scheduler to AnnotationSets and build_report**

In `annotation_sets()`:

```rust
let scheduler: HashSet<String> = config
    .annotation_classes
    .scheduler
    .iter()
    .cloned()
    .collect();
```

In `build_report()`, add scheduler detection alongside entrypoint/auth/review:

```rust
if sets.scheduler.contains(&annotation.annotation_key) {
    scheduler_signals.push(scheduler_signal(symbol, annotation));
}
```

**Step 4: Commit**

---

### Task 10: Add untested entry point and high-centrality untested signals

**Files:**
- Modify: `src/analysis/early_warnings.rs`
- Test: `src/tests/analysis/`

**Step 1: Add UntestedEntryPointSignal**

Cross-reference entry points with test_linkage metadata. For each entry point symbol, check if `metadata.test_linkage` exists:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UntestedEntryPointSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
}
```

In `build_report`, after collecting entry_points:

```rust
let mut untested_entry_points = Vec::new();
for ep in &entry_points {
    if let Some(symbol) = symbol_map.get(&ep.symbol_id) {
        let has_test_linkage = symbol.metadata
            .as_ref()
            .and_then(|m| m.get("test_linkage"))
            .is_some();
        if !has_test_linkage {
            untested_entry_points.push(UntestedEntryPointSignal {
                symbol_id: ep.symbol_id.clone(),
                symbol_name: ep.symbol_name.clone(),
                symbol_kind: ep.symbol_kind.clone(),
                language: ep.language.clone(),
                file_path: ep.file_path.clone(),
                start_line: ep.start_line,
                annotation: ep.annotation.clone(),
            });
        }
    }
}
```

**Step 2: Add HighCentralityUntestedSignal**

Query top-N centrality symbols with no test linkage:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighCentralityUntestedSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub reference_score: f64,
}
```

Query from the database (not the in-memory symbol list, since we need reference_score):

```rust
fn collect_high_centrality_untested(
    db: &SymbolDatabase,
    file_pattern: Option<&str>,
    limit: usize,
) -> Result<Vec<HighCentralityUntestedSignal>> {
    let mut stmt = db.conn.prepare(
        "SELECT id, name, kind, language, file_path, start_line, reference_score
         FROM symbols
         WHERE reference_score > 0
           AND (json_extract(metadata, '$.is_test') IS NULL
                OR json_extract(metadata, '$.is_test') != 1)
           AND json_extract(metadata, '$.test_linkage') IS NULL
         ORDER BY reference_score DESC
         LIMIT ?1",
    )?;
    // ... map rows to HighCentralityUntestedSignal
}
```

**Step 3: Update report structs and CLI output**

Add both new signal vecs to `EarlyWarningReport` and `ReportSummary`. Update the CLI `signals` command formatter to render the new sections.

**Step 4: Signal framing**

All output must use honest language:
- "No test linkage observed" not "untested"
- "No auth marker found in owner chain" not "unauthenticated"
- "Executes on a schedule without request context" not "insecure"

**Step 5: Run full test tier**

```bash
cargo xtask test dev 2>&1 | tail -20
```

**Step 6: Commit**

```bash
git add src/analysis/early_warnings.rs src/analysis/mod.rs src/tests/analysis/ src/cli_tools/
git commit -m "feat(signals): add scheduler, untested entry point, and high-centrality signals

Three new signal sections in the early warning report:
- SchedulerSignal: symbols with scheduler annotations
- UntestedEntryPointSignal: API endpoints with no observed test linkage
- HighCentralityUntestedSignal: high-centrality symbols with no test coverage

All framing uses 'observed'/'no linkage found' language."
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
   - Entry point test coverage gaps appear
   - No false "stub" tiers on fixture functions
6. Run `julie-server signals --workspace . --standalone --format markdown` and verify honest framing language

---

## Task Dependency Graph

```
Task 1 (types) ──→ Task 2 (TOML) ──→ Task 3 (init)
                                           │
                                           ▼
                    Task 4 (classify) ──→ Task 5 (server consumers)
                         │
                         ▼
Task 6 (evidence config) ──→ Task 7 (quality model)
                                      │
                                      ▼
                              Task 8 (risk gating)
                                      │
                                      ▼
                    Task 9 (scheduler) ──→ Task 10 (coverage signals)
```

Tasks 1-3 are serial (each depends on previous). Tasks 4-5 depend on 1-3. Tasks 6-7 can start after Task 3 (don't need Task 4-5). Task 8 needs Task 7. Tasks 9-10 need Task 8.

Within each session, tasks are serial. Sessions 1-2 are serial. Session 3 depends on Session 1 but can overlap with Session 2 if desired. Session 4 depends on Session 3.
