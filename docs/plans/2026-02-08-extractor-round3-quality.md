# Extractor Round 3: Quality Improvements

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Raise extractor quality across 9 tasks — feature additions, noise elimination, and code quality improvements across TypeScript, YAML, Python, PHP, Razor, CSS, and cross-cutting LazyLock/dead-code cleanup.

**Architecture:** Each task modifies a single extractor (or cross-cutting concern), follows TDD with failing test first, and targets specific issues documented in `docs/EXTRACTOR_AUDIT.md`. Tasks are independent and can be parallelized.

**Tech Stack:** Rust, tree-sitter, julie-extractors crate

**Test command:** `cargo test -p julie-extractors --lib`

---

## Task 1: TypeScript — Export All Specifiers

**Files:**
- Modify: `crates/julie-extractors/src/typescript/imports_exports.rs`
- Modify: `crates/julie-extractors/src/typescript/mod.rs` (visit_node must handle Vec return)
- Test: `crates/julie-extractors/src/tests/typescript/imports_exports.rs`

**Context:** `extract_export()` at line 66 uses `clause.named_child(0)` to get only the first export specifier. `export { a, b, c }` silently drops `b` and `c`. The function returns `Option<Symbol>` but needs to return `Vec<Symbol>` to support multiple specifiers.

**Step 1: Write failing test**

In `tests/typescript/imports_exports.rs`, add:

```rust
#[test]
fn test_extract_export_multiple_specifiers() {
    let code = "export { foo, bar, baz };";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let exports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Export).collect();
    assert_eq!(exports.len(), 3, "Should extract all 3 export specifiers, got: {:?}", exports.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert!(exports.iter().any(|s| s.name == "foo"));
    assert!(exports.iter().any(|s| s.name == "bar"));
    assert!(exports.iter().any(|s| s.name == "baz"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_extract_export_multiple_specifiers`
Expected: FAIL — only 1 export extracted

**Step 3: Implement the fix**

Change `extract_export` from returning `Option<Symbol>` to `Vec<Symbol>`. For the `export { ... }` branch (the `else` at line 62), iterate all named children of the `export_clause`:

```rust
/// Extract export statements — may return multiple symbols for `export { a, b, c }`
pub(super) fn extract_export(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    if let Some(declaration_node) = node.child_by_field_name("declaration") {
        // export class/function/const/etc — single export
        let name = declaration_node
            .child_by_field_name("name")
            .map(|n| extractor.base().get_node_text(&n));
        if let Some(name) = name {
            let doc_comment = extractor.base().find_doc_comment(&node);
            symbols.push(extractor.base_mut().create_symbol(
                &node, name, SymbolKind::Export,
                SymbolOptions { doc_comment, ..Default::default() },
            ));
        }
    } else if let Some(source_node) = node.child_by_field_name("source") {
        // export { ... } from '...' — re-export
        let name = extractor.base().get_node_text(&source_node)
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();
        let doc_comment = extractor.base().find_doc_comment(&node);
        symbols.push(extractor.base_mut().create_symbol(
            &node, name, SymbolKind::Export,
            SymbolOptions { doc_comment, ..Default::default() },
        ));
    } else if let Some(clause) = node.children(&mut node.walk())
        .find(|c| c.kind() == "export_clause")
    {
        // export { a, b, c } — iterate ALL named children
        let doc_comment = extractor.base().find_doc_comment(&node);
        let mut cursor = clause.walk();
        for child in clause.named_children(&mut cursor) {
            let spec_name = child.child_by_field_name("name")
                .map(|n| extractor.base().get_node_text(&n));
            if let Some(name) = spec_name {
                symbols.push(extractor.base_mut().create_symbol(
                    &child, name, SymbolKind::Export,
                    SymbolOptions { doc_comment: doc_comment.clone(), ..Default::default() },
                ));
            }
        }
    }

    symbols
}
```

Then update the caller in `mod.rs` (or wherever `extract_export` is called via `visit_node` / `extract_symbol_from_node`). The current pattern is likely:
```rust
"export_statement" => {
    symbol = imports_exports::extract_export(self, node);
}
```
Change to:
```rust
"export_statement" => {
    let export_symbols = imports_exports::extract_export(self, node);
    symbols.extend(export_symbols);
    // Don't set `symbol` since we already added them
}
```

