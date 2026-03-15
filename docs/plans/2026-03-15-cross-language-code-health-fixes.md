# Cross-Language Code Health Intelligence Fixes

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 6 issues discovered during dogfooding Code Health Intelligence against a C# reference workspace (LabHandbookV2), making test coverage, change risk, and security risk metadata accurate for class-based, DI-heavy, ORM-heavy languages.

**Architecture:** Fixes are localized to the analysis pipeline (`src/analysis/`) and test detection (`crates/julie-extractors/src/test_detection.rs`). No schema changes. Each fix is independently testable via TDD against existing test infrastructure patterns (TempDir + SymbolDatabase + manual SQL inserts).

**Tech Stack:** Rust, SQLite (rusqlite), tree-sitter extractors

**Reference workspace for dogfooding:** `~/source/LabHandbookV2` (C#/.NET + TypeScript/Vue, workspace ID `labhandbookv2_67e8c1cf`)

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `src/analysis/security_risk.rs` | Security risk scoring | Modify: expand sink patterns, tighten input handling |
| `src/tests/analysis/security_risk_tests.rs` | Security risk tests | Modify: add tests for new patterns |
| `crates/julie-extractors/src/test_detection.rs` | Test symbol detection | Modify: add lifecycle method attrs |
| `crates/julie-extractors/src/tests/test_detection.rs` | Test detection tests | Modify: add lifecycle tests |
| `src/analysis/test_coverage.rs` | Test-to-code linkage | Modify: add class aggregation, improve disambiguation |
| `src/tests/analysis/test_coverage_tests.rs` | Test coverage tests | Modify: add class aggregation + disambiguation tests |
| `src/database/relationships.rs` | Reference score computation | Modify: add constructor→class centrality propagation |
| `src/tests/core/database.rs` | Database tests | Modify: add constructor propagation test |

---

## Chunk 1: Security Risk Improvements

### Task 1: Expand ORM Sink Patterns

**Files:**
- Modify: `src/analysis/security_risk.rs:44-52` (DATABASE_SINKS constant)
- Test: `src/tests/analysis/security_risk_tests.rs`

- [ ] **Step 1: Write failing test for EF Core sink detection**

```rust
#[test]
fn test_sink_match_efcore_savechanges() {
    let callees = vec!["SaveChangesAsync".to_string()];
    let patterns: Vec<&str> = EXECUTION_SINKS.iter().chain(DATABASE_SINKS.iter()).copied().collect();
    let (score, matched) = compute_sink_signal(&callees, &[], &patterns);
    assert!(score > 0.0, "SaveChangesAsync should match a sink pattern");
    assert!(matched.contains(&"savechanges".to_string()) || matched.contains(&"savechangesasync".to_string()));
}

#[test]
fn test_sink_match_django_orm() {
    let callees = vec!["objects.filter".to_string()];
    let patterns: Vec<&str> = EXECUTION_SINKS.iter().chain(DATABASE_SINKS.iter()).copied().collect();
    let (score, matched) = compute_sink_signal(&callees, &[], &patterns);
    assert!(score > 0.0, "Django filter should match a sink pattern");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_sink_match_efcore_savechanges 2>&1 | tail -10`
Expected: FAIL — "SaveChangesAsync should match a sink pattern"

- [ ] **Step 3: Add ORM patterns to DATABASE_SINKS**

In `src/analysis/security_risk.rs`, replace the `DATABASE_SINKS` constant:

```rust
const DATABASE_SINKS: &[&str] = &[
    // Raw SQL execution
    "execute", "raw_sql", "exec_query", "executequery",
    "executeupdate", "rawquery", "runsql",
    // EF Core / .NET
    "savechanges", "executedelete", "executeupdate", "executesqlraw",
    "executesqlinterpolated", "fromsqlraw", "fromsql",
    // Django / SQLAlchemy / Python ORMs
    "filter", "raw", "commit", "cursor",
    // Rails / ActiveRecord
    "destroy", "find_by_sql", "update_all", "delete_all",
    // Prisma / TypeORM / JS ORMs
    "findmany", "findunique", "createmany", "deletemany",
    "getrepository", "createquerybuilder",
    // JPA / Hibernate
    "persist", "merge", "createquery", "createnativequery",
];
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_sink_match_efcore 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/analysis/security_risk.rs src/tests/analysis/security_risk_tests.rs
git commit -m "feat(security_risk): expand sink patterns for ORM frameworks

Add database sink patterns for EF Core, Django, Rails, Prisma, and
JPA/Hibernate. Previously only raw SQL patterns (execute, raw_sql) were
detected, missing LINQ/ORM-based database access entirely."
```

---

### Task 2: Tighten Input Handling False Positives

**Files:**
- Modify: `src/analysis/security_risk.rs:113-123` (has_input_handling function)
- Test: `src/tests/analysis/security_risk_tests.rs`

The current `INPUT_PATTERNS` matches `"Request"` which catches `RequestDelegate` (ASP.NET middleware DI type), and `"String"` which catches `ILogger<String>`. We need an exclusion list for known DI/framework types.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_input_handling_request_delegate_excluded() {
    // RequestDelegate is a DI framework type, not user input
    assert!(!has_input_handling(Some("(RequestDelegate next, ILogger<RoleClaimsMiddleware> logger)")));
}

#[test]
fn test_input_handling_real_http_request_still_matches() {
    // HttpRequest IS user input
    assert!(has_input_handling(Some("(HttpRequest req, string id)")));
}

#[test]
fn test_input_handling_ilogger_excluded() {
    // ILogger is infrastructure, not user input
    assert!(!has_input_handling(Some("(ILogger<Foo> logger, IOptions<Bar> opts)")));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_input_handling_request_delegate 2>&1 | tail -10`
Expected: FAIL — `has_input_handling` returns true for `RequestDelegate`

- [ ] **Step 3: Add DI exclusion patterns**

In `src/analysis/security_risk.rs`, add an exclusion list and modify `has_input_handling`:

```rust
/// DI / framework parameter types that match INPUT_PATTERNS but aren't user input.
/// These are checked AFTER a positive match to avoid false positives.
const DI_EXCLUSION_PATTERNS: &[&str] = &[
    "RequestDelegate",   // ASP.NET middleware pipeline delegate
    "ILogger",           // Logging infrastructure
    "IOptions",          // Configuration
    "IConfiguration",    // Configuration
    "IServiceProvider",  // DI container
    "IHostEnvironment",  // Hosting
    "IWebHostEnvironment",
    "IMemoryCache",      // Caching
    "CancellationToken", // Async cancellation
];

pub fn has_input_handling(signature: Option<&str>) -> bool {
    let sig = match signature {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };

    let param_portion = extract_parameter_portion(sig);

    // Check if any input pattern matches
    let has_match = INPUT_PATTERNS.iter().any(|pattern| param_portion.contains(pattern));
    if !has_match {
        return false;
    }

    // Check if ALL matches are explained by DI exclusion patterns.
    // If removing excluded types leaves no input pattern matches, return false.
    let mut remaining = param_portion.to_string();
    for excl in DI_EXCLUSION_PATTERNS {
        remaining = remaining.replace(excl, "");
    }
    INPUT_PATTERNS.iter().any(|pattern| remaining.contains(pattern))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_input_handling 2>&1 | tail -10`
Expected: All input_handling tests PASS (old and new)

- [ ] **Step 5: Commit**

```bash
git add src/analysis/security_risk.rs src/tests/analysis/security_risk_tests.rs
git commit -m "fix(security_risk): exclude DI framework types from input_handling

RequestDelegate, ILogger, IOptions etc. were falsely triggering
input_handling detection because they contain substrings matching
INPUT_PATTERNS (e.g. 'Request' in RequestDelegate). Add DI_EXCLUSION_PATTERNS
that suppress false positives while preserving real matches like HttpRequest."
```

---

## Chunk 2: Test Detection & Coverage Improvements

### Task 3: Recognize Test Lifecycle Methods

**Files:**
- Modify: `crates/julie-extractors/src/test_detection.rs:117-126` (detect_csharp) and other language detectors
- Test: `crates/julie-extractors/src/tests/test_detection.rs`

Test lifecycle methods (`[SetUp]`, `[TearDown]`, `@Before`, `@After`, `beforeEach`, etc.) establish the SUT and create relationships to production code. They should be marked `is_test = true` so their relationships count toward test coverage.

- [ ] **Step 1: Write failing tests for C# lifecycle detection**

In `crates/julie-extractors/src/tests/test_detection.rs`, add:

```rust
#[test]
fn csharp_setup_is_test() {
    assert!(check(
        "csharp", "SetUp", "Tests/MyTests.cs", &SymbolKind::Method,
        &[], &["SetUp".to_string()], None
    ));
}

#[test]
fn csharp_teardown_is_test() {
    assert!(check(
        "csharp", "TearDown", "Tests/MyTests.cs", &SymbolKind::Method,
        &[], &["TearDown".to_string()], None
    ));
}

#[test]
fn csharp_onetime_setup_is_test() {
    assert!(check(
        "csharp", "Initialize", "Tests/MyTests.cs", &SymbolKind::Method,
        &[], &["OneTimeSetUp".to_string()], None
    ));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p julie-extractors --lib csharp_setup_is_test 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Expand test attribute lists for each language**

In `crates/julie-extractors/src/test_detection.rs`:

**C# (detect_csharp):**
```rust
fn detect_csharp(attributes: &[String]) -> bool {
    let test_attrs = [
        // Test methods
        "Test", "TestMethod", "Fact", "Theory",
        // Lifecycle methods (NUnit)
        "SetUp", "TearDown", "OneTimeSetUp", "OneTimeTearDown",
        // Lifecycle methods (xUnit) — xUnit uses constructor/Dispose, no attrs needed
        // Lifecycle methods (MSTest)
        "TestInitialize", "TestCleanup", "ClassInitialize", "ClassCleanup",
    ];
    attributes.iter().any(|a| {
        let stripped = a.strip_prefix('[').unwrap_or(a);
        let stripped = stripped.strip_suffix(']').unwrap_or(stripped);
        test_attrs.contains(&stripped)
    })
}
```

**Java/Kotlin (detect_java_kotlin):**
```rust
fn detect_java_kotlin(decorators: &[String], attributes: &[String]) -> bool {
    let test_annotations = [
        // Test methods
        "Test", "ParameterizedTest", "RepeatedTest",
        // Lifecycle methods (JUnit 5)
        "BeforeEach", "AfterEach", "BeforeAll", "AfterAll",
        // Lifecycle methods (JUnit 4)
        "Before", "After", "BeforeClass", "AfterClass",
    ];
    decorators
        .iter()
        .chain(attributes.iter())
        .any(|a| test_annotations.contains(&a.as_str()))
}
```

**Python (detect_python) — add setUp recognition:**
```rust
fn detect_python(name: &str, decorators: &[String]) -> bool {
    if decorators
        .iter()
        .any(|d| d.starts_with("pytest") || d.starts_with("unittest"))
    {
        return true;
    }
    // unittest lifecycle methods
    if matches!(name, "setUp" | "tearDown" | "setUpClass" | "tearDownClass") {
        return true;
    }
    name.starts_with("test_")
}
```

**Swift (detect_swift):**
```rust
fn detect_swift(name: &str) -> bool {
    // XCTest: test* prefix + lifecycle methods
    name.starts_with("test") || matches!(name, "setUp" | "tearDown" | "setUpWithError" | "tearDownWithError")
}
```

- [ ] **Step 4: Add failing tests for Java/Python/Swift lifecycle, then verify all pass**

Run: `cargo test -p julie-extractors --lib setup_is_test 2>&1 | tail -10`
Run: `cargo test -p julie-extractors --lib teardown_is_test 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/test_detection.rs crates/julie-extractors/src/tests/test_detection.rs
git commit -m "feat(test_detection): recognize lifecycle methods as test context

Mark SetUp/TearDown (NUnit), BeforeEach/AfterEach (JUnit),
setUp/tearDown (Python unittest, XCTest), TestInitialize/TestCleanup
(MSTest) as is_test so their relationships count toward test coverage.
These methods establish the SUT and their production code references
are test-driven."
```

---

### Task 4: Improve Name-Match Disambiguation

**Files:**
- Modify: `src/analysis/test_coverage.rs:108-170` (Step 2b name-match fallback)
- Test: `src/tests/analysis/test_coverage_tests.rs`

Currently disambiguation uses only `common_directory_depth` which ties when all services are in the same directory. We need a secondary signal: **test class name similarity**.

When `LabTestServiceTests.ListAsync_ReturnsPagedResults` calls `ListAsync`, prefer `LabTestService.ListAsync` over `MediaService.ListAsync` because the test class name contains the production class name.

- [ ] **Step 1: Write failing test for class-name disambiguation**

In `src/tests/analysis/test_coverage_tests.rs`:

```rust
#[test]
fn test_name_match_prefers_class_name_similarity() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    insert_file(&db, "src/Services/LabTestService.cs");
    insert_file(&db, "src/Services/MediaService.cs");
    insert_file(&db, "tests/Services/LabTestServiceTests.cs");

    db.conn.execute_batch(r#"
        -- Two production symbols with same method name in same directory
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
        VALUES ('prod_labtest', 'ListAsync', 'method', 'csharp', 'src/Services/LabTestService.cs', 20, 0, 50, 0, 0, 0, NULL, 3.0, 'public');

        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
        VALUES ('prod_media', 'ListAsync', 'method', 'csharp', 'src/Services/MediaService.cs', 20, 0, 50, 0, 0, 0, NULL, 3.0, 'public');

        -- Test method that calls ListAsync from LabTestServiceTests
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
        VALUES ('test_1', 'ListAsync_ReturnsResults', 'method', 'csharp', 'tests/Services/LabTestServiceTests.cs', 30, 0, 45, 0, 0, 0,
                '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0, 'private');

        -- Identifier: test calls ListAsync (no target_symbol_id — needs name-match)
        INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
        VALUES ('ident_1', 'ListAsync', 'call', 'csharp', 'tests/Services/LabTestServiceTests.cs', 41, 0, 41, 20, 'test_1', NULL);
    "#).unwrap();

    let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
    assert_eq!(stats.symbols_covered, 1, "Should cover exactly one symbol");

    // Verify the CORRECT symbol was linked (LabTestService, not MediaService)
    let cov: Option<String> = db.conn.query_row(
        "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'prod_labtest'",
        [], |row| row.get(0)
    ).unwrap();
    assert!(cov.is_some(), "LabTestService.ListAsync should have test coverage");

    let no_cov: Option<String> = db.conn.query_row(
        "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'prod_media'",
        [], |row| row.get(0)
    ).unwrap();
    assert!(no_cov.is_none(), "MediaService.ListAsync should NOT have test coverage from LabTestServiceTests");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_name_match_prefers_class_name 2>&1 | tail -10`
Expected: FAIL — both symbols get coverage or the wrong one does

- [ ] **Step 3: Add class-name similarity to disambiguation**

In `src/analysis/test_coverage.rs`, modify the Step 2b disambiguation logic. After grouping by `(test_id, ident_name)`, score candidates by both directory proximity AND test file name → production file name similarity:

```rust
// For each group, pick the one with best combined score:
// directory proximity + file name similarity bonus
for ((_test_id, _ident_name), candidates) in name_matches {
    let best = candidates.into_iter()
        .max_by_key(|(_, prod_path, _, _, test_path)| {
            let dir_score = common_directory_depth(test_path, prod_path) * 10;

            // File name similarity: if test file name contains production file stem,
            // add a large bonus. E.g., "LabTestServiceTests.cs" contains "LabTestService"
            let test_file_stem = test_path.rsplit('/').next().unwrap_or("")
                .split('.').next().unwrap_or("");
            let prod_file_stem = prod_path.rsplit('/').next().unwrap_or("")
                .split('.').next().unwrap_or("");
            let name_bonus = if !prod_file_stem.is_empty()
                && test_file_stem.contains(prod_file_stem) {
                100  // Strong signal: test file name contains production file name
            } else {
                0
            };

            dir_score + name_bonus
        });
    if let Some((prod_id, _, test_name, tier, _)) = best {
        linkages.entry(prod_id).or_default().insert((_test_id, test_name, tier));
    }
}
```

Note: The `_test_id` variable needs to be captured properly — the `for` loop destructures `((test_id, ident_name), candidates)`, just replace the existing loop body.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_name_match_prefers_class_name 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run existing coverage tests to check for regressions**

Run: `cargo test --lib tests::analysis::test_coverage 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/analysis/test_coverage.rs src/tests/analysis/test_coverage_tests.rs
git commit -m "fix(test_coverage): disambiguate name-match by test file name similarity

When multiple production symbols share a name (e.g., ListAsync in
LabTestService, MediaService, PageService), prefer the one whose file
name matches the test file name. LabTestServiceTests → LabTestService
scores higher than → MediaService. Previously all candidates tied on
directory proximity when in the same parent directory."
```

---

### Task 5: Aggregate Method Coverage to Parent Class

**Files:**
- Modify: `src/analysis/test_coverage.rs` (add Step 3 after existing logic)
- Test: `src/tests/analysis/test_coverage_tests.rs`

After computing method-level coverage, roll up to parent classes. If a class's methods have test coverage, the class itself should also show coverage.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_class_inherits_method_coverage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    insert_file(&db, "src/services.rs");
    insert_file(&db, "tests/services_test.rs");

    db.conn.execute_batch(r#"
        -- Production class with a method
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
        VALUES ('class_1', 'PaymentService', 'class', 'csharp', 'src/services.rs', 1, 0, 50, 0, 0, 0, NULL, 5.0, 'public');

        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility, parent_symbol_id)
        VALUES ('method_1', 'ProcessPayment', 'method', 'csharp', 'src/services.rs', 10, 0, 30, 0, 0, 0, NULL, 3.0, 'public', 'class_1');

        -- Test that calls the method
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
        VALUES ('test_1', 'test_process_payment', 'method', 'csharp', 'tests/services_test.rs', 5, 0, 20, 0, 0, 0,
                '{"is_test": true, "test_quality": {"quality_tier": "thorough"}}', 0.0, 'private');

        INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
        VALUES ('rel_1', 'test_1', 'method_1', 'calls', 'tests/services_test.rs', 10);
    "#).unwrap();

    let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

    // Method should have coverage
    let method_cov: Option<String> = db.conn.query_row(
        "SELECT json_extract(metadata, '$.test_coverage.test_count') FROM symbols WHERE id = 'method_1'",
        [], |row| row.get(0)
    ).unwrap();
    assert!(method_cov.is_some(), "Method should have test coverage");

    // CLASS should ALSO have coverage (aggregated from methods)
    let class_cov: Option<String> = db.conn.query_row(
        "SELECT json_extract(metadata, '$.test_coverage.test_count') FROM symbols WHERE id = 'class_1'",
        [], |row| row.get(0)
    ).unwrap();
    assert!(class_cov.is_some(), "Class should inherit test coverage from its methods");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_class_inherits_method_coverage 2>&1 | tail -10`
Expected: FAIL — class has no test coverage

- [ ] **Step 3: Add coverage aggregation step**

In `src/analysis/test_coverage.rs`, after the existing logic that writes method-level coverage, add a Step 3 that propagates coverage up to parent classes:

```rust
// Step 3: Aggregate method-level coverage to parent classes/structs.
// If a class has no direct test coverage but its child methods do,
// roll up the best/worst tiers and total test count.
let mut parent_stmt = db.conn.prepare(
    "SELECT parent.id, child.id,
            json_extract(child.metadata, '$.test_coverage.test_count'),
            json_extract(child.metadata, '$.test_coverage.best_tier'),
            json_extract(child.metadata, '$.test_coverage.worst_tier')
     FROM symbols parent
     JOIN symbols child ON child.parent_symbol_id = parent.id
     WHERE parent.kind IN ('class', 'struct', 'interface', 'enum', 'trait')
       AND json_extract(child.metadata, '$.test_coverage') IS NOT NULL
       AND (json_extract(parent.metadata, '$.test_coverage') IS NULL)"
)?;

let mut parent_coverage: HashMap<String, (u32, String, String)> = HashMap::new();
let parent_rows = parent_stmt.query_map([], |row| {
    Ok((
        row.get::<_, String>(0)?,  // parent_id
        row.get::<_, u32>(2)?,     // child test_count
        row.get::<_, String>(3)?,  // child best_tier
        row.get::<_, String>(4)?,  // child worst_tier
    ))
})?;

for row in parent_rows {
    let (parent_id, child_count, child_best, child_worst) = row?;
    let entry = parent_coverage.entry(parent_id).or_insert((0, "stub".to_string(), "thorough".to_string()));
    entry.0 += child_count;
    if tier_rank(&child_best) > tier_rank(&entry.1) {
        entry.1 = child_best;
    }
    if tier_rank(&child_worst) < tier_rank(&entry.2) {
        entry.2 = child_worst;
    }
}

for (parent_id, (total_tests, best, worst)) in &parent_coverage {
    let coverage = serde_json::json!({
        "test_count": total_tests,
        "best_tier": best,
        "worst_tier": worst,
        "source": "aggregated_from_methods"
    });
    db.conn.execute(
        "UPDATE symbols SET metadata = json_set(
            COALESCE(metadata, '{}'),
            '$.test_coverage', json(?1)
        ) WHERE id = ?2",
        rusqlite::params![coverage.to_string(), parent_id],
    )?;
    stats.symbols_covered += 1;
}

debug!("Step 3 (parent aggregation): {} classes/structs got coverage from methods", parent_coverage.len());
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_class_inherits_method_coverage 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run existing coverage tests for regressions**

Run: `cargo test --lib tests::analysis::test_coverage 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/analysis/test_coverage.rs src/tests/analysis/test_coverage_tests.rs
git commit -m "feat(test_coverage): aggregate method coverage to parent classes

When a class has no direct test coverage but its child methods do,
roll up the best/worst tiers and total test count to the class. This
fixes the issue where deep_dive(LabTestService) showed 'untested' while
its individual methods had 15-28 covering tests each. Aggregated
coverage is marked with source='aggregated_from_methods'."
```

---

## Chunk 3: Centrality Propagation

### Task 6: Propagate Constructor Centrality to Parent Class

**Files:**
- Modify: `src/database/relationships.rs:252-305` (compute_reference_scores)
- Test: `src/tests/core/database.rs`

In C# with DI, all references target the constructor (via dependency injection), giving the class itself 0 centrality. Add a Step 3 to `compute_reference_scores` that propagates constructor centrality to the parent class.

- [ ] **Step 1: Write failing test**

In `src/tests/core/database.rs`, add:

```rust
#[test]
fn test_compute_reference_scores_propagates_constructor_centrality() {
    let temp = TempDir::new().unwrap();
    let db = SymbolDatabase::new(&temp.path().join("test.db")).unwrap();

    insert_file(&db, "src/services.cs");
    insert_file(&db, "src/program.cs");

    db.conn.execute_batch(r#"
        -- Class with no direct references
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility)
        VALUES ('class_1', 'LabTestService', 'class', 'csharp', 'src/services.cs', 1, 0, 100, 0, 0, 0, 0.0, 'public');

        -- Constructor referenced by DI
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility, parent_symbol_id)
        VALUES ('ctor_1', 'LabTestService', 'constructor', 'csharp', 'src/services.cs', 10, 0, 15, 0, 0, 0, 0.0, 'public', 'class_1');

        -- Multiple callers of the constructor
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility)
        VALUES ('caller_1', 'ConfigureServices', 'method', 'csharp', 'src/program.cs', 50, 0, 80, 0, 0, 0, 0.0, 'public');
        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility)
        VALUES ('caller_2', 'TestSetup', 'method', 'csharp', 'src/program.cs', 90, 0, 100, 0, 0, 0, 0.0, 'public');

        INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
        VALUES ('rel_1', 'caller_1', 'ctor_1', 'instantiates', 'src/program.cs', 55);
        INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
        VALUES ('rel_2', 'caller_2', 'ctor_1', 'uses', 'src/program.cs', 95);
    "#).unwrap();

    db.compute_reference_scores().unwrap();

    let ctor_score: f64 = db.conn.query_row(
        "SELECT reference_score FROM symbols WHERE id = 'ctor_1'", [], |row| row.get(0)
    ).unwrap();
    assert!(ctor_score > 0.0, "Constructor should have centrality from DI references");

    let class_score: f64 = db.conn.query_row(
        "SELECT reference_score FROM symbols WHERE id = 'class_1'", [], |row| row.get(0)
    ).unwrap();
    assert!(class_score > 0.0, "Class should inherit constructor centrality");
    assert!(class_score >= ctor_score * 0.5, "Class should get at least 50% of constructor centrality");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_compute_reference_scores_propagates_constructor_centrality 2>&1 | tail -10`
Expected: FAIL — class has 0 centrality

- [ ] **Step 3: Add Step 3 to compute_reference_scores**

In `src/database/relationships.rs`, after Step 2 (interface→implementation propagation), add Step 3:

```rust
// Step 3: Propagate constructor centrality to parent class.
// In C# / Java / TypeScript DI patterns, all references target the constructor,
// leaving the class itself with zero centrality. Give the class 70% of its
// constructor's score (same factor as interface→implementation propagation).
self.conn.execute(
    "UPDATE symbols SET reference_score = reference_score + COALESCE(
        (SELECT MAX(ctor.reference_score) * 0.7
         FROM symbols ctor
         WHERE ctor.parent_symbol_id = symbols.id
           AND ctor.kind = 'constructor'
           AND ctor.reference_score > 0
        ), 0.0
    )
    WHERE kind IN ('class', 'struct')
      AND reference_score = 0.0
      AND EXISTS (
          SELECT 1 FROM symbols ctor
          WHERE ctor.parent_symbol_id = symbols.id
            AND ctor.kind = 'constructor'
            AND ctor.reference_score > 0
      )",
    [],
)?;
```

Note the guard `AND reference_score = 0.0` — this only fires for classes with zero direct centrality, avoiding double-counting for classes that already have their own references.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_compute_reference_scores_propagates_constructor_centrality 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run existing reference score tests for regressions**

Run: `cargo test --lib test_compute_reference_scores 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/database/relationships.rs src/tests/core/database.rs
git commit -m "feat(centrality): propagate constructor centrality to parent class

In DI-heavy languages (C#, Java, TypeScript), all references target
the constructor via dependency injection, leaving the class with zero
centrality. Now classes with zero direct centrality inherit 70% of their
highest-scoring constructor's reference_score. Same propagation factor
as the existing interface→implementation step."
```

---

## Chunk 4: Verification & Dogfood

### Task 7: Run Full Test Suite and Dogfood

- [ ] **Step 1: Run xtask dev tier**

Run: `cargo xtask test dev`
Expected: All buckets pass (modulo known pre-existing failures)

- [ ] **Step 2: Build release for dogfooding**

Run: `cargo build --release`

- [ ] **Step 3: Restart Claude Code and re-index reference workspace**

Ask user to restart Claude Code, then:
```
manage_workspace(operation="refresh", workspace_id="labhandbookv2_67e8c1cf")
```

- [ ] **Step 4: Dogfood verification queries**

Verify each fix:

1. **Sink patterns**: `deep_dive(symbol="DatabaseMediaStorage", workspace="labhandbookv2_67e8c1cf")` — should show security risk with sink calls for SaveChangesAsync
2. **Input handling**: `deep_dive(symbol="RoleClaimsMiddleware", workspace="labhandbookv2_67e8c1cf")` — constructor should NOT show "accepts string params"
3. **Lifecycle methods**: Check that `LabTestService` methods have more accurate test counts
4. **Name disambiguation**: `ListAsync` in `LabTestService.cs` should show tests from `LabTestServiceTests` only, not `MediaServiceTests`
5. **Class coverage**: `deep_dive(symbol="LabTestService", workspace="labhandbookv2_67e8c1cf")` — class should show test coverage (aggregated from methods)
6. **Constructor centrality**: `deep_dive(symbol="LabHandbookDbContext", workspace="labhandbookv2_67e8c1cf")` — class should show non-zero centrality

- [ ] **Step 5: Commit checkpoint**

```bash
git add -A
git commit -m "docs: add cross-language code health fixes plan"
```
