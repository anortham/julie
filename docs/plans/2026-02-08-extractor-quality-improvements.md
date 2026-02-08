# Extractor Quality Improvements: Tier 1 Features + Tier 2 Noise Fixes

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fill critical feature gaps in 4 extractors (Vue, Rust, Dart, TOML) and eliminate search-polluting noise from 4 extractors (CSS, Bash, Regex, Razor).

**Architecture:** Each task modifies a single extractor in isolation. Tasks are independent — no cross-extractor dependencies. All follow TDD: write failing test, implement, verify. Each extractor lives in `crates/julie-extractors/src/{language}/` with tests in `crates/julie-extractors/src/tests/{language}/`.

**Tech Stack:** Rust, tree-sitter parsers, regex for Vue SFC parsing. Tests use `tree_sitter::Parser` + extractor-specific `extract_symbols()`.

**Test command:** `cargo test -p julie-extractors --lib -- {test_name} --exact`

---

## Task 1: TOML Key-Value Pair Extraction

**Why:** Currently only table headers (`[package]`) are extracted. Key-value pairs like `name = "julie"` are invisible to search. The JSON extractor already does this, so TOML should too for consistency.

**Files:**
- Modify: `crates/julie-extractors/src/toml/mod.rs`
- Test: `crates/julie-extractors/src/tests/toml/mod.rs`

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/toml/mod.rs`:

```rust
#[test]
fn test_extract_toml_key_value_pairs() {
    let toml = r#"
[package]
name = "julie"
version = "1.0.0"
edition = "2021"

[dependencies]
tokio = "1.0"
serde = { version = "1.0", features = ["derive"] }
"#;

    let symbols = extract_symbols(toml);

    // Tables should still be extracted
    let tables: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Module).collect();
    assert!(tables.len() >= 2, "Should extract table headers");

    // Key-value pairs should now be extracted as Property
    let kvs: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Property).collect();
    assert!(kvs.iter().any(|s| s.name == "name"), "Should extract 'name' key");
    assert!(kvs.iter().any(|s| s.name == "version"), "Should extract 'version' key");
    assert!(kvs.iter().any(|s| s.name == "edition"), "Should extract 'edition' key");
    assert!(kvs.iter().any(|s| s.name == "tokio"), "Should extract 'tokio' dependency key");
    assert!(kvs.iter().any(|s| s.name == "serde"), "Should extract 'serde' dependency key");

    // Check that key-value pairs have parent_id linking to their table
    let name_kv = kvs.iter().find(|s| s.name == "name").unwrap();
    let package_table = tables.iter().find(|s| s.name == "package").unwrap();
    assert_eq!(name_kv.parent_id.as_deref(), Some(package_table.id.as_str()),
        "Key should be child of its table");

    // Check signature includes the value
    assert!(name_kv.signature.as_deref().unwrap_or("").contains("julie"),
        "Signature should include the value");
}

#[test]
fn test_extract_toml_top_level_key_value_pairs() {
    let toml = r#"
title = "My Config"
debug = true
count = 42
"#;

    let symbols = extract_symbols(toml);

    // Top-level keys should be extracted with no parent_id
    let title = symbols.iter().find(|s| s.name == "title").expect("Should extract 'title'");
    assert_eq!(title.kind, SymbolKind::Property);
    assert!(title.parent_id.is_none(), "Top-level key should have no parent");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_extract_toml_key_value_pairs --exact`
Expected: FAIL — key-value pairs not extracted

**Step 3: Write minimal implementation**

In `crates/julie-extractors/src/toml/mod.rs`, modify `extract_symbol_from_node` to handle `pair` nodes:

```rust
fn extract_symbol_from_node(
    &mut self,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    match node.kind() {
        "table" => self.extract_table(node, parent_id, false),
        "table_array_element" => self.extract_table(node, parent_id, true),
        "pair" => self.extract_pair(node, parent_id),
        _ => None,
    }
}
```

Add `extract_pair` method:

```rust
/// Extract a key-value pair as a Property symbol
fn extract_pair(
    &mut self,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    use crate::base::SymbolOptions;

    // First child is the key, second (after "=") is the value
    let key_node = node.child(0)?;
    let key_name = self.base.get_node_text(&key_node);
    let key_name = key_name.trim_matches('"').trim_matches('\'');

    // Get the value for the signature
    let value_text = node.child_by_field_name("value")
        .map(|v| self.base.get_node_text(&v))
        .unwrap_or_default();

    // Truncate long values (inline tables, arrays)
    let display_value = if value_text.len() > 80 {
        format!("{}...", &value_text[..77])
    } else {
        value_text
    };

    let signature = format!("{} = {}", key_name, display_value);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: None,
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: None,
        ..Default::default()
    };

    Some(self.base.create_symbol(
        &node,
        key_name.to_string(),
        SymbolKind::Property,
        options,
    ))
}
```

**Important:** The tree-sitter TOML grammar (`tree_sitter_toml_ng`) uses `pair` nodes for key-value pairs. The key is the first child and the value field is named `"value"`. Verify this by examining the AST if tests fail — the field name might differ. If there's no named field, iterate children and pick the one after the `"="` node.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_extract_toml_key_value --exact`
Expected: PASS