Check exactly how the symbol dispatch works in `mod.rs` to wire this correctly. Look at how `extract_enum` (which already returns `Vec<Symbol>`) is handled — follow the same pattern.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib test_extract_export`
Expected: All export tests PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/imports_exports.rs crates/julie-extractors/src/typescript/mod.rs crates/julie-extractors/src/tests/typescript/imports_exports.rs
git commit -m "fix(typescript): extract all specifiers from export { a, b, c }"
```

---

## Task 2: TypeScript — Interface Member Extraction

**Files:**
- Modify: `crates/julie-extractors/src/typescript/interfaces.rs`
- Test: `crates/julie-extractors/src/tests/typescript/types.rs`

**Context:** `extract_interface()` (lines 11-30) only extracts the interface name. No child symbols are created for method signatures or property signatures. The enum extractor (`extract_enum` at lines 54-121) already demonstrates the correct pattern: return `Vec<Symbol>`, iterate body children, create child symbols with `parent_id`.

**Step 1: Write failing test**

In `tests/typescript/types.rs`, add:

```rust
#[test]
fn test_extract_interface_members() {
    let code = r#"
interface User {
    id: number;
    name: string;
    getName(): string;
    setName(name: string): void;
}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Interface itself
    let interface = symbols.iter().find(|s| s.name == "User" && s.kind == SymbolKind::Interface);
    assert!(interface.is_some(), "Should extract interface User");
    let interface_id = &interface.unwrap().id;

    // Property members
    let id_prop = symbols.iter().find(|s| s.name == "id" && s.kind == SymbolKind::Property);
    assert!(id_prop.is_some(), "Should extract property 'id'");
    assert_eq!(id_prop.unwrap().parent_id.as_ref(), Some(interface_id));

    let name_prop = symbols.iter().find(|s| s.name == "name" && s.kind == SymbolKind::Property);
    assert!(name_prop.is_some(), "Should extract property 'name'");

    // Method members
    let get_name = symbols.iter().find(|s| s.name == "getName" && s.kind == SymbolKind::Method);
    assert!(get_name.is_some(), "Should extract method 'getName'");
    assert_eq!(get_name.unwrap().parent_id.as_ref(), Some(interface_id));

    let set_name = symbols.iter().find(|s| s.name == "setName" && s.kind == SymbolKind::Method);
    assert!(set_name.is_some(), "Should extract method 'setName'");
}
```

