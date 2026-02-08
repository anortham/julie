# Round 4: Extractor Quality Improvements (B → B+)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring 10 B-rated extractors to B+ quality across correctness fixes, feature additions, noise reduction, and code quality improvements.

**Architecture:** Each task targets a single extractor with focused fixes. Changes are independent — no cross-task dependencies. TDD: write failing test, implement fix, verify, commit.

**Tech Stack:** Rust, tree-sitter, regex, serde_json. Test harness: `cargo test -p julie-extractors`.

**Run all extractor tests:** `cargo test -p julie-extractors --lib 2>&1 | tail -5`

---

### Task 1: JavaScript — Fix aliased import duplicates

**Problem:** `import { createElement as h }` creates TWO import symbols — one for "createElement" and one for "h". Should only create one.

**Files:**
- Modify: `crates/julie-extractors/src/javascript/imports.rs:67-74`
- Test: `crates/julie-extractors/src/tests/javascript/modern_features.rs`

**Step 1: Write failing test**

In the test file, add a test that verifies aliased imports don't create duplicates:

```rust
#[test]
fn test_aliased_import_no_duplicate() {
    let code = r#"import { createElement as h, Fragment } from 'react';"#;
    let symbols = extract_symbols(code);
    // Should have exactly 1 import symbol for the source, not separate symbols for each specifier
    // The key point: "createElement" and "h" should NOT both appear as separate import symbols
    let import_symbols: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();
    // We expect 1 import symbol (the source "react"), not 3
    assert_eq!(import_symbols.len(), 1, "Aliased imports should not create duplicate symbols. Found: {:?}", import_symbols.iter().map(|s| &s.name).collect::<Vec<_>>());
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p julie-extractors test_aliased_import_no_duplicate -- --nocapture 2>&1 | tail -20
```

**Step 3: Fix the code**

In `imports.rs` lines 67-74, the `import_specifier` case pushes BOTH name AND alias:

```rust
// CURRENT (broken):
"import_specifier" => {
    if let Some(name_node) = child.child_by_field_name("name") {
        specifiers.push(self.base.get_node_text(&name_node));
    }
    if let Some(alias_node) = child.child_by_field_name("alias") {
        specifiers.push(self.base.get_node_text(&alias_node));
    }
}
```

Fix: when alias exists, push only alias (the local binding name). When no alias, push name:

```rust
// FIXED:
"import_specifier" => {
    if let Some(alias_node) = child.child_by_field_name("alias") {
        // Aliased import: use local binding name (e.g., "h" from "createElement as h")
        specifiers.push(self.base.get_node_text(&alias_node));
    } else if let Some(name_node) = child.child_by_field_name("name") {
        // Non-aliased import: use the imported name directly
        specifiers.push(self.base.get_node_text(&name_node));
    }
}
```

Also check the `named_imports` case at lines 84-93 — it currently does NOT handle aliases inside named_imports, which is correct (aliases there go through the `import_specifier` path during tree-sitter recursion).

**Step 4: Run tests**

```bash
cargo test -p julie-extractors test_aliased_import_no_duplicate -- --nocapture 2>&1 | tail -20
cargo test -p julie-extractors --lib 2>&1 | tail -5
```