**Step 5: Run full TOML test suite**

Run: `cargo test -p julie-extractors --lib -- toml_extractor_tests`
Expected: All existing tests still PASS (tables should be unaffected)

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/toml/mod.rs crates/julie-extractors/src/tests/toml/mod.rs
git commit -m "feat(toml): extract key-value pairs as Property symbols

TOML extractor now extracts key-value pairs in addition to table headers.
Keys become SymbolKind::Property with parent_id linking to their table.
Signatures include the value for search context."
```

---

## Task 2: Rust Struct Field and Enum Variant Extraction

**Why:** Can't search for or navigate to struct fields or enum variants — a core use case for a Rust codebase like Julie itself. The relationship extractor already walks `field_declaration_list` and `enum_variant_list` for type references, but individual fields/variants aren't extracted as symbols.

**Files:**
- Modify: `crates/julie-extractors/src/rust/types.rs`
- Modify: `crates/julie-extractors/src/rust/mod.rs` (the `walk_tree`/`extract_symbol` dispatch)
- Test: `crates/julie-extractors/src/tests/rust/types.rs` (or `mod.rs` if types.rs is small)

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/rust/types.rs` (or create a new section in `mod.rs`):

```rust
#[test]
fn test_extract_struct_fields() {
    let rust_code = r#"
pub struct User {
    pub id: u64,
    name: String,
    email: Option<String>,
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(rust_code, None).unwrap();
    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        rust_code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Struct should exist
    let user_struct = symbols.iter().find(|s| s.name == "User").expect("User struct not found");
    assert_eq!(user_struct.kind, SymbolKind::Struct);

    // Fields should be extracted as children
    let fields: Vec<_> = symbols.iter()
        .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&user_struct.id))
        .collect();

    assert_eq!(fields.len(), 3, "Should extract all 3 fields");
    assert!(fields.iter().any(|f| f.name == "id"), "Should extract 'id' field");
    assert!(fields.iter().any(|f| f.name == "name"), "Should extract 'name' field");
    assert!(fields.iter().any(|f| f.name == "email"), "Should extract 'email' field");

    // Check field signatures include type
    let id_field = fields.iter().find(|f| f.name == "id").unwrap();
    assert!(id_field.signature.as_deref().unwrap_or("").contains("u64"),
        "Field signature should include type");
}

#[test]
fn test_extract_enum_variants() {
    let rust_code = r#"
pub enum Color {
    Red,
    Green,
    Blue(u8, u8, u8),
    Custom { r: u8, g: u8, b: u8 },
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(rust_code, None).unwrap();
    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        rust_code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    let color_enum = symbols.iter().find(|s| s.name == "Color").expect("Color enum not found");
    assert_eq!(color_enum.kind, SymbolKind::Enum);

    let variants: Vec<_> = symbols.iter()
        .filter(|s| s.kind == SymbolKind::EnumMember && s.parent_id.as_deref() == Some(&color_enum.id))
        .collect();

    assert_eq!(variants.len(), 4, "Should extract all 4 variants");
    assert!(variants.iter().any(|v| v.name == "Red"));
    assert!(variants.iter().any(|v| v.name == "Green"));
    assert!(variants.iter().any(|v| v.name == "Blue"));
    assert!(variants.iter().any(|v| v.name == "Custom"));

    // Tuple variant should have signature with types
    let blue = variants.iter().find(|v| v.name == "Blue").unwrap();
    assert!(blue.signature.as_deref().unwrap_or("").contains("u8"),
        "Tuple variant should show types in signature");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_extract_struct_fields --exact`