Note: this test uses `tree_sitter_typescript::LANGUAGE_TYPESCRIPT` — make sure the import is present. If not, add `use tree_sitter_typescript;`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_extract_interface_members`
Expected: FAIL — no property or method symbols found

**Step 3: Implement the fix**

Change `extract_interface` to return `Vec<Symbol>`, following the `extract_enum` pattern:

```rust
/// Extract an interface declaration and its members
pub(super) fn extract_interface(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let name_node = node.child_by_field_name("name");
    let name = match name_node.map(|n| extractor.base().get_node_text(&n)) {
        Some(name) => name,
        None => return symbols,
    };

    let doc_comment = extractor.base().find_doc_comment(&node);

    let interface_symbol = extractor.base_mut().create_symbol(
        &node, name, SymbolKind::Interface,
        SymbolOptions { doc_comment, ..Default::default() },
    );

    let parent_id = interface_symbol.id.clone();
    symbols.push(interface_symbol);

    // Extract members from the interface body
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "method_signature" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let member_name = extractor.base().get_node_text(&name_node);
                        let sig = extractor.base().get_node_text(&child);
                        let member = extractor.base_mut().create_symbol(
                            &child, member_name, SymbolKind::Method,
                            SymbolOptions {
                                parent_id: Some(parent_id.clone()),
                                signature: Some(sig),
                                ..Default::default()
                            },
                        );
                        symbols.push(member);
                    }
                }
                "property_signature" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let member_name = extractor.base().get_node_text(&name_node);
                        let sig = extractor.base().get_node_text(&child);
                        let member = extractor.base_mut().create_symbol(
                            &child, member_name, SymbolKind::Property,
                            SymbolOptions {
                                parent_id: Some(parent_id.clone()),
                                signature: Some(sig),
                                ..Default::default()
                            },
                        );
                        symbols.push(member);
                    }
                }
                _ => {}
            }
        }
    }

    symbols
}
```

Update the caller in `mod.rs` to handle the Vec return — same pattern as `extract_enum`.

**IMPORTANT:** Verify the tree-sitter node kinds by parsing a test interface and printing the AST. The TypeScript grammar may use `"method_signature"` and `"property_signature"` inside `"interface_body"` — but verify this. If the kinds differ, adjust accordingly.

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib test_extract_interface`
Expected: All interface tests PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/interfaces.rs crates/julie-extractors/src/typescript/mod.rs crates/julie-extractors/src/tests/typescript/types.rs
git commit -m "feat(typescript): extract interface method and property members"
```

---

## Task 3: TypeScript — Fix `find_containing_function` for Methods

**Files:**
- Modify: `crates/julie-extractors/src/typescript/relationships.rs` (line 147)
- Test: `crates/julie-extractors/src/tests/typescript/relationships.rs`

**Context:** `find_containing_function` at line 147 only matches `SymbolKind::Function`. Methods are `SymbolKind::Method`, so call relationships inside class methods are silently dropped. One-line fix.

**Step 1: Write failing test**

In `tests/typescript/relationships.rs`, add:

```rust
#[test]
fn test_method_call_relationships() {
    let code = r#"
class UserService {
    getUser() {
        fetchData();
    }
}
function fetchData() {
    return {};
}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // getUser() should have a Calls relationship to fetchData()
    let get_user = symbols.iter().find(|s| s.name == "getUser").unwrap();
    let calls_from_method = relationships.iter().any(|r| {
        r.source_id == get_user.id && r.kind == crate::base::RelationshipKind::Calls
    });
    assert!(calls_from_method, "Method getUser should have a Calls relationship. Relationships: {:?}", relationships);
}
```

Check exact field names for `Relationship` struct (source_id/target_id/kind) — verify against the base types.

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_method_call_relationships`
Expected: FAIL — no Calls relationship found from method

**Step 3: Implement the fix**

In `relationships.rs` line 147, change:
```rust
if matches!(symbol.kind, SymbolKind::Function)
```
to:
```rust
if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor)
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib test_method_call`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/relationships.rs crates/julie-extractors/src/tests/typescript/relationships.rs
git commit -m "fix(typescript): find_containing_function now matches Method and Constructor"
```

---

## Task 4: YAML — Meaningful Names + Anchor/Alias Support

**Files:**
- Modify: `crates/julie-extractors/src/yaml/mod.rs`
- Test: `crates/julie-extractors/src/tests/yaml/mod.rs`

**Context:** The YAML extractor has two P1 issues:
1. Generic `"document"` and `"flow_mapping"` names are pure noise
2. No anchor (`&name`) / alias (`*name`) support

**Step 1: Write failing tests**

```rust
#[test]
fn test_document_uses_first_key_as_name() {
    let yaml = r#"
name: my-app
version: 1.0
"#;
    let symbols = extract_symbols(yaml);
    // The document should NOT be named "document"
    let doc_symbols: Vec<_> = symbols.iter().filter(|s| s.name == "document").collect();
    assert!(doc_symbols.is_empty(), "Should not create generic 'document' symbol");
}

#[test]
fn test_flow_mapping_not_extracted_as_symbol() {
    let yaml = r#"
config: {host: localhost, port: 8080}
"#;
    let symbols = extract_symbols(yaml);
    let flow_symbols: Vec<_> = symbols.iter().filter(|s| s.name == "flow_mapping").collect();
    assert!(flow_symbols.is_empty(), "Should not create generic 'flow_mapping' symbol");
}