Verify no existing tests break — the `modern_features.rs` test at line 456 looks for `s.name == "createElement"` which will need to be updated to look for the alias "h" instead, since that's now the local binding name. Alternatively, if the test expects the original name, check whether the import symbol's name should be the source ("react") rather than individual specifiers. **Investigate the existing test expectations before finalizing.**

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/javascript/imports.rs crates/julie-extractors/src/tests/javascript/
git commit -m "fix(javascript): stop creating duplicate symbols for aliased imports"
```

---

### Task 2: Kotlin — Fix companion object naming + param type sentinel

**Problem 1:** `companion object Factory { }` always gets name "Companion" instead of "Factory".
**Problem 2:** Constructor parameters with no type get empty string `""` instead of being skipped.

**Files:**
- Modify: `crates/julie-extractors/src/kotlin/types.rs:214-227`
- Modify: `crates/julie-extractors/src/kotlin/properties.rs:168`
- Test: `crates/julie-extractors/src/tests/kotlin/mod.rs`

**Step 1: Write failing test for companion naming**

```rust
#[test]
fn test_named_companion_object() {
    let code = r#"
        class MyClass {
            companion object Factory {
                fun create(): MyClass = MyClass()
            }
        }
    "#;
    let symbols = extract_symbols(code);
    // Named companion should use its actual name "Factory", not "Companion"
    let factory = symbols.iter().find(|s| s.name == "Factory");
    assert!(factory.is_some(), "Named companion object should use custom name 'Factory', not 'Companion'");
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p julie-extractors test_named_companion_object -- --nocapture 2>&1 | tail -20
```

**Step 3: Fix companion naming**

In `types.rs`, the `extract_companion_object` function at lines 214-227:

```rust
// CURRENT (line 226): Always "Companion" when no custom name
let name = if let Some(ref name_node) = name_node {
    let custom_name = base.get_node_text(name_node);
    signature.push_str(&format!(" {}", custom_name));
    custom_name
} else {
    "Companion".to_string()  // ← This is fine for UNNAMED companions
};
```

Wait — re-read this code. The issue is that `name_node` lookup at line 219-222 searches for the FIRST `identifier` child. In `companion object Factory`, the grammar might place "Factory" as an identifier child. Verify: if the lookup IS finding "Factory", then the companion IS getting the right name. If it's NOT finding it, the tree-sitter-kotlin grammar might use a different field name.

**Debug approach:** Add a test that prints the symbol names. If "Factory" IS extracted correctly already, then the bug might be limited to specific grammar versions.

**If the lookup works:** The fix is already in place. Just ensure the test passes and the existing test at `mod.rs:289` (which expects `s.name == "Companion"` for an UNNAMED companion) is preserved.

**If the lookup doesn't work:** Check the tree-sitter-kotlin AST structure for `companion object Factory` — the name might be under `child_by_field_name("name")` instead of a bare identifier search.

**Step 4: Fix param type sentinel**

In `properties.rs:168`:
```rust
// CURRENT:
.unwrap_or_else(|| "".to_string());

// FIX: Use unwrap_or_default() for clarity (same result, cleaner idiom).
// The empty string is already handled correctly downstream — the signature
// building at line 207 conditionally includes the type only when non-empty.
// Verify this by checking the signature construction.
```

If the downstream code handles empty strings correctly (doesn't produce "paramName: " with trailing colon), then this is a cosmetic fix. If it DOES produce noise, change the type to `Option<String>` and skip type in signature when None.

**Step 5: Run all tests and commit**

```bash
cargo test -p julie-extractors --lib 2>&1 | tail -5
git add crates/julie-extractors/src/kotlin/
git commit -m "fix(kotlin): use custom name for named companion objects"
```

---

### Task 3: Zig — Detect @import as SymbolKind::Import

**Problem:** `const std = @import("std")` is extracted as `SymbolKind::Variable`. It should be `SymbolKind::Import` since `@import` is Zig's module import mechanism.

**Files:**
- Modify: `crates/julie-extractors/src/zig/variables.rs`
- Test: `crates/julie-extractors/src/tests/zig/mod.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_import_detection() {
    let code = r#"
        const std = @import("std");
        const Allocator = std.mem.Allocator;
        const testing = @import("testing");
    "#;
    let symbols = extract_symbols(code);
    let std_import = symbols.iter().find(|s| s.name == "std").unwrap();
    assert_eq!(std_import.kind, SymbolKind::Import, "const with @import should be SymbolKind::Import");
    let testing_import = symbols.iter().find(|s| s.name == "testing").unwrap();
    assert_eq!(testing_import.kind, SymbolKind::Import);
    // Non-import const should remain Variable/Constant
    let allocator = symbols.iter().find(|s| s.name == "Allocator").unwrap();
    assert_ne!(allocator.kind, SymbolKind::Import, "Non-import const should not be Import");
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p julie-extractors test_import_detection -- --nocapture 2>&1 | tail -20
```

**Step 3: Implement @import detection**

In `variables.rs`, find the `extract_variable` function. After extracting the variable name and before creating the symbol, check if the initializer expression contains a `@import` builtin call:

```rust
// After determining the initial SymbolKind (Variable or Constant), check for @import:
// Look for builtin_call or function_call child with text starting with "@import"
let is_import = node.children(&mut node.walk()).any(|child| {
    if child.kind() == "builtin_call_expr" || child.kind() == "builtin_expression" {
        let text = base.get_node_text(&child);
        text.starts_with("@import")
    } else {
        false
    }
});

if is_import {
    kind = SymbolKind::Import;
}
```

**Note:** The exact tree-sitter-zig node kind for `@import("std")` needs verification. Common possibilities: `builtin_call_expr`, `builtin_expression`, `application_expression`. Debug by printing node children kinds in a test.

**Step 4: Run tests**

```bash
cargo test -p julie-extractors test_import_detection -- --nocapture 2>&1 | tail -20
cargo test -p julie-extractors --lib 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/zig/
git commit -m "feat(zig): detect @import as SymbolKind::Import"
```

---

### Task 4: Lua — Refactor variables.rs to reduce duplication

**Problem:** `variables.rs` at 460 lines has significant code duplication between `extract_local_variable_declaration` (16-130), `extract_assignment_statement` (133-334), and `extract_variable_assignment` (337-460). The type inference, metadata construction, and symbol creation logic is nearly identical across all three.

**Note:** require() → Import detection already works (helpers.rs:48-54). The "unknown" sentinels listed in the audit have already been cleaned up (helpers.rs returns `String::new()`, not `"unknown"`).

**Files:**
- Modify: `crates/julie-extractors/src/lua/variables.rs`
- Test: `crates/julie-extractors/src/tests/lua/`

**Step 1: Run existing tests to establish baseline**

```bash
cargo test -p julie-extractors --lib -- lua 2>&1 | tail -10
```

All Lua tests must pass before and after refactoring.

**Step 2: Extract common code into helper functions**

Create helper functions for the duplicated logic:

```rust
/// Determine SymbolKind and data type from an expression node
fn infer_kind_and_type(base: &BaseExtractor, expression: Option<&Node>) -> (SymbolKind, String) {
    // ... common logic from lines 62-90 (local), and equivalent in assignment/variable_assignment
}

/// Build metadata HashMap for a variable symbol
fn build_variable_metadata(data_type: &str, additional: Option<HashMap<String, serde_json::Value>>) -> HashMap<String, serde_json::Value> {
    // ... common metadata construction
}
```

**Step 3: Simplify the three extraction functions to call the helpers**

Each function should focus on its unique aspects (how to find variable names, what visibility to use) and delegate common work to the helpers.

Target: reduce `variables.rs` from ~460 lines to ~350 lines.

**Step 4: Run all Lua tests**

```bash
cargo test -p julie-extractors --lib -- lua 2>&1 | tail -10
```

All tests must still pass — this is a pure refactor, no behavior change.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/lua/variables.rs
git commit -m "refactor(lua): extract common variable creation logic to reduce duplication"
```

---

### Task 5: QML — Add id: binding, enum, and alias property extraction

**Problem:** QML `id:` property declarations (critical for referencing), enum declarations, and alias properties are not extracted. The `traverse_node` dispatch only handles `ui_object_definition`, `ui_property`, `ui_signal`, and `function_declaration`.

**Files:**
- Modify: `crates/julie-extractors/src/qml/mod.rs:48-117` (traverse_node)
- Test: `crates/julie-extractors/src/tests/qml/basics.rs` or new test file

**Step 1: Write failing tests**

```rust
#[test]
fn test_extract_qml_id_binding() {
    let code = r#"
        import QtQuick 2.15
        Rectangle {
            id: root
            width: 400

            Text {
                id: label
                text: "Hello"
            }
        }
    "#;
    let symbols = extract_symbols(code);
    let root_id = symbols.iter().find(|s| s.name == "root" && s.kind == SymbolKind::Property);
    assert!(root_id.is_some(), "id: root should be extracted as Property");
    let label_id = symbols.iter().find(|s| s.name == "label" && s.kind == SymbolKind::Property);
    assert!(label_id.is_some(), "id: label should be extracted as Property");
}

#[test]
fn test_extract_qml_enum() {
    let code = r#"
        import QtQuick 2.15
        Item {
            enum Direction { Left, Right, Up, Down }
        }
    "#;
    let symbols = extract_symbols(code);
    let direction = symbols.iter().find(|s| s.name == "Direction" && s.kind == SymbolKind::Enum);
    assert!(direction.is_some(), "QML enum should be extracted");
}

#[test]
fn test_extract_qml_alias_property() {
    let code = r#"
        import QtQuick 2.15
        Rectangle {
            property alias contentWidth: content.width
            property int normalProp: 42
        }
    "#;
    let symbols = extract_symbols(code);
    let alias_prop = symbols.iter().find(|s| s.name == "contentWidth" && s.kind == SymbolKind::Property);
    assert!(alias_prop.is_some(), "alias property should be extracted");
    // Check that signature includes "alias" keyword
    assert!(alias_prop.unwrap().signature.as_ref().unwrap().contains("alias"),
        "Alias property signature should contain 'alias'");
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p julie-extractors test_extract_qml_id_binding -- --nocapture 2>&1 | tail -20
cargo test -p julie-extractors test_extract_qml_enum -- --nocapture 2>&1 | tail -20
```

**Step 3: Implement id: binding extraction**

In `traverse_node`, the `id:` binding is likely a `ui_binding` or `ui_script_binding` node in tree-sitter-qmljs. Add a match arm:

```rust
// QML id bindings (id: myId)
"ui_binding" | "ui_script_binding" => {
    // Check if this is an id: binding
    if let Some(name_node) = node.child_by_field_name("name") {
        let binding_name = self.base.get_node_text(&name_node);
        if binding_name == "id" {
            // Extract the value as the symbol name
            if let Some(value_node) = node.child_by_field_name("value")
                .or_else(|| node.child_by_field_name("binding")) {
                let id_value = self.base.get_node_text(&value_node);
                let options = SymbolOptions {
                    parent_id: parent_id.clone(),
                    signature: Some(format!("id: {}", id_value)),
                    ..Default::default()
                };
                let symbol = self.base.create_symbol(
                    &node, id_value, SymbolKind::Property, options);
                self.symbols.push(symbol);
            }
        }
    }
}
```

**Note:** The exact node kinds and field names need verification against the tree-sitter-qmljs grammar. Debug by printing `node.kind()` and child kinds for a QML file with `id:` declarations.

**Step 4: Implement enum extraction**

QML enums use JavaScript-like syntax. Check if tree-sitter-qmljs produces `enum_declaration` nodes. If so, add:

```rust
"enum_declaration" => {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = self.base.get_node_text(&name_node);
        let options = SymbolOptions {
            parent_id: parent_id.clone(),
            ..Default::default()
        };
        let symbol = self.base.create_symbol(&node, name, SymbolKind::Enum, options);
        self.symbols.push(symbol.clone());
        current_symbol = Some(symbol);
    }
}
```

**Step 5: Implement alias property metadata**

The existing `ui_property` handler at line 73 already extracts properties. Enhance it to include `alias` in the signature when the property declaration contains the `alias` keyword:

```rust
"ui_property" => {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = self.base.get_node_text(&name_node);
        let full_text = self.base.get_node_text(&node);
        let signature = Some(full_text);
        let options = SymbolOptions {
            parent_id: parent_id.clone(),
            signature,
            ..Default::default()
        };
        let symbol = self.base.create_symbol(&node, name, SymbolKind::Property, options);
        self.symbols.push(symbol);
    }
}
```

**Step 6: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- qml 2>&1 | tail -10
git add crates/julie-extractors/src/qml/ crates/julie-extractors/src/tests/qml/
git commit -m "feat(qml): extract id: bindings, enums, and alias property signatures"
```

---

### Task 6: HTML — Fix comment naming + fallback noise filter

**Problem 1:** All HTML comments get the literal name `"comment"` (elements.rs:208), making them indistinguishable.
**Problem 2:** The fallback regex extractor (fallback.rs:78-156) doesn't call `should_extract_element()`, so it extracts every element including generic `<div>` and `<span>`.

**Note:** Comments are currently skipped entirely (`mod.rs:108` returns `None` for `"comment"` nodes). The `extract_comment` function in elements.rs is dead code. The fix should either: (a) remove the dead `extract_comment` function, or (b) enable it with meaningful names.

**Files:**
- Modify: `crates/julie-extractors/src/html/elements.rs:175-218`
- Modify: `crates/julie-extractors/src/html/fallback.rs:78-156`
- Modify: `crates/julie-extractors/src/html/mod.rs:108` (if enabling comments)
- Test: `crates/julie-extractors/src/tests/html/`

**Step 1: Write failing test for fallback noise filter**

```rust
#[test]
fn test_fallback_extractor_filters_noise() {
    // Create HTML that would trigger fallback (or test the function directly)
    // The key assertion: generic <div>, <span>, <p> without id/name should not be extracted
    let code = r#"<div><span>text</span><nav class="main">menu</nav><p>paragraph</p></div>"#;
    // After fallback extraction, only semantic elements should appear
    // nav should be extracted, div/span/p should NOT (unless they have id/name)
}
```

**Step 2: Fix the fallback extractor**

In `fallback.rs`, after extracting `tag_name` and `attributes`, add the `should_extract_element` filter:

```rust
// After: let attributes = AttributeHandler::parse_attributes_from_text(attributes_text);
// Add:
if !super::elements::ElementExtractor::should_extract_element_public(&tag_name, &attributes) {
    continue;
}
```

**Note:** `should_extract_element` is currently a private method on `ElementExtractor`. It needs to be made `pub(super)` or extracted as a standalone function accessible from `fallback.rs`.

**Step 3: Handle the comment naming**

Two options:
- **Option A (recommended):** Remove the dead `extract_comment` function entirely since comments are already skipped in mod.rs. HTML comments serve as doc_comments on adjacent elements, which already works.
- **Option B:** Enable comment extraction with truncated content as the name (e.g., first 50 chars of comment text).

Go with Option A unless there's a specific reason to extract comments.

**Step 4: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- html 2>&1 | tail -10
git add crates/julie-extractors/src/html/
git commit -m "fix(html): apply noise filter to fallback extractor, remove dead comment code"
```

---

### Task 7: Dart — typedef → SymbolKind::Type + tighten ERROR recovery

**Problem 1:** `typedef StringCallback = void Function(String)` uses `SymbolKind::Class` (types.rs:213). Should be `SymbolKind::Type`.
**Problem 2:** ERROR node recovery extracts ANY `identifier` as `EnumMember` (mod.rs:325-392), which is noisy for non-enum ERROR contexts.

**Files:**
- Modify: `crates/julie-extractors/src/dart/types.rs:213`
- Modify: `crates/julie-extractors/src/dart/mod.rs:325-392` (recover_from_node_recursive)
- Test: `crates/julie-extractors/src/tests/dart/mod.rs`

**Step 1: Write failing test for typedef kind**

```rust
#[test]
fn test_typedef_uses_type_kind() {
    let code = r#"
        typedef StringCallback = void Function(String);
        typedef NumberProcessor<T extends num> = T Function(T);
    "#;
    let symbols = extract_symbols(code);
    let callback = symbols.iter().find(|s| s.name == "StringCallback").unwrap();
    assert_eq!(callback.kind, SymbolKind::Type, "typedef should be SymbolKind::Type, not Class");
    let processor = symbols.iter().find(|s| s.name == "NumberProcessor").unwrap();
    assert_eq!(processor.kind, SymbolKind::Type);
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p julie-extractors test_typedef_uses_type_kind -- --nocapture 2>&1 | tail -20
```

**Step 3: Fix typedef SymbolKind**

In `types.rs:213`, change `SymbolKind::Class` to `SymbolKind::Type`:

```rust
// CURRENT:
let mut symbol = base.create_symbol(node, name, SymbolKind::Class, ...);
// FIX:
let mut symbol = base.create_symbol(node, name, SymbolKind::Type, ...);
```

**Step 4: Tighten ERROR recovery**

In `mod.rs`, the `recover_from_node_recursive` function at lines 325-392 walks ERROR nodes. Currently it extracts `member_access` as EnumMember and `const_object_expression` as Constructor. This is appropriate when context confirms we're in an enum ERROR block.

The issue is the fallback path — any lowercase `identifier` in an ERROR block could be extracted as EnumMember. Check if there's such a fallback, and if so, restrict it to identifiers that are uppercase-starting (enum-like convention in Dart):

```rust
// Only extract identifiers that look like enum values (PascalCase or UPPER_CASE)
let first_char = name.chars().next().unwrap_or('a');
if first_char.is_uppercase() {
    // ... extract as EnumMember
}
```

**Step 5: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- dart 2>&1 | tail -10
git add crates/julie-extractors/src/dart/
git commit -m "fix(dart): typedef uses SymbolKind::Type, tighten ERROR recovery"
```

---

### Task 8: SQL — Split error_handling.rs + remove hardcoded skip list

**Problem 1:** `error_handling.rs` at 503 lines exceeds the 500-line limit. It contains 9 extraction functions that belong in their domain modules.
**Problem 2:** `views.rs:204-217` has a hardcoded skip list of table alias names (`"u"`, `"ae"`, `"users"`, etc.) that is test-data-specific.

**Files:**
- Modify: `crates/julie-extractors/src/sql/error_handling.rs` (keep only dispatcher)
- Modify: `crates/julie-extractors/src/sql/routines.rs` (receive procedure + function error extractors)
- Modify: `crates/julie-extractors/src/sql/schemas.rs` (receive schema + domain + type error extractors)
- Modify: `crates/julie-extractors/src/sql/views.rs:204-217` (remove skip list)
- Modify: `crates/julie-extractors/src/sql/constraints.rs` (receive constraint error extractor)
- Test: `crates/julie-extractors/src/tests/sql/`

**Step 1: Run existing tests to establish baseline**

```bash
cargo test -p julie-extractors --lib -- sql 2>&1 | tail -10
```

**Step 2: Move domain functions to their modules**

Move each error extraction function to its corresponding domain module:

| Function | From | To |
|----------|------|-----|
| `extract_procedures_from_error` (34-76) | error_handling.rs | routines.rs |
| `extract_functions_from_error` (79-154) | error_handling.rs | routines.rs |
| `extract_schemas_from_error` (157-188) | error_handling.rs | schemas.rs |
| `extract_views_from_error` (191-222) | error_handling.rs | views.rs |
| `extract_triggers_from_error` (225-270) | error_handling.rs | views.rs (or new triggers.rs) |
| `extract_constraints_from_error` (273-367) | error_handling.rs | constraints.rs |
| `extract_domains_from_error` (370-421) | error_handling.rs | schemas.rs |
| `extract_types_from_error` (424-465) | error_handling.rs | schemas.rs |
| `extract_aggregates_from_error` (468-503) | error_handling.rs | routines.rs |

Keep `extract_multiple_from_error_node` (13-31) in `error_handling.rs` as the dispatcher, updating it to call the moved functions via their new module paths.

**Important:** Check that receiving modules won't exceed 500 lines after the additions. If any would, create a new sub-module instead.

**Step 3: Remove hardcoded skip list**

In `views.rs:204-217`, replace the hardcoded list with a generic heuristic:

```rust
// CURRENT:
if ["u", "ae", "users", "analytics_events", "id", "username", "email"].contains(&alias_name) {
    continue;
}

// FIX: Remove the hardcoded skip list. Instead, use a reasonable heuristic:
// Skip single-character aliases (common table abbreviations like "u", "t", "a")
if alias_name.len() <= 1 {
    continue;
}
```

Or remove the skip entirely if the aliases are legitimately part of view definitions.

**Step 4: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- sql 2>&1 | tail -10
git add crates/julie-extractors/src/sql/
git commit -m "refactor(sql): split error_handling.rs into domain modules, remove hardcoded skip list"
```

---

### Task 9: PowerShell — LazyLock regex in imports.rs

**Problem:** `imports.rs` compiles 3+ regex patterns inline on every function call (lines 63, 73, 83). The `Import-Module` regex is duplicated between `extract_import_command` (line 63) and `extract_import_module_name` (line 180).

**Files:**
- Modify: `crates/julie-extractors/src/powershell/imports.rs`
- Test: `crates/julie-extractors/src/tests/powershell/mod.rs`

**Step 1: Run existing tests**

```bash
cargo test -p julie-extractors --lib -- powershell 2>&1 | tail -10
```

**Step 2: Extract regex to static LazyLock**

Add at the top of `imports.rs`:

```rust
use std::sync::LazyLock;

static IMPORT_MODULE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"Import-Module\s+(?:-Name\s+["']?([^"'\s]+)["']?|([A-Za-z0-9.-]+))"#).unwrap()
});

static USING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"using\s+(?:namespace|module)\s+([A-Za-z0-9.-_]+)").unwrap()
});

static EXPORT_MODULE_MEMBER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Export-ModuleMember\s+-(\w+)").unwrap()
});
```

**Step 3: Replace inline regex with static references**

Replace all `Regex::new(...).unwrap()` calls with references to the statics:
- Line 63: `Regex::new(...)` → `IMPORT_MODULE_RE.captures(...)`
- Line 73: `Regex::new(...)` → `USING_RE.captures(...)`
- Line 83: `Regex::new(...)` → `EXPORT_MODULE_MEMBER_RE.captures(...)`
- Lines 180-182: Delete `extract_import_module_name` or simplify it to use `IMPORT_MODULE_RE`
- Lines 190-195: Delete `extract_using_name` or simplify to use `USING_RE`

**Step 4: Check if `extract_import_module_name` and `extract_using_name` are still needed**

These helper functions (lines 178-195) duplicate the regex from `extract_import_command`. If they're only called from one place, inline them. If called from multiple places, keep them but use the static regex.

**Step 5: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- powershell 2>&1 | tail -10
git add crates/julie-extractors/src/powershell/imports.rs
git commit -m "perf(powershell): replace inline Regex::new with LazyLock statics in imports.rs"
```

---

### Task 10: Bash — Remove empty relationship stubs + add shebang detection

**Problem 1:** `relationships.rs:72-93` has two empty stub methods (`extract_command_substitution_relationships`, `extract_file_relationships`) that are dispatched but do nothing.
**Problem 2:** Shebang lines (`#!/bin/bash`) are not detected, losing important metadata about the script's interpreter.

**Files:**
- Modify: `crates/julie-extractors/src/bash/relationships.rs:72-93`
- Modify: `crates/julie-extractors/src/bash/mod.rs` (remove dispatch + add shebang)
- Test: `crates/julie-extractors/src/tests/bash/`

**Step 1: Write failing test for shebang**

```rust
#[test]
fn test_extract_shebang() {
    let code = r#"#!/bin/bash
set -euo pipefail

function main() {
    echo "Hello"
}
"#;
    let symbols = extract_symbols(code);
    // Shebang should be extracted as metadata (e.g., Variable with signature "#!/bin/bash")
    let shebang = symbols.iter().find(|s| s.name.contains("bash") || s.signature.as_ref().map_or(false, |s| s.contains("#!/bin/bash")));
    assert!(shebang.is_some(), "Shebang line should be extracted");
}
```

**Step 2: Remove empty stubs**

In `relationships.rs`, delete:
- Lines 72-81: `extract_command_substitution_relationships` (empty stub)
- Lines 83-93: `extract_file_relationships` (empty stub)

In `mod.rs`, update `walk_tree_for_relationships` to remove the dispatch arms that call these stubs:

```rust
// REMOVE these cases:
"command_substitution" => {
    self.extract_command_substitution_relationships(node, symbols, relationships);
}
"file_redirect" => {
    self.extract_file_relationships(node, symbols, relationships);
}
```

**Step 3: Add shebang detection**

In `mod.rs`, in the `extract_symbols` method (or `extract_symbol_from_node`), detect the shebang line. Tree-sitter-bash likely represents it as a `comment` node at position (0,0) with text starting with `#!`. Alternatively, check the raw content for a shebang:

```rust
// In extract_symbols, before or after the tree walk:
// Check first line for shebang
if let Some(first_line) = self.base.content.lines().next() {
    if first_line.starts_with("#!") {
        let interpreter = first_line.trim_start_matches("#!").trim();
        let name = interpreter.rsplit('/').next().unwrap_or(interpreter);
        // Create a symbol for the shebang
        // Use the root node for position
        let root = tree.root_node();
        let symbol = self.base.create_symbol(
            &root,
            name.to_string(),
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(first_line.to_string()),
                ..Default::default()
            },
        );
        symbols.push(symbol);
    }
}
```

**Step 4: Run tests and commit**

```bash
cargo test -p julie-extractors --lib -- bash 2>&1 | tail -10
git add crates/julie-extractors/src/bash/
git commit -m "fix(bash): remove empty relationship stubs, add shebang detection"
```

---

## Post-Implementation

After all 10 tasks, update `docs/EXTRACTOR_AUDIT.md`:
1. Add "Round 4" summary section
2. Update ratings for all 10 extractors (B → B+)
3. Update "Remaining Issues" columns to reflect fixes
4. Correct audit inaccuracies discovered during implementation:
   - Zig: "Parameters as symbols (noisy)" is WRONG — parameters are correctly in signature only, not individual symbols
   - Zig: "unknown" sentinel in types.rs:163 — already cleaned up (uses `String::new()`)
   - Lua: "unknown" sentinels — already cleaned up (uses `String::new()`)
   - Lua: "No visibility model" is WRONG — local→Private, global→Public already implemented

Final verification:
```bash
cargo test -p julie-extractors --lib 2>&1 | tail -5
```

Expected: 1199+ tests pass, 0 failures (likely 1210+ with new tests).