Expected: FAIL — fields not extracted

**Step 3: Write minimal implementation**

The approach: After `extract_struct` creates the struct symbol, the `walk_tree` method already recurses into children. We need to handle `field_declaration` nodes (children of `field_declaration_list`). Similarly for `enum_variant` nodes inside `enum_variant_list`.

Add two new functions to `crates/julie-extractors/src/rust/types.rs`:

```rust
pub(super) fn extract_field(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();

    // Field name is in the "name" field of field_declaration
    let name = node.child_by_field_name("name")
        .map(|n| base.get_node_text(&n))?;

    // Field type
    let field_type = node.child_by_field_name("type")
        .map(|t| base.get_node_text(&t))
        .unwrap_or_default();

    let visibility = extract_visibility(base, node);
    let signature = format!("{}{}: {}", visibility, name, field_type);

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Field,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    ))
}

pub(super) fn extract_enum_variant(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();

    let name = node.child_by_field_name("name")
        .map(|n| base.get_node_text(&n))?;

    // Build variant signature showing tuple/struct fields if present
    let body = node.child_by_field_name("body")
        .map(|b| base.get_node_text(&b))
        .unwrap_or_default();

    let signature = if body.is_empty() {
        name.clone()
    } else {
        format!("{}{}", name, body)
    };

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // Enum variants inherit enum visibility
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    ))
}
```

Then in `crates/julie-extractors/src/rust/mod.rs`, add to the `extract_symbol` match:

```rust
"field_declaration" => types::extract_field(self, node, parent_id),
"enum_variant" => types::extract_enum_variant(self, node, parent_id),
```

**Important:** Verify the tree-sitter Rust grammar node kinds. Struct fields are `field_declaration` inside `field_declaration_list`. Enum variants are `enum_variant` inside `enum_variant_list`. The `name` and `type`/`body` field names may differ — check with a debug print of `node.kind()` and field names if tests fail.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_extract_struct_fields test_extract_enum_variants`
Expected: PASS

**Step 5: Run full Rust test suite**

Run: `cargo test -p julie-extractors --lib -- rust_extractor_tests`
Expected: All existing tests still PASS

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/rust/types.rs crates/julie-extractors/src/rust/mod.rs crates/julie-extractors/src/tests/rust/
git commit -m "feat(rust): extract struct fields and enum variants as symbols

Fields are SymbolKind::Field with type in signature, parented to struct.
Variants are SymbolKind::EnumMember with tuple/struct body in signature.
Enables search and navigation to individual fields and variants."
```

---

## Task 3: CSS Keyframe Percentage Noise Elimination

**Why:** `0%`, `50%`, `100%`, `from`, `to` are extracted as individual `SymbolKind::Variable` symbols. Having a symbol named "50%" is useless for code intelligence and pollutes search results.