#[test]
fn test_yaml_anchor_extraction() {
    let yaml = r#"
defaults: &defaults
  adapter: postgres
  host: localhost

development:
  <<: *defaults
  database: dev_db
"#;
    let symbols = extract_symbols(yaml);
    // Should have keys: defaults, adapter, host, development, database
    let defaults = symbols.iter().find(|s| s.name == "defaults");
    assert!(defaults.is_some(), "Should extract 'defaults' key");
    // The anchor name should be in the signature or doc_comment
    let defaults = defaults.unwrap();
    let has_anchor_info = defaults.signature.as_ref().map_or(false, |s| s.contains("&defaults"))
        || defaults.doc_comment.as_ref().map_or(false, |d| d.contains("&defaults"));
    assert!(has_anchor_info, "Should include anchor name in signature or doc_comment");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p julie-extractors --lib test_document_uses_first_key -- test_flow_mapping_not -- test_yaml_anchor`
Expected: All FAIL

**Step 3: Implement the fixes**

Three changes to `mod.rs`:

1. **Remove `extract_document`** — stop creating `"document"` symbols. The document node is just a container; its keys are already extracted as children.

2. **Remove `extract_flow_mapping`** — stop creating `"flow_mapping"` symbols. Flow mapping pairs are already extracted individually if they contain keys.

3. **Add anchor detection** in `extract_mapping_pair` — when a mapping pair has an anchor tag (`&name`), include it in the signature. The tree-sitter-yaml grammar may represent anchors as `"anchor"` child nodes or as part of the value text.

For the anchor handling, check the AST structure by looking at what tree-sitter-yaml produces for `defaults: &defaults`. The anchor may be a child of the `block_mapping_pair` node or of its value child.

In `extract_symbol_from_node`, change:
```rust
match node.kind() {
    "document" => None,  // Skip — container only, no meaningful name
    "block_mapping_pair" => self.extract_mapping_pair(node, parent_id),
    "flow_mapping" => None,  // Skip — inline mappings are noise
    _ => None,
}
```

In `extract_mapping_pair`, after extracting the key name, scan children for an `"anchor"` node:
```rust
// Check for anchor (&name)
let mut anchor_name = None;
let mut cursor = node.walk();
for child in node.children(&mut cursor) {
    if child.kind() == "anchor" {
        let text = self.base.get_node_text(&child);
        // Anchor text includes &, e.g., "&defaults"
        anchor_name = Some(text);
    }
}

let signature = anchor_name.map(|a| format!("{}: {}", key_name, a));
```

**IMPORTANT:** Verify the tree-sitter-yaml node types by inspecting a parsed anchor YAML. The anchor might be under a different parent or have a different node kind. Print the tree if uncertain.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p julie-extractors --lib yaml`
Expected: All YAML tests PASS (old tests may need updating since `"document"` symbols are removed)

**Step 5: Update existing tests**

Some existing tests assert `"document"` in the symbols list. Update them to reflect the new behavior (no document symbols). Check `test_extract_simple_key_value_pairs`, `test_github_actions_workflow`, etc.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/yaml/mod.rs crates/julie-extractors/src/tests/yaml/mod.rs
git commit -m "fix(yaml): remove generic document/flow_mapping noise, add anchor detection"
```

---

## Task 5: Python — `@property` as SymbolKind::Property

**Files:**
- Modify: `crates/julie-extractors/src/python/functions.rs` (lines 56, 132-171)
- Test: `crates/julie-extractors/src/tests/python/mod.rs`

**Context:** `determine_function_kind()` returns `Method` for all class methods. It doesn't check the decorator list for `@property`. The decorator extraction already works — `extract_decorators()` returns `Vec<String>` with entries like `"property"`.

**Step 1: Write failing test**

```rust
#[test]
fn test_property_decorator_uses_property_kind() {
    let code = r#"
class User:
    def __init__(self, first, last):
        self._first = first
        self._last = last

    @property
    def full_name(self) -> str:
        return f"{self._first} {self._last}"

    @full_name.setter
    def full_name(self, value):
        parts = value.split()
        self._first = parts[0]
        self._last = parts[1]
"#;
    let (mut extractor, tree) = create_extractor_and_parse(code);
    let symbols = extractor.extract_symbols(&tree);

    let full_name = symbols.iter().find(|s| s.name == "full_name" && s.kind == SymbolKind::Property);
    assert!(full_name.is_some(), "Method with @property should use SymbolKind::Property, got: {:?}",
        symbols.iter().filter(|s| s.name == "full_name").map(|s| &s.kind).collect::<Vec<_>>());
}
```

Note: Check how other Python tests create extractors — they may use `create_extractor_and_parse` helper or create directly. Match the existing test pattern.

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_property_decorator_uses_property_kind`
Expected: FAIL — SymbolKind is Method, not Property

**Step 3: Implement the fix**

In `extract_function()` (functions.rs), after computing `decorators_list` (around line 40) and before calling `determine_function_kind` (line 56), pass the decorators to the kind determination:

```rust
// Determine if it's a method or function based on context
let (symbol_kind, parent_id) = determine_function_kind(extractor, &node, &name, &decorators_list);
```

Then modify `determine_function_kind` to accept decorators:

```rust
fn determine_function_kind(
    extractor: &PythonExtractor,
    node: &Node,
    name: &str,
    decorators: &[String],
) -> (SymbolKind, Option<String>) {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "class_definition" {
            let class_name = match parent.child_by_field_name("name") {
                Some(name_node) => extractor.base().get_node_text(&name_node),
                None => continue,
            };

            let start_pos = parent.start_position();
            let parent_id = extractor.base().generate_id(
                &class_name,
                start_pos.row as u32,
                start_pos.column as u32,
            );

            let symbol_kind = if name == "__init__" {
                SymbolKind::Constructor
            } else if decorators.iter().any(|d| d == "property" || d.ends_with(".setter") || d.ends_with(".getter") || d.ends_with(".deleter")) {
                SymbolKind::Property
            } else {
                SymbolKind::Method
            };

            return (symbol_kind, Some(parent_id));
        }
        current = parent;
    }

    (SymbolKind::Function, None)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p julie-extractors --lib python`
Expected: All Python tests PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/python/functions.rs crates/julie-extractors/src/tests/python/mod.rs
git commit -m "feat(python): @property decorated methods use SymbolKind::Property"
```

---

## Task 6: PHP — Grouped Use Declarations

**Files:**
- Modify: `crates/julie-extractors/src/php/namespaces.rs` (lines 42-97)
- Modify: `crates/julie-extractors/src/php/mod.rs` (caller site)
- Test: `crates/julie-extractors/src/tests/php/mod.rs` or `edge_cases.rs`

**Context:** `extract_use()` uses `find_child()` which only returns the first `namespace_use_clause`. Grouped declarations like `use App\{Controller, Model, Service}` drop all but the first. Function needs to return `Vec<Symbol>`.

**Step 1: Write failing test**

```rust
#[test]
fn test_extract_grouped_use_declarations() {
    // Test PHP grouped use: use App\{Controller, Model, Service}
    let code = r#"<?php
use App\{Controller, Model, Service};
"#;
    // Create parser and extractor (match existing PHP test pattern)
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(), "test.php".to_string(), code.to_string(), &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let imports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();
    assert!(imports.len() >= 3, "Should extract all 3 grouped imports, got {}: {:?}",
        imports.len(), imports.iter().map(|s| &s.name).collect::<Vec<_>>());
}
```

**IMPORTANT:** First verify what tree-sitter-php produces for grouped use declarations. Print the AST to understand the node structure. The grammar may produce multiple `namespace_use_clause` children, or it may produce a `namespace_use_group` with children. Adjust the implementation based on the actual AST.

**Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_extract_grouped_use_declarations`
Expected: FAIL — only 1 import

**Step 3: Implement the fix**

Change `extract_use` to return `Vec<Symbol>`. For `namespace_use_declaration` nodes, iterate ALL `namespace_use_clause` children instead of just the first:

```rust
pub(super) fn extract_use(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    match node.kind() {
        "namespace_use_declaration" => {
            // Iterate ALL namespace_use_clause children (handles grouped imports)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "namespace_use_clause" {
                    if let Some(qualified_name) = find_child(extractor, &child, "qualified_name") {
                        let name = extractor.get_base().get_node_text(&qualified_name);
                        let alias = find_child(extractor, &child, "namespace_aliasing_clause")
                            .map(|a| extractor.get_base().get_node_text(&a));

                        let mut signature = format!("use {}", name);
                        if let Some(ref alias_text) = alias {
                            signature.push_str(&format!(" {}", alias_text));
                        }

                        let mut metadata = HashMap::new();
                        metadata.insert("type".to_string(), serde_json::Value::String("use".to_string()));
                        if let Some(alias_text) = alias {
                            metadata.insert("alias".to_string(), serde_json::Value::String(alias_text));
                        }

                        let doc_comment = extractor.get_base().find_doc_comment(&node);

                        symbols.push(extractor.get_base_mut().create_symbol(
                            &child, name, SymbolKind::Import,
                            SymbolOptions {
                                signature: Some(signature),
                                visibility: Some(Visibility::Public),
                                parent_id: parent_id.map(|s| s.to_string()),
                                metadata: Some(metadata),
                                doc_comment,
                            },
                        ));
                    }
                }
            }

            // If no clause children found, try legacy format
            if symbols.is_empty() {
                let name = find_child(extractor, &node, "namespace_name")
                    .or_else(|| find_child(extractor, &node, "qualified_name"))
                    .map(|n| extractor.get_base().get_node_text(&n));
                if let Some(name) = name {
                    let doc_comment = extractor.get_base().find_doc_comment(&node);
                    symbols.push(extractor.get_base_mut().create_symbol(
                        &node, name.clone(), SymbolKind::Import,
                        SymbolOptions {
                            signature: Some(format!("use {}", name)),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment,
                        },
                    ));
                }
            }
        }
        _ => {
            // Legacy use_declaration
            let name = find_child(extractor, &node, "namespace_name")
                .or_else(|| find_child(extractor, &node, "qualified_name"))
                .map(|n| extractor.get_base().get_node_text(&n));
            if let Some(name) = name {
                let alias = find_child(extractor, &node, "namespace_aliasing_clause")
                    .map(|a| extractor.get_base().get_node_text(&a));
                let mut signature = format!("use {}", name);
                if let Some(ref alias_text) = alias {
                    signature.push_str(&format!(" {}", alias_text));
                }
                let doc_comment = extractor.get_base().find_doc_comment(&node);
                symbols.push(extractor.get_base_mut().create_symbol(
                    &node, name, SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment,
                    },
                ));
            }
        }
    }

    symbols
}
```

Update the caller in `mod.rs` to handle `Vec<Symbol>` return — `symbols.extend(extract_use(...))`.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p julie-extractors --lib php`
Expected: All PHP tests PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/php/namespaces.rs crates/julie-extractors/src/php/mod.rs crates/julie-extractors/src/tests/php/
git commit -m "fix(php): extract all clauses from grouped use declarations"
```

---

## Task 7: Razor — Stop Extracting Assignment/Element-Access as Definitions

**Files:**
- Modify: `crates/julie-extractors/src/razor/mod.rs` (lines 134-135)
- Modify: `crates/julie-extractors/src/razor/csharp.rs` (lines 55-56)
- Test: `crates/julie-extractors/src/tests/razor/mod.rs`

**Context:** `assignment_expression` (mod.rs:135) and `element_access_expression` (csharp.rs:55-56) are extracted as Variable symbols. These are usages (reading/writing ViewData, ViewBag), not definitions. Same pattern as the invocation_expression fix from Round 2 — make them no-ops.

**Step 1: Write test confirming noise elimination**

```rust
#[test]
fn test_assignment_and_element_access_not_extracted_as_definitions() {
    let code = r#"@{
    ViewData["Title"] = "Home Page";
    ViewBag.Message = "Hello";
}
<h1>@ViewData["Title"]</h1>"#;
    // Parse and extract (match existing Razor test pattern)
    let symbols = extract_symbols(code);
    // Assignments should NOT create Variable symbols
    let viewdata_def = symbols.iter().find(|s| s.name == "ViewData" && s.kind == SymbolKind::Variable);
    assert!(viewdata_def.is_none(), "ViewData assignment should not create a definition symbol");
    let viewbag_def = symbols.iter().find(|s| s.name == "ViewBag" && s.kind == SymbolKind::Variable);
    assert!(viewbag_def.is_none(), "ViewBag assignment should not create a definition symbol");
}
```

**Step 2: Run test to verify it fails**

Expected: FAIL — ViewData Variable symbol exists

**Step 3: Implement the fix**

In `mod.rs`, change:
```rust
"assignment_expression" => {
    symbol = self.extract_assignment(node, parent_id.as_deref());
}
```
to:
```rust
// Assignment expressions (ViewData["Title"] = "Home") are USAGES, not definitions.
// They are tracked via identifier extraction for reference relationships.
"assignment_expression" => {}
```

In `csharp.rs`, change:
```rust
"element_access_expression" => {
    symbol = self.extract_element_access(node, parent_id);
}
```
to:
```rust
// Element access expressions (ViewData["Title"]) are USAGES, not definitions.
"element_access_expression" => {}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p julie-extractors --lib razor`
Expected: PASS (some existing tests may need updating if they assert on ViewData symbols)

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/razor/mod.rs crates/julie-extractors/src/razor/csharp.rs crates/julie-extractors/src/tests/razor/mod.rs
git commit -m "fix(razor): stop extracting assignment/element_access as definitions"
```

---

## Task 8: CSS — Deduplicate @keyframes Animation Name

**Files:**
- Modify: `crates/julie-extractors/src/css/mod.rs` (lines 85-93)
- Modify: `crates/julie-extractors/src/css/animations.rs` (delete `extract_animation_name`)
- Test: `crates/julie-extractors/src/tests/css/animations.rs` (or `mod.rs`)

**Context:** `@keyframes fadeIn { ... }` creates TWO symbols: `@keyframes fadeIn` (from `extract_keyframes_rule`) and `fadeIn` (from `extract_animation_name`). The second is redundant — remove it.

**Step 1: Write test confirming no duplicate**

```rust
#[test]
fn test_keyframes_no_duplicate_animation_name() {
    let css = "@keyframes slideIn { from { opacity: 0; } to { opacity: 1; } }";
    let symbols = extract_symbols(css);
    let keyframe_symbols: Vec<_> = symbols.iter()
        .filter(|s| s.name.contains("slideIn"))
        .collect();
    assert_eq!(keyframe_symbols.len(), 1, "Should have exactly 1 symbol for slideIn, not 2. Got: {:?}",
        keyframe_symbols.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert_eq!(keyframe_symbols[0].name, "@keyframes slideIn");
}
```

**Step 2: Run test to verify it fails**

Expected: FAIL — 2 symbols found (both `@keyframes slideIn` and `slideIn`)

**Step 3: Implement the fix**

In `css/mod.rs`, remove the second extraction call (lines 85-93):
```rust
// REMOVE this block:
// Also extract the animation name as a separate symbol
if let Some(animation_symbol) = AnimationExtractor::extract_animation_name(
    &mut self.base,
    node,
    current_parent_id.as_deref(),
) {
    symbols.push(animation_symbol);
}
```

In `animations.rs`, remove or mark as dead the `extract_animation_name` function (lines 48-79). Since nothing calls it anymore, just delete it.

Also fix the inline `Regex::new()` in `extract_keyframes_name` (line 99) — use `LazyLock`:
```rust
use std::sync::LazyLock;

static KEYFRAMES_NAME_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"@keyframes\s+([^\s{]+)").unwrap());

