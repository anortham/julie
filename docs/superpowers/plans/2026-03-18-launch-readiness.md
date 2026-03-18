# Launch Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 language bugs, complete verification matrix for all 33 languages, update docs for public launch, validate skills.

**Architecture:** Three phases — bug fixes (parallelizable TDD), then documentation (README + GH Pages + harness guides), then skill validation. Bug fixes target the resolver (`score_candidate`), Swift/Kotlin/Dart extractors, and existing Dart 3 recovery code.

**Tech Stack:** Rust, tree-sitter, SQLite, HTML/CSS/JS (site), Markdown (docs)

**Spec:** `docs/superpowers/specs/2026-03-18-launch-readiness-design.md`

---

## Phase 1A: Bug Fixes (All 4 tasks are independent — parallelize)

### Task 1: Fix Python test-subclass centrality theft

The resolver's `score_candidate` doesn't penalize test-file candidates. When `class Flask(flask.Flask)` in `tests/test_config.py` competes with the real `Flask` in `src/flask/app.py`, the test class can win on proximity scoring because tests import from it. The centrality de-weight in Step 4 of `compute_reference_scores` (×0.1) isn't enough when raw score 213×0.1=21.3 still beats 1.4.

**Files:**
- Modify: `src/tools/workspace/indexing/resolver.rs:117-178` (`score_candidate`)
- Test: `src/tests/tools/workspace/resolver.rs` (existing resolver test file)
- Reference: `src/database/relationships.rs:372-419` (existing Step 4 de-weight)
- Reference: `src/search/scoring.rs` (`is_test_path` function — `crate::search::scoring::is_test_path`)

- [ ] **Step 1: Review existing resolver tests**

Existing tests are at `src/tests/tools/workspace/resolver.rs`. Study the test helpers (e.g., `make_symbol`) and imports. The existing tests use `select_best_candidate` (the public API) — **not** `score_candidate` (which is private). New tests MUST go through `select_best_candidate`.

Also check whether `Symbol` and `PendingRelationship` derive `Default`. If not, use the existing `make_symbol` helper pattern.

- [ ] **Step 2: Write failing test — production class wins over test subclass**

Test via `select_best_candidate` with both candidates in the list:

```rust
#[test]
fn test_select_best_candidate_prefers_production_over_test_file() {
    // Production Flask class — use make_symbol helper or construct manually
    let prod = make_symbol("prod_flask", "Flask", SymbolKind::Class, "src/flask/app.py", "python");
    let test = make_symbol("test_flask", "Flask", SymbolKind::Class, "tests/test_config.py", "python");

    let pending = PendingRelationship {
        from_symbol_id: "caller_id".into(),
        file_path: "src/flask/helpers.py".into(),
        callee_name: "Flask".into(),
        kind: RelationshipKind::Calls,
        ..  // fill remaining fields
    };

    let parent_ctx = ParentReferenceContext::default();
    let candidates = vec![prod.clone(), test.clone()];
    let best = select_best_candidate(&candidates, &pending, &parent_ctx);

    assert_eq!(best.map(|s| &s.id), Some(&"prod_flask".to_string()),
        "Production Flask should win over test Flask");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib test_select_best_candidate_prefers_production 2>&1 | tail -10`
Expected: FAIL (currently no test-file penalty — both get same score, first wins by position)

- [ ] **Step 4: Implement test-file penalty in `score_candidate`**

In `src/tools/workspace/indexing/resolver.rs`, after the import-constrained disambiguation block (line ~176) and before the `score` return, add:

```rust
// De-preference test file candidates. Production code should resolve to
// production definitions, not test doubles/subclasses. The penalty is strong
// enough to override proximity (+50) and kind match (+10), but NOT strong
// enough to override parent-reference context (+200) — if a test file
// genuinely imports from a test helper, that should still resolve correctly.
if crate::search::scoring::is_test_path(&candidate.file_path) {
    score = score.saturating_sub(75);
}
```