**Files:**
- Modify: `crates/julie-extractors/src/css/animations.rs`
- Modify: `crates/julie-extractors/src/css/mod.rs` (if it calls `extract_keyframes`)
- Test: `crates/julie-extractors/src/tests/css/animations.rs`

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/css/animations.rs`:

```rust
#[test]
fn test_keyframe_percentages_not_extracted_as_symbols() {
    let css_code = r#"
@keyframes slideIn {
  0% { transform: translateX(-100%); }
  50% { opacity: 0.5; }
  100% { transform: translateX(0); }
}

@keyframes fadeOut {
  from { opacity: 1; }
  to { opacity: 0; }
}
"#;

    let symbols = extract_symbols(css_code);

    // @keyframes rules should still be extracted
    assert!(symbols.iter().any(|s| s.name == "@keyframes slideIn"),
        "Should extract @keyframes slideIn");
    assert!(symbols.iter().any(|s| s.name == "@keyframes fadeOut"),
        "Should extract @keyframes fadeOut");

    // Individual keyframe percentages should NOT be extracted
    assert!(!symbols.iter().any(|s| s.name == "0%"), "Should not extract '0%' as symbol");
    assert!(!symbols.iter().any(|s| s.name == "50%"), "Should not extract '50%' as symbol");
    assert!(!symbols.iter().any(|s| s.name == "100%"), "Should not extract '100%' as symbol");
    assert!(!symbols.iter().any(|s| s.name == "from"), "Should not extract 'from' as symbol");
    assert!(!symbols.iter().any(|s| s.name == "to"), "Should not extract 'to' as symbol");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_keyframe_percentages_not_extracted_as_symbols --exact`
Expected: FAIL — percentages are currently extracted

**Step 3: Write minimal implementation**

In `crates/julie-extractors/src/css/animations.rs`, make `extract_keyframes` a no-op (or delete the method body and have it do nothing):

```rust
/// Individual keyframe blocks (0%, 50%, from, to) are not meaningful symbols.
/// The @keyframes rule itself is sufficient for code intelligence.
pub(super) fn extract_keyframes(
    _base: &mut BaseExtractor,
    _node: Node,
    _symbols: &mut Vec<Symbol>,
    _parent_id: Option<&str>,
) {
    // Intentionally empty — individual keyframe percentages are noise.
    // The @keyframes rule (extracted by extract_keyframes_rule) is sufficient.
}
```

Also check `crates/julie-extractors/src/css/mod.rs` for where `extract_keyframes` is called — it should continue to call `extract_keyframes_rule` (which extracts the `@keyframes fadeIn` rule) but the individual blocks inside are now skipped.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_keyframe_percentages_not_extracted_as_symbols --exact`
Expected: PASS

**Step 5: Run full CSS test suite and check for broken assertions**

Run: `cargo test -p julie-extractors --lib -- css`
Expected: Some existing tests MAY fail if they assert on the presence of keyframe percentage symbols. Update those assertions to match the new behavior (percentages no longer extracted). The `@keyframes` rule itself should still be present.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/css/animations.rs crates/julie-extractors/src/tests/css/
git commit -m "fix(css): stop extracting keyframe percentages as symbols

Individual keyframe blocks (0%, 50%, from, to) are noise in search results.
The @keyframes rule itself provides sufficient code intelligence.
Reduces symbol count and improves search quality for CSS files."
```

---

## Task 4: Bash Control Flow Noise Elimination

**Why:** `for_statement`, `while_statement`, `if_statement` are matched in `extract_symbol_from_node` but return `None` — so they're already not extracted. However, the audit says they ARE extracted as `SymbolKind::Method` with synthetic names. Let me re-read... Actually, looking at the current code (line 91), these return `None`. The audit might be referring to a previous state that was already fixed, OR there's another code path. The implementer should verify whether control flow blocks actually appear in extracted symbols. If they don't, just add a test confirming they're not extracted and move on.

**Files:**
- Modify: `crates/julie-extractors/src/bash/mod.rs` (if needed)
- Test: `crates/julie-extractors/src/tests/bash/mod.rs`

**Step 1: Write the test**

```rust
#[test]
fn test_control_flow_not_extracted_as_symbols() {
    let code = r#"
#!/bin/bash

for item in "$@"; do
    echo "$item"
done

while true; do
    sleep 1
done

if [ -f "$1" ]; then
    cat "$1"
fi

case "$1" in
    start) echo "starting" ;;
    stop) echo "stopping" ;;
esac
"#;

    let symbols = extract_symbols(code);

    // Control flow should NOT produce symbols
    assert!(!symbols.iter().any(|s| s.name.contains("for")),
        "for loops should not be symbols");
    assert!(!symbols.iter().any(|s| s.name.contains("while")),
        "while loops should not be symbols");
    assert!(!symbols.iter().any(|s| s.name.contains("if")),
        "if statements should not be symbols");
    assert!(!symbols.iter().any(|s| s.name.contains("case")),
        "case statements should not be symbols");
    assert!(!symbols.iter().any(|s| s.kind == SymbolKind::Method),
        "No Method symbols should be produced for control flow");
}
```

**Step 2: Run test**

Run: `cargo test -p julie-extractors --lib -- test_control_flow_not_extracted_as_symbols --exact`

If it PASSES: control flow is already not extracted. Just commit the test and move on.
If it FAILS: find the code path that produces these symbols and remove it.

**Step 3: Fix if needed**

Look for any other match arm or function that handles `for_statement`/`while_statement`/`if_statement`/`case_statement` and returns a symbol. The `walk_tree_for_symbols` method recurses into all children, so if there's no match for these node kinds, they won't produce symbols. Check if `extract_command` is accidentally matching these via `command` node kind.

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/bash/ crates/julie-extractors/src/tests/bash/
git commit -m "test(bash): confirm control flow blocks are not extracted as symbols

Adds test verifying for/while/if/case don't produce symbols.
Control flow blocks are noise for code intelligence."
```