pub(super) fn extract_keyframes_name(base: &BaseExtractor, node: &Node) -> Option<String> {
    let text = base.get_node_text(node);
    KEYFRAMES_NAME_RE.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p julie-extractors --lib css`
Expected: PASS (update any tests that assert on the separate animation name symbol)

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/css/mod.rs crates/julie-extractors/src/css/animations.rs crates/julie-extractors/src/tests/css/
git commit -m "fix(css): deduplicate @keyframes animation name, use LazyLock for regex"
```

---

## Task 9: LazyLock Regex Audit (Cross-Cutting)

**Files to modify** (inline `Regex::new()` → `LazyLock`):
- `crates/julie-extractors/src/rust/mod.rs` — 2 regex in `infer_types()` (lines ~156, ~174)
- `crates/julie-extractors/src/python/mod.rs` — 2 regex in `infer_type_from_signature()` (lines ~119, ~128)
- `crates/julie-extractors/src/bash/variables.rs` — 1 regex in `is_environment_variable()` (line ~114)
- `crates/julie-extractors/src/powershell/helpers.rs` — 4 regex (lines ~140, ~155, ~164, ~173)
- `crates/julie-extractors/src/css/properties.rs` — 1 regex in `extract_supports_condition()` (line ~96)
- `crates/julie-extractors/src/sql/constraints.rs` — ~5 regex in `extract_alter_table_constraint()`

**Context:** These extractors compile regex on every function call. The pattern is always the same: replace inline `Regex::new(...)` with a module-level `static REGEX_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(...).unwrap());`. This is safe, correct, and improves performance.

**Step 1: No new tests needed**

This is a pure performance refactoring — existing tests verify correctness. Run the full extractor test suite before and after.

**Step 2: Run baseline tests**

Run: `cargo test -p julie-extractors --lib`
Expected: 1183 tests PASS

**Step 3: Apply the pattern across all files**

For each file, move the inline regex to a module-level `LazyLock`:

```rust
use std::sync::LazyLock;
use regex::Regex;

static RETURN_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"->\s*([^{]+)").unwrap());
```

Then replace the inline usage:
```rust
// Before:
let re = Regex::new(r"->\s*([^{]+)").unwrap();
// After:
RETURN_TYPE_RE.captures(...)
```

Apply to all files listed above. Use descriptive regex names (e.g., `RETURN_TYPE_RE`, `VARIABLE_TYPE_RE`, `ENV_VAR_RE`, `PARAMETER_ATTR_RE`, etc.).

**Step 4: Run tests to verify no regressions**

Run: `cargo test -p julie-extractors --lib`
Expected: 1183 tests PASS

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/rust/mod.rs crates/julie-extractors/src/python/mod.rs crates/julie-extractors/src/bash/variables.rs crates/julie-extractors/src/powershell/helpers.rs crates/julie-extractors/src/css/properties.rs crates/julie-extractors/src/sql/constraints.rs
git commit -m "perf: replace inline Regex::new() with LazyLock across 6 extractors"
```

---

## Task 10: Dead Code Cleanup

**Files:**
- `crates/julie-extractors/src/ruby/helpers.rs` — 2 dead functions (lines 79, 92)
- `crates/julie-extractors/src/ruby/assignments.rs` — 2 dead functions (lines 127, 254)
- `crates/julie-extractors/src/regex/mod.rs` — 3 dead modules + 1 dead function (lines 6-11, 124)
- `crates/julie-extractors/src/bash/signatures.rs` — 2 dead functions (lines 48, 76)
- `crates/julie-extractors/src/bash/helpers.rs` — 1 dead function (line 114)
- `crates/julie-extractors/src/c/signatures.rs` — 1 dead function (line 343)

**Context:** These `#[allow(dead_code)]` annotations suppress warnings for functions/modules that are genuinely unused. Remove the dead code. If a module has mix of used and unused functions, only remove the unused ones and drop the `#[allow(dead_code)]`.

**Step 1: Run baseline tests**

Run: `cargo test -p julie-extractors --lib`
Expected: 1183 tests PASS

**Step 2: For each file:**

1. Read the function/module marked dead
2. Verify it's actually unused with `fast_refs` (check for zero callers)
3. Delete the function (or remove the module import if the entire module is dead)
4. Remove the `#[allow(dead_code)]` annotation

**IMPORTANT:** For the regex modules (`helpers`, `patterns`, `signatures`), check if the modules contain ANY used functions before deleting. If a module has some used and some unused functions, only delete the unused ones and keep the module. Use `fast_refs` for each public function in those modules.

For the `extract_patterns_from_text` function in `regex/mod.rs` (line 124), it was intentionally disabled in Round 2 — safe to delete.

**Step 3: Run tests to verify no regressions**

Run: `cargo test -p julie-extractors --lib`
Expected: 1183 tests PASS, 0 warnings about dead code from these files

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/ruby/ crates/julie-extractors/src/regex/ crates/julie-extractors/src/bash/ crates/julie-extractors/src/c/signatures.rs
git commit -m "chore: remove dead code across ruby, regex, bash, c extractors"
```

---

## Execution Notes

### Task Dependencies
All 10 tasks are independent. Tasks 1-3 all touch TypeScript but different files (imports_exports.rs, interfaces.rs, relationships.rs) so they CAN run in parallel. Tasks 4-10 touch completely different extractors.

### Post-Execution Verification
After all tasks complete, run the full extractor test suite:
```bash
cargo test -p julie-extractors --lib
```
Expected: 1183+ tests PASS (new tests added by tasks 1-8)

### Audit Document Update
After all tasks complete, update `docs/EXTRACTOR_AUDIT.md`:
- TypeScript: B → B+ (export fix + interface members + method containment)
- YAML: B → B+ (meaningful names, no noise)
- Python: B → B+ (@property support)
- PHP: B cleaner (grouped use fix)
- Razor: B cleaner (noise elimination)
- CSS: B cleaner (no duplicates)
- Cross-cutting: LazyLock + dead code cleanup