> **Watch out:** The spec warns this may not be sufficient. If the -75 penalty doesn't produce a PASS at re-verification (Task 5), the problem is deeper — the resolver may need stronger penalization, or `compute_reference_scores()` Step 1b/Step 4 may need adjustment. TDD will reveal which layer needs the fix.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib test_select_best_candidate_prefers_production 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Write edge case test — test-only symbol still resolves**

```rust
#[test]
fn test_select_best_candidate_test_symbol_resolves_when_only_option() {
    let test = make_symbol("test_helper", "TestHelper", SymbolKind::Class, "tests/helpers.py", "python");

    let pending = PendingRelationship {
        from_symbol_id: "caller_id".into(),
        file_path: "tests/test_config.py".into(),
        callee_name: "TestHelper".into(),
        kind: RelationshipKind::Calls,
        .. // fill remaining fields
    };

    let parent_ctx = ParentReferenceContext::default();
    let candidates = vec![test.clone()];
    let best = select_best_candidate(&candidates, &pending, &parent_ctx);

    assert!(best.is_some(), "Test-only symbol should still resolve when it's the only candidate");
}
```

- [ ] **Step 7: Run edge case test**

Run: `cargo test --lib test_select_best_candidate_test_symbol_resolves 2>&1 | tail -10`
Expected: PASS (penalty subtracts from score but doesn't zero it)

- [ ] **Step 8: Run xtask dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: No new failures

- [ ] **Step 9: Commit**

```bash
git add src/tools/workspace/indexing/resolver.rs src/tests/core/resolver_tests.rs src/tests/mod.rs
git commit -m "fix(resolver): penalize test-file candidates in disambiguation

Test subclasses (e.g., test Flask extending real Flask) could win resolver
disambiguation, causing centrality to accumulate on the wrong symbol.
Add a -75 penalty for candidates in test paths, strong enough to override
proximity bonuses but not parent-reference context."
```

---

### Task 2: Fix Swift `Session` class extraction

`open class Session: @unchecked Sendable` is missing because `node.child_by_field_name("name")` returns `None`. The `@unchecked` attribute likely changes the AST structure.

**Files:**
- Modify: `crates/julie-extractors/src/swift/types.rs:11-18`
- Test: `crates/julie-extractors/src/tests/swift_tests.rs` (or new file)
- Reference: `crates/julie-extractors/src/swift/mod.rs:66` (dispatch)

- [ ] **Step 1: Reproduce with tree-sitter AST dump**

Write a test that parses `open class Session: @unchecked Sendable { }` and dumps the AST to understand the node structure:

```rust
#[test]
fn test_swift_class_with_unchecked_sendable() {
    let source = "open class Session: @unchecked Sendable {\n    func connect() {}\n}";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_swift::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    // Print AST to understand structure
    println!("{}", tree.root_node().to_sexp());
    panic!("Dump AST — check output");
}
```

Run: `cargo test --lib test_swift_class_with_unchecked_sendable -- --nocapture 2>&1 | tail -30`

Study the S-expression output. Look for where the `name` field is — it might be wrapped in an attribute modifier node.

- [ ] **Step 2: Write failing test for extraction**

Based on the AST dump, write a test that extracts symbols from the same source and asserts `Session` class is found:

```rust
#[test]
fn test_extract_swift_class_unchecked_sendable() {
    let source = "open class Session: @unchecked Sendable {\n    func connect() {}\n}";
    let mut extractor = SwiftExtractor::new(
        "swift".into(),
        "Session.swift".into(),
        source.into(),
        Path::new("/test"),
    );
    let tree = /* parse source */;
    let symbols = extractor.extract_symbols(&tree);

    let session = symbols.iter().find(|s| s.name == "Session");
    assert!(session.is_some(), "Session class should be extracted. Got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert_eq!(session.unwrap().kind, SymbolKind::Class);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib test_extract_swift_class_unchecked_sendable 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 4: Fix `extract_class` in `swift/types.rs`**

Based on the AST dump from Step 1, fix the name extraction. Likely solutions:
- If `child_by_field_name("name")` fails because the attribute wrapper changes field positions, add a fallback that searches children for `type_identifier` or `user_type` nodes.
- If the entire declaration is wrapped in an ERROR node, add recovery logic similar to Kotlin's approach.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib test_extract_swift_class_unchecked_sendable 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/julie-extractors/src/swift/types.rs crates/julie-extractors/src/tests/swift_tests.rs
git commit -m "fix(swift): extract class declarations with @unchecked attribute

Session: @unchecked Sendable syntax caused child_by_field_name('name')
to return None. Added fallback extraction for this pattern."
```

---

### Task 3: Fix Kotlin sealed class `JsonReader` extraction

`sealed class JsonReader` is absent while `sealed class JsonWriter` works. The Kotlin extractor matches `class_declaration` and has ERROR recovery, but something fails for JsonReader specifically.

**Files:**
- Modify: `crates/julie-extractors/src/kotlin/types.rs:13` or `crates/julie-extractors/src/kotlin/mod.rs:60-148`
- Test: `crates/julie-extractors/src/tests/kotlin_tests.rs` (or new file)

- [ ] **Step 1: Reproduce with tree-sitter AST dump**

Parse the actual `JsonReader` source from moshi to see what tree-sitter produces:

```rust
#[test]
fn test_kotlin_sealed_class_ast() {
    let source = r#"sealed class JsonReader : Closeable {
  abstract fun beginArray(): JsonReader
  abstract fun endArray(): JsonReader
}"#;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_kotlin::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    println!("{}", tree.root_node().to_sexp());
    panic!("Dump AST — check output");
}
```

Run: `cargo test --lib test_kotlin_sealed_class_ast -- --nocapture 2>&1 | tail -30`

Compare with `sealed class JsonWriter` AST — find what differs. Key question: does tree-sitter produce `class_declaration` or `ERROR` for JsonReader?

- [ ] **Step 2: Write failing test for extraction**

```rust
#[test]
fn test_extract_kotlin_sealed_class() {
    let source = r#"sealed class JsonReader : Closeable {
  abstract fun beginArray(): JsonReader
  abstract fun endArray(): JsonReader
}"#;
    // ... create extractor, parse, extract symbols ...

    let reader = symbols.iter().find(|s| s.name == "JsonReader");
    assert!(reader.is_some(), "JsonReader should be extracted. Got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert_eq!(reader.unwrap().kind, SymbolKind::Class);

    // Members should be parented to JsonReader
    let methods: Vec<_> = symbols.iter()
        .filter(|s| s.parent_id.as_deref() == Some(&reader.unwrap().id))
        .collect();
    assert!(methods.len() >= 2, "Should have at least 2 child methods");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib test_extract_kotlin_sealed_class 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 4: Fix based on AST analysis**

Likely fixes:
- If `class_declaration` is produced but `extract_class` fails to find the identifier (because sealed modifier shifts children), fix the child lookup in `kotlin/types.rs`.
- If tree-sitter produces an ERROR node, enhance the ERROR recovery in `kotlin/mod.rs:132-148` to handle the sealed modifier pattern.
- If the issue is specific to `JsonReader` content (e.g., abstract members cause parse error), handle that pattern.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib test_extract_kotlin_sealed_class 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/julie-extractors/src/kotlin/ crates/julie-extractors/src/tests/kotlin_tests.rs
git commit -m "fix(kotlin): extract sealed class declarations that tree-sitter misparses"
```

---

### Task 4: Debug Dart 3 modifier class recovery

The recovery code already exists in `dart/mod.rs` (`recover_dart3_modifier_class` at line 325, called from `visit_node` at line 174). The verification found ProviderContainer (`base class`) and AsyncValue (`sealed class`) are STILL missing. Debug why.

**Files:**
- Modify: `crates/julie-extractors/src/dart/mod.rs:169-210` or `crates/julie-extractors/src/dart/mod.rs:325-411`
- Test: `crates/julie-extractors/src/tests/dart_tests.rs` (or new file)

- [ ] **Step 1: Reproduce with actual riverpod source**

Find the actual `ProviderContainer` declaration in riverpod. It's likely:
```dart
base class ProviderContainer {
  // ...
}
```

Write a test that parses this and dumps the AST:

```rust
#[test]
fn test_dart3_base_class_ast() {
    let source = "base class ProviderContainer {\n  void dispose() {}\n}";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_dart::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    println!("{}", tree.root_node().to_sexp());
    // Also check: is the ERROR node at program level?
    let root = tree.root_node();
    for i in 0..root.child_count() {
        let child = root.child(i).unwrap();
        println!("Child {}: kind={}, text={:?}", i, child.kind(), &source[child.byte_range()]);
    }
    panic!("Dump AST");
}
```

Run: `cargo test --lib test_dart3_base_class_ast -- --nocapture 2>&1 | tail -30`

Key questions:
- Is the ERROR node at program level (parent == "program")? The guard at line 170 requires this.
- Does the ERROR node contain the expected child pattern (modifier + "class" keyword + name)?
- Is the class body (`block`) a **sibling** of the ERROR node?

- [ ] **Step 2: Write failing test for extraction**

```rust
#[test]
fn test_extract_dart3_base_class() {
    let source = "base class ProviderContainer {\n  void dispose() {}\n}";
    // ... create DartExtractor, parse, extract symbols ...

    let container = symbols.iter().find(|s| s.name == "ProviderContainer");
    assert!(container.is_some(), "ProviderContainer should be extracted. Got: {:?}",
        symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
    assert_eq!(container.unwrap().kind, SymbolKind::Class);
}
```

Also test `sealed class`:
```rust
#[test]
fn test_extract_dart3_sealed_class() {
    let source = "sealed class AsyncValue<T> {\n  const AsyncValue();\n}";
    // ... similar assertion ...
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib test_extract_dart3_ 2>&1 | tail -10`
Expected: FAIL (recovery not triggering for these patterns)

- [ ] **Step 4: Fix based on AST analysis**

Common reasons the recovery fails:
- ERROR node not at program level (nested in another node)
- Modifier text doesn't match `DART3_CLASS_MODIFIERS` (case sensitivity, extra whitespace)
- Class body isn't a sibling `block` node (might be inside the ERROR node)
- `expression_statement` instead of `ERROR` for some modifier patterns

Fix the specific pattern mismatch found in Step 1.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib test_extract_dart3_ 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/julie-extractors/src/dart/ crates/julie-extractors/src/tests/dart_tests.rs
git commit -m "fix(dart): handle Dart 3 class modifier recovery for base/sealed patterns

The recovery code existed but wasn't triggering for [specific reason].
Fixed [specific fix]."
```

---

## Phase 1B: Re-verify Fixed Languages

After completing Tasks 1-4, re-verify each fixed language against its reference project.

### Task 5: Re-verify Python against pallets/flask

**Prerequisites:** Task 1 complete

- [ ] **Step 1: Add flask as reference workspace** (if not already added)

```
manage_workspace(operation="add", path="/path/to/flask", name="flask")
```

Wait for indexing to complete.

- [ ] **Step 2: Verify centrality fix**

```
deep_dive(symbol="Flask", depth="overview", context_file="app.py")
```

Expected: Flask class centrality >0.5 (was ~0 before fix).

```
deep_dive(symbol="Flask", depth="overview", context_file="test_config")
```

Expected: Test Flask centrality < production Flask centrality.

- [ ] **Step 3: Verify definition search fix**

```
fast_search(query="Flask", search_target="definitions", language="python")
```

Expected: Real Flask from `src/flask/app.py` ranks #1.

- [ ] **Step 4: Update verification results**

Update `docs/LANGUAGE_VERIFICATION_RESULTS.md` — Python row:
- Centrality: FAIL → PASS
- Def Search: FAIL → PASS

- [ ] **Step 5: Commit**

```bash
git add docs/LANGUAGE_VERIFICATION_RESULTS.md
git commit -m "docs: update Python verification results — centrality theft fixed"
```

---

### Task 6: Re-verify Swift against Alamofire/Alamofire

**Prerequisites:** Task 2 complete

- [ ] **Step 1: Add Alamofire as reference workspace**
- [ ] **Step 2: Verify Session class extracted**

```
get_symbols(file_path="Source/Core/Session.swift", max_depth=1, mode="structure")
```

Expected: Session class with ~80 child methods.

- [ ] **Step 3: Verify centrality and deep_dive**

```
deep_dive(symbol="Session", depth="overview", context_file="Session.swift")
```

Expected: Session has reasonable centrality, methods parented correctly.

- [ ] **Step 4: Update verification results**

Update Swift row: Symbols PARTIAL→PASS, Centrality PARTIAL→PASS, deep_dive PARTIAL→PASS

- [ ] **Step 5: Commit**

---

### Task 7: Re-verify Kotlin against square/moshi

**Prerequisites:** Task 3 complete

- [ ] **Step 1: Add moshi as reference workspace**
- [ ] **Step 2: Verify JsonReader extracted**

```
get_symbols(file_path="moshi/src/main/kotlin/com/squareup/moshi/JsonReader.kt", max_depth=1, mode="structure")
```

Expected: JsonReader class with 30+ child methods.

- [ ] **Step 3: Update verification results**

Update Kotlin row: Symbols PARTIAL→PASS, deep_dive PARTIAL→PASS

- [ ] **Step 4: Commit**

---

### Task 8: Re-verify Dart against rrousselGit/riverpod

**Prerequisites:** Task 4 complete

- [ ] **Step 1: Add riverpod as reference workspace**
- [ ] **Step 2: Verify ProviderContainer and AsyncValue extracted**

```
fast_search(query="ProviderContainer", search_target="definitions", language="dart")
fast_search(query="AsyncValue", search_target="definitions", language="dart")
```

Expected: Both found as Class kind.

- [ ] **Step 3: Update verification results**

Update Dart row: Symbols PARTIAL→PASS

- [ ] **Step 4: Commit**

---

## Phase 1C: New Language Verifications

Follow `docs/LANGUAGE_VERIFICATION_CHECKLIST.md` for each language. These are independent and can be parallelized.

### Task 9: Verify Rust (Full Tier)

**Reference project:** Julie itself (already indexed as primary workspace)

- [ ] **Step 1: Run all 8 verification checks** against Julie's own codebase using the checklist
- [ ] **Step 2: Record results** in `docs/LANGUAGE_VERIFICATION_RESULTS.md`
- [ ] **Step 3: Fix any bugs found** (TDD — write failing test, fix, verify)
- [ ] **Step 4: Commit**

### Task 10: Verify JavaScript (Full Tier)

**Reference project:** Pick a focused JS project (e.g., expressjs/express, lodash, or similar pure JS project — NOT TypeScript)

- [ ] **Step 1: Clone and add as reference workspace**
- [ ] **Step 2: Run all 8 verification checks**
- [ ] **Step 3: Record results**
- [ ] **Step 4: Fix any bugs found** (TDD)
- [ ] **Step 5: Commit**

### Task 11: Verify Specialized Tier (7 languages)

**Reference projects:** Bash (ohmyzsh), PowerShell (PSScriptAnalyzer), Vue (pinia), R (ggplot2), SQL (flyway), HTML (any web project), CSS (any web project)

For each: Checks 1, 3, 5, 8 only (per checklist Specialized Tier criteria).

- [ ] **Step 1: Clone and index each project**
- [ ] **Step 2: Run applicable checks for each language**
- [ ] **Step 3: Record results** in verification matrix
- [ ] **Step 4: Fix any bugs found** (TDD)
- [ ] **Step 5: Commit results**

### Task 12: Verify Data/Docs Tier (5 languages)

**Reference project:** Julie itself (Markdown in docs/, JSON/TOML/YAML in config files, Regex in extractor)

Check 1 only (symbol extraction).

- [ ] **Step 1: Run Check 1 for Markdown, JSON, TOML, YAML, Regex**

```
get_symbols(file_path="docs/ARCHITECTURE.md", max_depth=1, mode="structure")
get_symbols(file_path="Cargo.toml", max_depth=1, mode="structure")
get_symbols(file_path=".github/workflows/ci.yml", max_depth=1, mode="structure")
```

- [ ] **Step 2: Record results** — expect all PASS for basic symbol extraction
- [ ] **Step 3: Commit**

---

## Phase 2: Documentation

### Task 13: README quick fixes

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Fix language count**

Replace "31" with "33" in all occurrences:
- Line 2: "across 31 programming languages" → "across 33 programming languages"
- Line 22: "Cross-language code navigation... across 31 languages" → "33 languages"
- Line 52: "Supported Languages (31)" → "Supported Languages (33)"
- Line 176: "Test detection across all 31 languages" → "33 languages"

- [ ] **Step 2: Fix language list**

Add Scala to Core section, add Elixir with new Functional section:
```markdown
**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala

**Systems:** C, C++, Go, Lua, Zig

**Functional:** Elixir

**Specialized:** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart

**Documentation:** Markdown, JSON, TOML, YAML
```

Remove JSONL from Documentation (it's a JSON file extension alias, not a separate language).

- [ ] **Step 3: Fix project structure comment**

Line 276: update `extractors/` comment from "31 languages" to "33 languages".

- [ ] **Step 4: Fix tool count and add query_metrics**

Change "Tools (7)" to "Tools (8)". Add after manage_workspace:

```markdown
### Code Health & Metrics

- `query_metrics` - Query pre-computed code health metrics
  - Sort by security risk, change risk, centrality, or test coverage
  - Filter by risk level, test status, symbol kind, file pattern, and language
  - Powers the `/codehealth`, `/security-audit`, and `/architecture` skills
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: update README — 33 languages, 8 tools, remove JSONL alias"
```

---

### Task 14: README Skills section + installation guide

**Prerequisites:** Task 13 complete (both modify README.md — MUST be sequential, not parallel).

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add Skills section** (after Code Health Intelligence)

```markdown
## Skills

Julie ships with 10 pre-built skills — reusable prompt workflows that combine Julie's tools into higher-level capabilities. Skills are invoked as slash commands (e.g., `/codehealth`).

### Report Skills

| Skill | Description |
|-------|-------------|
| `/codehealth` | Risk hotspots, test gaps, dead code candidates, and prioritized recommendations |
| `/security-audit` | Security risk analysis with plain-language explanations of risky patterns |
| `/architecture` | Architecture overview — entry points, module map, dependency flow, reading order |

### Navigation & Analysis Skills

| Skill | Description |
|-------|-------------|
| `/explore-area` | Orient on an unfamiliar area of the codebase using token-budgeted exploration |
| `/call-trace` | Trace the call path between two functions |
| `/logic-flow` | Step-by-step explanation of a function's logic and control flow |
| `/impact-analysis` | Analyze blast radius of changing a symbol — callers grouped by risk |
| `/dependency-graph` | Show module dependencies by analyzing imports and cross-references |
| `/type-flow` | Trace how types flow through a function — parameters, transforms, returns |
| `/search-debug` | Diagnose why a search returns unexpected results (for Julie development) |
```

- [ ] **Step 2: Add skill installation guide**

```markdown
### Installing Skills

Skills are shipped as markdown files in `.claude/skills/`. Installation depends on your AI coding tool:

**Claude Code** — skills work automatically when Julie's repo is cloned. To use them in other projects, copy `.claude/skills/` to your project root.

**Other tools** — see below for where to copy skill files:
```

- [ ] **Step 3: Web research for harness-specific paths**

Research current skill/rules file locations for:
- VS Code / GitHub Copilot
- Cursor
- Windsurf
- Gemini CLI
- Codex CLI
- OpenCode

Use `WebSearch` to find current documentation for each harness.

- [ ] **Step 4: Write harness-specific instructions** based on research

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: add Skills section with installation guide for multiple harnesses"
```

---

### Task 15: GitHub Pages site updates

**Files:**
- Modify: `docs/site/index.html`
- Modify: `docs/site/script.js` (for new install tab logic if needed)

- [ ] **Step 1: Add Skills section** between Code Health (section 5) and Tools (section 7)

Create a new section with terminal mockups for the 3 report skills. List the 7 navigation/analysis skills below.

- [ ] **Step 2: Update tool count and add query_metrics card**

Line 327: "7 tools" → "8 tools"
Add an 8th tool card for `query_metrics` in the card grid.

- [ ] **Step 3: Add Gemini CLI and Codex CLI install tabs**

Add two new tabs in the install section (section 10) with correct MCP config format. Research correct format via web search.

- [ ] **Step 4: Update footer version**

Line 590: update version to current release.

- [ ] **Step 5: Commit**

```bash
git add docs/site/
git commit -m "docs(site): add Skills section, query_metrics card, Gemini/Codex install tabs"
```

---

### Task 16: Document known limitations

**Files:**
- Modify: `docs/LANGUAGE_VERIFICATION_RESULTS.md`

- [ ] **Step 1: Add Known Limitations section**

After the per-language details, add a summary of accepted limitations:

```markdown
## Known Limitations (Accepted)

| Language | Limitation | Workaround |
|----------|-----------|------------|
| C++ | Zero cross-file references in header-only projects | Most C++ projects with .cpp files work correctly |
| C | Centrality split between header and implementation | Header gets refs; use `context_file` to reach implementation |
| PHP | Class-level relationship tracking weak for namespace-heavy code | Method-level refs work; use `language` filter |
| Ruby | Centrality accumulates on constant instead of class symbol | Class still found via search; centrality ranking affected |
| Lua | Class-like tables stored as variable kind | Lua has no class keyword; metatables are detected as variables |
| Go | Markdown headings can outrank Go structs without language filter | Use `language="go"` for accurate results |
```

- [ ] **Step 2: Commit**

```bash
git add docs/LANGUAGE_VERIFICATION_RESULTS.md
git commit -m "docs: add known limitations section to verification results"
```

---

## Phase 3: Skill Validation

### Task 17: Validate all 10 skills via skill-creator

**Files:**
- Potentially modify: `.claude/skills/*/SKILL.md` (any of the 10 skill files)

- [ ] **Step 1: Run skill-creator reviewer on each skill**

Invoke `skill-creator:skill-creator` to review each of the 10 skills:
1. `codehealth`
2. `security-audit`
3. `architecture`
4. `explore-area`
5. `call-trace`
6. `logic-flow`
7. `impact-analysis`
8. `dependency-graph`
9. `type-flow`
10. `search-debug`

For each skill, check:
- Description triggers correctly
- Allowed-tools are complete
- Query patterns match current tool APIs
- Output format is well-structured

- [ ] **Step 2: Fix any issues flagged**
- [ ] **Step 3: Commit**

```bash
git add .claude/skills/
git commit -m "chore(skills): apply quality fixes from skill-creator review"
```

---

## Parallelization Guide

```
Independent (can run simultaneously):
├── Task 1 (Python fix)
├── Task 2 (Swift fix)
├── Task 3 (Kotlin fix)
├── Task 4 (Dart fix)
├── Task 9 (Rust verification)
├── Task 10 (JS verification)
├── Task 11 (Specialized tier)
├── Task 12 (Data/Docs tier)
├── Task 13 (README quick fixes — trivial, do immediately)
└── Task 17 (skill validation) — independent, run anytime

Sequential dependencies:
├── Task 1 → Task 5 (Python re-verify)
├── Task 2 → Task 6 (Swift re-verify)
├── Task 3 → Task 7 (Kotlin re-verify)
├── Task 4 → Task 8 (Dart re-verify)
├── Task 13 → Task 14 (both modify README.md — must be sequential)
└── Tasks 5-12 → Tasks 14-16 (docs need final verification state)

⚠️ Compilation note: Tasks 2, 3, 4 all modify the `julie-extractors` crate.
Running all three as parallel subagents will cause compile-lock contention.
Recommend running at most 2 extractor tasks concurrently.
```