---

## Task 5: Dart Import Extraction

**Why:** The Dart extractor doc comment claims "Imports and library dependencies" support but zero import extraction is implemented. Imports are fundamental for understanding code structure and cross-file relationships.

**Files:**
- Create: `crates/julie-extractors/src/dart/imports.rs`
- Modify: `crates/julie-extractors/src/dart/mod.rs` (add `mod imports`, add to `visit_node`)
- Test: `crates/julie-extractors/src/tests/dart/mod.rs`

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/dart/mod.rs`:

```rust
#[test]
fn test_extract_dart_imports() {
    let dart_code = r#"
import 'dart:async';
import 'package:flutter/material.dart';
import 'package:my_app/models/user.dart' as user_model;
import 'package:my_app/utils.dart' show formatDate, parseDate;
import 'package:my_app/legacy.dart' hide deprecatedFunction;

export 'package:my_app/models/user.dart';

library my_app;

part 'src/models.dart';
part of 'my_app.dart';
"#;

    let symbols = extract_symbols(dart_code);

    let imports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();

    assert!(imports.iter().any(|s| s.name.contains("dart:async")),
        "Should extract dart:async import");
    assert!(imports.iter().any(|s| s.name.contains("flutter/material.dart")),
        "Should extract flutter/material.dart import");
    assert!(imports.iter().any(|s| s.name.contains("user.dart")),
        "Should extract user.dart import");

    // Aliased import should have alias in signature or name
    let aliased = imports.iter().find(|s| s.name.contains("user.dart") && s.signature.as_deref().unwrap_or("").contains("as"));
    assert!(aliased.is_some(), "Should handle aliased imports");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_extract_dart_imports --exact`
Expected: FAIL — no imports extracted

**Step 3: Write minimal implementation**

Create `crates/julie-extractors/src/dart/imports.rs`:

```rust
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract import/export/library/part declarations from Dart code
pub(super) fn extract_import(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Get the full text and extract the URI
    let text = base.get_node_text(&node);

    // Find the string literal (URI) in the import
    let uri = find_child_string_literal(base, node)?;

    // Extract any modifiers (as, show, hide)
    let signature = text.trim().trim_end_matches(';').to_string();

    Some(base.create_symbol(
        &node,
        uri,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            ..Default::default()
        },
    ))
}

fn find_child_string_literal(base: &BaseExtractor, node: Node) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string_literal" {
            let text = base.get_node_text(&child);
            return Some(text.trim_matches('\'').trim_matches('"').to_string());
        }
        // Recurse one level for configured_uri etc.
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if grandchild.kind() == "string_literal" {
                let text = base.get_node_text(&grandchild);
                return Some(text.trim_matches('\'').trim_matches('"').to_string());
            }
        }
    }
    None
}
```

In `crates/julie-extractors/src/dart/mod.rs`:
1. Add `mod imports;` at the top with the other module declarations
2. In `visit_node`, add match arms for import-related node kinds:

```rust
"import_or_export" | "import_specification" | "library_import" => {
    if let Some(sym) = imports::extract_import(&mut self.base, node, parent_id.as_deref()) {
        symbols.push(sym);
    }
}
```

**Important:** The Dart tree-sitter grammar node kinds may differ. Common ones are `import_or_export`, `library_import`, `part_directive`, `part_of_directive`, `library_directive`, `export_directive`. The implementer MUST check the actual node kinds by printing `node.kind()` for Dart import statements. Use `cargo test` output or a small debug script to verify.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_extract_dart_imports --exact`
Expected: PASS

**Step 5: Run full Dart test suite**

Run: `cargo test -p julie-extractors --lib -- dart`
Expected: All existing tests still PASS

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/dart/imports.rs crates/julie-extractors/src/dart/mod.rs crates/julie-extractors/src/tests/dart/
git commit -m "feat(dart): add import/export/library/part extraction

Dart extractor now extracts import, export, library, and part directives
as SymbolKind::Import. Handles aliased imports (as), show/hide filters.
Fills a gap noted in the extractor audit."
```

---

## Task 6: Vue Composition API and `<script setup>` Support

**Why:** The Vue extractor only handles Options API (`data()`, `methods:`, `computed:`). Vue 3's recommended pattern — Composition API with `<script setup>` — is completely unhandled. Modern Vue codebases are essentially invisible.

**Files:**
- Modify: `crates/julie-extractors/src/vue/script.rs` (add Composition API extraction)
- Modify: `crates/julie-extractors/src/vue/helpers.rs` (add new regex patterns)
- Modify: `crates/julie-extractors/src/vue/parsing.rs` (detect `setup` attribute)
- Modify: `crates/julie-extractors/src/vue/mod.rs` (handle `<script setup>` sections)
- Test: `crates/julie-extractors/src/tests/vue/mod.rs`

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/vue/mod.rs`:

```rust
#[test]
fn test_extract_vue3_script_setup() {
    let vue_code = r#"
<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { User } from './types'

const count = ref(0)
const name = ref('World')
const doubled = computed(() => count.value * 2)

function increment() {
  count.value++
}

const reset = () => {
  count.value = 0
}

onMounted(() => {
  console.log('mounted')
})

defineProps<{
  title: string
  items: string[]
}>()

const emit = defineEmits(['update', 'delete'])
</script>

<template>
  <div>{{ count }}</div>
</template>
"#;

    let symbols = extract_symbols(vue_code);

    // Should extract ref/computed variables
    assert!(symbols.iter().any(|s| s.name == "count"), "Should extract 'count' ref");
    assert!(symbols.iter().any(|s| s.name == "name"), "Should extract 'name' ref");
    assert!(symbols.iter().any(|s| s.name == "doubled"), "Should extract 'doubled' computed");

    // Should extract functions
    assert!(symbols.iter().any(|s| s.name == "increment"), "Should extract 'increment' function");
    assert!(symbols.iter().any(|s| s.name == "reset"), "Should extract 'reset' arrow function");
}

#[test]
fn test_extract_vue3_composition_api_setup_function() {
    let vue_code = r#"
<script>
import { ref, computed } from 'vue'

export default {
  name: 'MyComponent',
  setup() {
    const count = ref(0)
    function increment() {
      count.value++
    }
    return { count, increment }
  }
}
</script>
"#;

    let symbols = extract_symbols(vue_code);

    // Component name should be found
    assert!(symbols.iter().any(|s| s.name == "MyComponent"),
        "Should extract component name");

    // setup function should be extracted
    assert!(symbols.iter().any(|s| s.name == "setup"),
        "Should extract setup function");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_extract_vue3_script_setup --exact`
Expected: FAIL

**Step 3: Implement `<script setup>` detection**

The approach: For `<script setup>`, we parse the section content with tree-sitter JavaScript/TypeScript (already done for identifiers) and extract symbols from the resulting AST, rather than using regex line-by-line matching.

In `crates/julie-extractors/src/vue/parsing.rs`, the `VueSection` struct already has a `lang` field. We need to also detect the `setup` attribute. Modify parsing to capture whether `setup` is present:

Add to `VueSection`:
```rust
pub(crate) is_setup: bool,
```

In `parse_vue_sfc`, after detecting a script section, check if the opening tag contains `setup`:
```rust
let is_setup = section_type == "script" && attrs.contains("setup");
```

Pass `is_setup` through to `VueSectionBuilder` and `VueSection`.

In `crates/julie-extractors/src/vue/script.rs`, add a new function `extract_script_setup_symbols` that:
1. Parses the script content with tree-sitter JS/TS parser (based on `lang`)
2. Walks the AST looking for:
   - `variable_declarator` with `call_expression` where callee is `ref`, `reactive`, `computed`, `defineProps`, `defineEmits`, `defineExpose` → extract as Variable/Property
   - `function_declaration` → extract as Function
   - `lexical_declaration` with arrow function → extract as Function
   - `import_statement` → extract as Import

The existing `extract_script_symbols` (regex-based) continues to handle Options API.

In `crates/julie-extractors/src/vue/mod.rs`, in `extract_section_symbols`, route to the appropriate function:
```rust
if section.section_type == "script" {
    if section.is_setup {
        script::extract_script_setup_symbols(&self.base, section)
    } else {
        script::extract_script_symbols(&self.base, section)
    }
}
```

**Important notes for the implementer:**
- The Vue SFC parser is regex-based (not tree-sitter) for splitting sections. But within `<script setup>`, we can use tree-sitter JS/TS to parse the content.
- The `identifiers.rs` module already does `parse_script_section()` using tree-sitter — reuse that pattern.
- Keep the Options API path working (don't break it).
- Line offsets: symbols extracted from the script section need `section.start_line` added to their line numbers.
- The `create_symbol_manual` helper in `script.rs` handles line offset — reuse it.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_extract_vue3_script_setup test_extract_vue3_composition_api_setup_function`
Expected: PASS

**Step 5: Run full Vue test suite**

Run: `cargo test -p julie-extractors --lib -- vue`
Expected: All existing tests still PASS

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/vue/ crates/julie-extractors/src/tests/vue/
git commit -m "feat(vue): add Composition API and <script setup> support

Vue extractor now handles Vue 3 patterns:
- <script setup> with ref(), computed(), defineProps(), defineEmits()
- Composition API setup() function
- Tree-sitter based parsing for script setup (vs regex for Options API)
Major gap filled for modern Vue codebases."
```

---

## Task 7: Razor Invocation-as-Definition Noise Fix

**Why:** `invocation_expression` nodes like `Html.Raw(...)`, `RenderBody()`, `Component.InvokeAsync(...)` are extracted as `SymbolKind::Function` definitions. These are *usages/references*, not definitions. They create false positives in search results.

**Files:**
- Modify: `crates/julie-extractors/src/razor/mod.rs` (remove `invocation_expression` from `visit_node`)
- Test: `crates/julie-extractors/src/tests/razor/mod.rs`

**Step 1: Write the failing test**

Add to `crates/julie-extractors/src/tests/razor/mod.rs`:

```rust
#[test]
fn test_invocations_not_extracted_as_definitions() {
    let razor_code = r#"
@code {
    private string title = "Hello";

    private void UpdateTitle() {
        title = "Updated";
    }
}

<h1>@title</h1>
<p>@Html.Raw("<b>bold</b>")</p>
<p>@RenderBody()</p>
"#;

    let symbols = extract_symbols(razor_code);

    // Real definitions should be extracted
    assert!(symbols.iter().any(|s| s.name == "title"), "Should extract 'title' field");
    assert!(symbols.iter().any(|s| s.name == "UpdateTitle"), "Should extract 'UpdateTitle' method");

    // Method calls should NOT be extracted as definitions
    assert!(!symbols.iter().any(|s| s.name == "Html" || s.name == "Raw" || s.name == "Html.Raw"),
        "Html.Raw() is a usage, not a definition");
    assert!(!symbols.iter().any(|s| s.name == "RenderBody"),
        "RenderBody() is a usage, not a definition");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib -- test_invocations_not_extracted_as_definitions --exact`
Expected: FAIL (if invocations are still being extracted)

**Step 3: Write minimal implementation**

In `crates/julie-extractors/src/razor/mod.rs`, in the `visit_node` method, change the `invocation_expression` arm to not extract a symbol:

```rust
// Invocation expressions are USAGES, not definitions.
// They are handled by identifier extraction for call tracking.
"invocation_expression" => {}
```

Similarly, consider whether `assignment_expression` and `element_access_expression` should also stop producing symbols (they're in `stubs.rs` as `extract_assignment` and similar). The audit flagged these too. The implementer should check if removing them breaks any tests and update accordingly.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib -- test_invocations_not_extracted_as_definitions --exact`
Expected: PASS

**Step 5: Run full Razor test suite**

Run: `cargo test -p julie-extractors --lib -- razor`
Expected: Some tests MAY fail if they assert on the presence of invocation symbols. Update those assertions.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/razor/ crates/julie-extractors/src/tests/razor/
git commit -m "fix(razor): stop extracting invocations as symbol definitions

Html.Raw(), RenderBody(), Component.InvokeAsync() are usages, not
definitions. They are still tracked via identifier extraction for
call relationship resolution. Reduces noise in search results."
```

---

## Task 8: Regex Noise Reduction

**Why:** The regex extractor extracts every individual literal character, anchor, and quantified expression as separate symbols. A simple regex produces 10+ symbols, flooding the index. We should only extract meaningful named constructs.

**Files:**
- Modify: `crates/julie-extractors/src/regex/patterns.rs`
- Modify: `crates/julie-extractors/src/regex/mod.rs`
- Test: `crates/julie-extractors/src/tests/regex/mod.rs` (find test file)

**Step 1: Investigate current behavior**

Before writing tests, the implementer should:
1. Look at `patterns.rs` and `mod.rs` to understand what's being extracted
2. Run a simple test case and count how many symbols a basic regex produces
3. Identify which symbol extractions are valuable (named groups, character classes) vs noise (individual literals, anchors, quantifiers)

**Step 2: Write the failing test**

```rust
#[test]
fn test_regex_does_not_extract_individual_literals() {
    let regex_code = r#"/^[a-z]+@[a-z]+\.[a-z]{2,}$/i"#;

    let symbols = extract_symbols(regex_code);

    // The overall pattern should be extracted
    assert!(!symbols.is_empty(), "Should extract at least one symbol");

    // Individual characters/anchors should NOT be separate symbols
    assert!(!symbols.iter().any(|s| s.name == "^"), "Should not extract '^' anchor as symbol");
    assert!(!symbols.iter().any(|s| s.name == "$"), "Should not extract '$' anchor as symbol");
    assert!(!symbols.iter().any(|s| s.name == "+"), "Should not extract '+' quantifier as symbol");

    // Named groups SHOULD be extracted
    // (if the regex had named groups, they should appear)
}

#[test]
fn test_regex_extracts_named_groups() {
    let regex_code = r#"/(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/"#;

    let symbols = extract_symbols(regex_code);

    // Named groups should be extracted
    assert!(symbols.iter().any(|s| s.name == "year"), "Should extract named group 'year'");
    assert!(symbols.iter().any(|s| s.name == "month"), "Should extract named group 'month'");
    assert!(symbols.iter().any(|s| s.name == "day"), "Should extract named group 'day'");

    // But individual quantifiers/literals should not
    let non_group_symbols: Vec<_> = symbols.iter()
        .filter(|s| !["year", "month", "day"].contains(&s.name.as_str())
            && !s.name.starts_with('/'))
        .collect();

    // Allow the overall pattern + character classes, but not dozens of atoms
    assert!(non_group_symbols.len() <= 5,
        "Should not have excessive noise symbols (got {})", non_group_symbols.len());
}
```

**Step 3: Implementation approach**

The implementer should:
1. In `patterns.rs`, identify which `extract_*` functions produce noise (likely `extract_literal`, `extract_anchor`, `extract_quantifier`)
2. Either skip these entirely or gate them behind a "verbose" flag
3. Keep extraction of: named groups, character classes (as a single symbol, not individual ranges), the overall pattern
4. The goal is to reduce a simple regex from 10+ symbols down to 1-5 meaningful ones

**Step 4: Run tests and fix broken assertions**

Run: `cargo test -p julie-extractors --lib -- regex`
Fix any broken tests that asserted on noise symbols.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/regex/ crates/julie-extractors/src/tests/regex/
git commit -m "fix(regex): reduce noise by skipping individual literals/anchors/quantifiers

Regex extractor now only extracts meaningful constructs: named groups,
character classes, and the overall pattern. Individual literals, anchors,
and quantifiers are no longer separate symbols. Dramatically reduces
symbol count and improves search quality."
```

---

## Verification

After all tasks, run the full extractor test suite:

```bash
cargo test -p julie-extractors --lib
```

Expected: All tests pass (1126+ tests, with our new additions).

Also run a quick sanity check that the binary still compiles:

```bash
cargo check
```
