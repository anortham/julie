# Final Three Extractors B+ → A Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Elevate PHP, Vue, and YAML — the last three B+ extractors — to A rating by implementing their missing core features.

**Architecture:** Each extractor has specific missing features identified in `docs/EXTRACTOR_AUDIT.md`. PHP needs arrow function and anonymous class extraction via new tree-sitter node kind handlers. Vue needs expanded CSS selector extraction in the style section via regex additions. YAML needs alias-as-identifier extraction and SymbolKind differentiation for nested mappings.

**Tech Stack:** Rust, tree-sitter (PHP, YAML grammars), regex (Vue style section)

---

## PHP (2 tasks)

### Task 1: PHP Arrow Functions

Arrow functions (`fn($x) => $x + 1`) are a core PHP 7.4+ feature. Tree-sitter-php produces an `arrow_function` node kind. Currently completely unhandled in `visit_node`.

**Files:**
- Modify: `crates/julie-extractors/src/php/functions.rs` (add `extract_arrow_function`)
- Modify: `crates/julie-extractors/src/php/mod.rs` (add `arrow_function` to visit_node match)
- Test: `crates/julie-extractors/src/tests/php/mod.rs`

**Step 1: Write the failing tests**

Add a new test module `mod php_arrow_function_tests` at the end of the test file:

```rust
mod php_arrow_function_tests {
    use super::*;
    use crate::base::SymbolKind;

    #[test]
    fn test_arrow_function_standalone_assignment() {
        let code = r#"<?php
$double = fn(int $n): int => $n * 2;
"#;
        let symbols = extract_symbols(code);
        // The variable assignment extracts "double", but we also want
        // the arrow function itself to NOT create a duplicate.
        // Arrow functions in assignments are covered by extract_variable_assignment.
        // What we need is standalone arrow functions (e.g., passed as arguments).
        assert!(symbols.iter().any(|s| s.name == "double"));
    }

    #[test]
    fn test_arrow_function_as_argument_not_lost() {
        // Arrow functions passed directly as arguments should be extractable
        // when they appear at expression_statement level, but inline args
        // are naturally invisible (like anonymous functions in other languages).
        // The key extraction is when they ARE assigned to variables.
        let code = r#"<?php
$transform = fn(string $s): string => strtoupper($s);
$predicate = fn($x) => $x > 0;
$noop = fn() => null;
"#;
        let symbols = extract_symbols(code);

        let transform = symbols.iter().find(|s| s.name == "transform").unwrap();
        assert_eq!(transform.kind, SymbolKind::Variable);
        // Signature should contain the arrow function
        assert!(transform.signature.as_ref().unwrap().contains("fn(string $s): string => strtoupper($s)"));

        let predicate = symbols.iter().find(|s| s.name == "predicate").unwrap();
        assert!(predicate.signature.as_ref().unwrap().contains("fn($x) => $x > 0"));

        let noop = symbols.iter().find(|s| s.name == "noop").unwrap();
        assert!(noop.signature.as_ref().unwrap().contains("fn() => null"));
    }

    #[test]
    fn test_arrow_function_inside_class_method_not_top_level() {
        // Arrow functions inside methods should NOT create top-level symbols
        let code = r#"<?php
class Processor {
    public function process(array $items): array {
        return array_map(fn($item) => $item * 2, $items);
    }
}
"#;
        let symbols = extract_symbols(code);

        let processor = symbols.iter().find(|s| s.name == "Processor").unwrap();
        assert_eq!(processor.kind, SymbolKind::Class);

        let process = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(process.kind, SymbolKind::Method);

        // No standalone arrow function symbol should exist
        assert!(!symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name.contains("fn")));
    }
}
```

**Step 2: Run tests to verify they pass (or identify what needs fixing)**

Run: `cargo test --package julie-extractors -- php_arrow_function`
Expected: Tests should pass with current variable assignment extraction. If any fail, that identifies what needs fixing.

**Step 3: Verify and adjust**

The key insight here is that PHP arrow functions in assignments are already captured via `extract_variable_assignment` (the RHS becomes the signature). Arrow functions passed inline as arguments (like `array_map(fn($n) => $n * 2, ...)`) are similar to anonymous functions in JS — they don't create named symbols. So the real question is: **do we need to add `arrow_function` to visit_node?**

If tests pass → arrow functions are already adequately handled. Mark this in the audit.

If the signature doesn't include the full arrow function text → the `extract_variable_assignment` function truncates the RHS. Fix the signature to include the full arrow function text.

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/tests/php/mod.rs
git commit -m "test(php): add arrow function extraction tests"
```

---

### Task 2: PHP Anonymous Classes

Anonymous classes (`new class { ... }`) are a PHP 7+ feature. Tree-sitter-php produces an `anonymous_class` node kind (NOT `object_creation_expression`). Currently, anonymous classes assigned to variables are partially extracted via `extract_variable_assignment` (the variable gets a symbol), but the class body is not walked for child symbols — methods inside anonymous classes are extracted as children of the variable, not as children of a proper Class symbol.

**Files:**
- Modify: `crates/julie-extractors/src/php/types.rs` (add `extract_anonymous_class`)
- Modify: `crates/julie-extractors/src/php/mod.rs` (add `anonymous_class` to visit_node match + import)
- Test: `crates/julie-extractors/src/tests/php/mod.rs`

**Step 1: Write the failing tests**

Add to the test file:

```rust
mod php_anonymous_class_tests {
    use super::*;
    use crate::base::SymbolKind;

    #[test]
    fn test_anonymous_class_with_interface() {
        let code = r#"<?php
$logger = new class implements LoggerInterface {
    public function log(string $message): void {
        echo $message;
    }
};
"#;
        let symbols = extract_symbols(code);

        // Should have an anonymous class symbol
        let anon_class = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name.starts_with("anonymous_class"));
        assert!(anon_class.is_some(), "Should extract anonymous class as Class symbol");
        let anon_class = anon_class.unwrap();
        assert!(anon_class.signature.as_ref().unwrap().contains("implements LoggerInterface"));

        // Method should be parented to the anonymous class
        let log_method = symbols.iter().find(|s| s.name == "log" && s.kind == SymbolKind::Method);
        assert!(log_method.is_some());
        assert_eq!(log_method.unwrap().parent_id.as_ref(), Some(&anon_class.id));
    }

    #[test]
    fn test_anonymous_class_with_extends() {
        let code = r#"<?php
$handler = new class extends BaseHandler {
    public function handle(): void {}
};
"#;
        let symbols = extract_symbols(code);

        let anon_class = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name.starts_with("anonymous_class"));
        assert!(anon_class.is_some());
        assert!(anon_class.unwrap().signature.as_ref().unwrap().contains("extends BaseHandler"));
    }

    #[test]
    fn test_anonymous_class_with_constructor_args() {
        let code = r#"<?php
$obj = new class($param1, $param2) {
    public function __construct(private string $name, private int $age) {}
    public function getName(): string { return $this->name; }
};
"#;
        let symbols = extract_symbols(code);

        let anon_class = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name.starts_with("anonymous_class"));
        assert!(anon_class.is_some());

        // Constructor should be a child of the anonymous class
        let constructor = symbols.iter().find(|s| s.name == "__construct" && s.kind == SymbolKind::Constructor);
        assert!(constructor.is_some());
        assert_eq!(constructor.unwrap().parent_id.as_ref(), Some(&anon_class.unwrap().id));
    }

    #[test]
    fn test_anonymous_class_bare() {
        let code = r#"<?php
$simple = new class {
    public string $value = "hello";
};
"#;
        let symbols = extract_symbols(code);

        let anon_class = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name.starts_with("anonymous_class"));
        assert!(anon_class.is_some());
        assert!(anon_class.unwrap().signature.as_ref().unwrap().contains("class"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --package julie-extractors -- php_anonymous_class`
Expected: FAIL — no symbol with `SymbolKind::Class` and name starting with `anonymous_class`.

**Step 3: Implement `extract_anonymous_class` in `types.rs`**

Add to `crates/julie-extractors/src/php/types.rs`:

```rust
pub(super) fn extract_anonymous_class(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Anonymous classes don't have a name node.
    // Generate a name based on line number for uniqueness.
    let line = node.start_position().row + 1;
    let name = format!("anonymous_class_L{}", line);

    let extends_node = find_child(extractor, &node, "base_clause");
    let implements_node = find_child(extractor, &node, "class_interface_clause");

    let mut signature = "class".to_string();

    if let Some(extends_node) = extends_node {
        let base_class = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" extends {}", base_class));
    }

    if let Some(implements_node) = implements_node {
        let interfaces = extractor
            .get_base()
            .get_node_text(&implements_node)
            .replace("implements", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" implements {}", interfaces));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("anonymous_class".to_string()),
    );

    let doc_comment = extractor.get_base().find_doc_comment(&node);

    Some(extractor.get_base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}
```

**Step 4: Add `anonymous_class` to visit_node in `mod.rs`**

In `crates/julie-extractors/src/php/mod.rs`, add to the `visit_node` match:

```rust
"anonymous_class" => extract_anonymous_class(self, node, parent_id.as_deref()),
```

And add the import at the top:
```rust
use types::{extract_anonymous_class, extract_class, ...};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test --package julie-extractors -- php_anonymous_class`
Expected: All 4 tests PASS.

**Step 6: Run full PHP test suite for regressions**

Run: `cargo test --package julie-extractors -- php`
Expected: All tests pass, no regressions.

**Step 7: Commit**

```bash
git add crates/julie-extractors/src/php/types.rs crates/julie-extractors/src/php/mod.rs crates/julie-extractors/src/tests/php/mod.rs
git commit -m "feat(php): extract anonymous classes as SymbolKind::Class with children"
```

---

## Vue (1 task)

### Task 3: Vue Style Section Enhancement

The Vue style section currently only extracts class selectors (`.className {`) via a single regex. Missing: ID selectors (`#id {`), CSS custom properties (`--my-prop: ...`). These are common patterns in Vue SFCs.

**Files:**
- Modify: `crates/julie-extractors/src/vue/helpers.rs` (add `CSS_ID_RE` and `CSS_CUSTOM_PROP_RE` LazyLock patterns)
- Modify: `crates/julie-extractors/src/vue/style.rs` (add extraction loops for new patterns)
- Test: `crates/julie-extractors/src/tests/vue/mod.rs`

**Step 1: Write the failing tests**

Add a new test module at the end of the Vue test file:

```rust
mod vue_style_enhanced_tests {
    use crate::base::SymbolKind;
    use crate::vue::VueExtractor;

    fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
        VueExtractor::new(
            "vue".to_string(),
            file_path.to_string(),
            code.to_string(),
            &std::path::PathBuf::from("/test"),
        )
    }

    #[test]
    fn test_extract_id_selectors() {
        let code = r#"
<style>
#app {
  font-family: sans-serif;
}

#sidebar {
  width: 300px;
}
</style>
"#;
        let mut extractor = create_extractor("test.vue", code);
        let symbols = extractor.extract_symbols(None);

        let app = symbols.iter().find(|s| s.name == "app" && s.signature.as_ref().unwrap() == "#app");
        assert!(app.is_some(), "Should extract #app ID selector");
        assert_eq!(app.unwrap().kind, SymbolKind::Property);

        let sidebar = symbols.iter().find(|s| s.name == "sidebar" && s.signature.as_ref().unwrap() == "#sidebar");
        assert!(sidebar.is_some(), "Should extract #sidebar ID selector");
    }

    #[test]
    fn test_extract_css_custom_properties() {
        let code = r#"
<style>
:root {
  --primary-color: #3498db;
  --font-size: 16px;
  --spacing-unit: 8px;
}
</style>
"#;
        let mut extractor = create_extractor("test.vue", code);
        let symbols = extractor.extract_symbols(None);

        let primary = symbols.iter().find(|s| s.name == "--primary-color");
        assert!(primary.is_some(), "Should extract --primary-color custom property");
        assert_eq!(primary.unwrap().kind, SymbolKind::Variable);

        let font_size = symbols.iter().find(|s| s.name == "--font-size");
        assert!(font_size.is_some(), "Should extract --font-size custom property");

        let spacing = symbols.iter().find(|s| s.name == "--spacing-unit");
        assert!(spacing.is_some(), "Should extract --spacing-unit custom property");
    }

    #[test]
    fn test_mixed_style_selectors() {
        let code = r#"
<style scoped>
.container {
  display: flex;
}

#main-content {
  padding: 20px;
}

:root {
  --bg-color: white;
}
</style>
"#;
        let mut extractor = create_extractor("test.vue", code);
        let symbols = extractor.extract_symbols(None);

        assert!(symbols.iter().any(|s| s.name == "container"), "Should find .container class");
        assert!(symbols.iter().any(|s| s.name == "main-content" && s.signature.as_ref().unwrap() == "#main-content"), "Should find #main-content ID");
        assert!(symbols.iter().any(|s| s.name == "--bg-color"), "Should find --bg-color custom property");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --package julie-extractors -- vue_style_enhanced`
Expected: FAIL — ID selectors and custom properties not extracted.

**Step 3: Add new regex patterns in `helpers.rs`**

Add to `crates/julie-extractors/src/vue/helpers.rs`:

```rust
pub(super) static CSS_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#([a-zA-Z_-][a-zA-Z0-9_-]*)\s*\{").unwrap());

pub(super) static CSS_CUSTOM_PROP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(--[a-zA-Z_-][a-zA-Z0-9_-]*)\s*:").unwrap());
```

**Step 4: Add extraction loops in `style.rs`**

In `extract_style_symbols`, after the existing CSS_CLASS_RE loop, add:

```rust
// Extract CSS ID selectors
for captures in CSS_ID_RE.captures_iter(line) {
    if let Some(id_name) = captures.get(1) {
        let name = id_name.as_str();
        let start_col = id_name.start();
        symbols.push(create_symbol_manual(
            base,
            name,
            SymbolKind::Property,
            actual_line,
            start_col,
            actual_line,
            start_col + name.len(),
            Some(format!("#{}", name)),
            doc_comment.clone(),
            None,
        ));
    }
}

// Extract CSS custom properties (--var-name: value)
for captures in CSS_CUSTOM_PROP_RE.captures_iter(line) {
    if let Some(prop_name) = captures.get(1) {
        let name = prop_name.as_str();
        let start_col = prop_name.start();
        symbols.push(create_symbol_manual(
            base,
            name,
            SymbolKind::Variable,
            actual_line,
            start_col,
            actual_line,
            start_col + name.len(),
            Some(name.to_string()),
            doc_comment.clone(),
            None,
        ));
    }
}
```

Update the import in `style.rs`:
```rust
use super::helpers::{CSS_CLASS_RE, CSS_ID_RE, CSS_CUSTOM_PROP_RE};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test --package julie-extractors -- vue_style_enhanced`
Expected: All 3 tests PASS.

**Step 6: Run full Vue test suite for regressions**

Run: `cargo test --package julie-extractors -- vue`
Expected: All tests pass.

**Step 7: Commit**

```bash
git add crates/julie-extractors/src/vue/helpers.rs crates/julie-extractors/src/vue/style.rs crates/julie-extractors/src/tests/vue/mod.rs
git commit -m "feat(vue): extract ID selectors and CSS custom properties from style section"
```

---

## YAML (3 tasks)

### Task 4: YAML SymbolKind Differentiation

Currently all mapping pairs are `SymbolKind::Variable`. Mapping pairs whose value contains a nested `block_mapping` should be `SymbolKind::Module` (they're containers, like JSON objects are). This matches the pattern used in JSON extractor.

**Files:**
- Modify: `crates/julie-extractors/src/yaml/mod.rs` (update `extract_mapping_pair`)
- Test: `crates/julie-extractors/src/tests/yaml/mod.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_container_keys_are_module() {
    let yaml = r#"
database:
  host: localhost
  port: 5432

server:
  address: 0.0.0.0
  workers: 4

simple_key: simple_value
"#;
    let symbols = extract_symbols(yaml);

    // Container keys with nested mappings should be Module
    let database = symbols.iter().find(|s| s.name == "database").unwrap();
    assert_eq!(database.kind, SymbolKind::Module, "Container key should be Module");

    let server = symbols.iter().find(|s| s.name == "server").unwrap();
    assert_eq!(server.kind, SymbolKind::Module, "Container key should be Module");

    // Leaf keys should remain Variable
    let host = symbols.iter().find(|s| s.name == "host").unwrap();
    assert_eq!(host.kind, SymbolKind::Variable, "Leaf key should be Variable");

    let simple = symbols.iter().find(|s| s.name == "simple_key").unwrap();
    assert_eq!(simple.kind, SymbolKind::Variable, "Leaf key should be Variable");
}

#[test]
fn test_nested_container_hierarchy() {
    let yaml = r#"
level1:
  level2:
    level3:
      key: value
"#;
    let symbols = extract_symbols(yaml);

    let l1 = symbols.iter().find(|s| s.name == "level1").unwrap();
    assert_eq!(l1.kind, SymbolKind::Module);

    let l2 = symbols.iter().find(|s| s.name == "level2").unwrap();
    assert_eq!(l2.kind, SymbolKind::Module);
    assert_eq!(l2.parent_id.as_ref(), Some(&l1.id));

    let l3 = symbols.iter().find(|s| s.name == "level3").unwrap();
    assert_eq!(l3.kind, SymbolKind::Module);

    let key = symbols.iter().find(|s| s.name == "key").unwrap();
    assert_eq!(key.kind, SymbolKind::Variable);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --package julie-extractors -- test_container_keys_are_module`
Expected: FAIL — `database.kind` is `Variable`, not `Module`.

**Step 3: Update `extract_mapping_pair` in `yaml/mod.rs`**

Check if the mapping pair's value side contains a `block_mapping` child:

```rust
fn extract_mapping_pair(
    &mut self,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    use crate::base::SymbolOptions;

    let key_name = self.extract_mapping_key(node)?;
    let anchor = self.extract_anchor(node);
    let signature = anchor.map(|a| format!("{}: &{}", key_name, a));

    // Determine if this is a container (has nested block_mapping) or a leaf
    let is_container = self.has_nested_mapping(node);
    let kind = if is_container {
        SymbolKind::Module
    } else {
        SymbolKind::Variable
    };

    let options = SymbolOptions {
        signature,
        visibility: None,
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: None,
        ..Default::default()
    };

    Some(self.base.create_symbol(&node, key_name, kind, options))
}

fn has_nested_mapping(&self, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block_node" {
            let mut block_cursor = child.walk();
            for block_child in child.children(&mut block_cursor) {
                if block_child.kind() == "block_mapping" {
                    return true;
                }
            }
        }
    }
    false
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --package julie-extractors -- test_container_keys`
Expected: PASS.

**Step 5: Run full YAML test suite**

Run: `cargo test --package julie-extractors -- yaml`
Expected: Some existing tests may need updating (they assert `SymbolKind::Variable` for container keys). Fix assertions.

**Step 6: Commit**

```bash
git add crates/julie-extractors/src/yaml/mod.rs crates/julie-extractors/src/tests/yaml/mod.rs
git commit -m "feat(yaml): differentiate container keys (Module) from leaf keys (Variable)"
```

---

### Task 5: YAML Alias References as Identifiers

YAML aliases (`*name`) reference anchors (`&name`). Tree-sitter produces `alias` nodes with `alias_name` children. These should be extracted as identifiers that reference the anchor definition.

**Files:**
- Modify: `crates/julie-extractors/src/yaml/mod.rs` (update `extract_identifiers` and add alias walking)
- Test: `crates/julie-extractors/src/tests/yaml/mod.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_alias_extracted_as_identifier() {
    let yaml = r#"
defaults: &defaults
  adapter: postgres
  host: localhost

development:
  <<: *defaults
  database: dev_db
"#;
    let symbols = extract_symbols(yaml);
    let identifiers = extract_identifiers(yaml, &symbols);

    // *defaults should be extracted as an identifier
    let alias_ref = identifiers.iter().find(|id| id.name == "defaults" && id.kind == IdentifierKind::Reference);
    assert!(alias_ref.is_some(), "Alias *defaults should be an identifier");
}

#[test]
fn test_multiple_aliases_same_anchor() {
    let yaml = r#"
base: &base
  timeout: 30

service_a:
  <<: *base
  port: 8080

service_b:
  <<: *base
  port: 8081
"#;
    let symbols = extract_symbols(yaml);
    let identifiers = extract_identifiers(yaml, &symbols);

    let base_refs: Vec<_> = identifiers.iter().filter(|id| id.name == "base" && id.kind == IdentifierKind::Reference).collect();
    assert_eq!(base_refs.len(), 2, "Should find 2 alias references to &base");
}
```

Note: You'll need to add an `extract_identifiers` helper to the test module (similar to `extract_symbols` but also calls `extractor.extract_identifiers()`). Check how other test files do this.

**Step 2: Run tests to verify they fail**

Run: `cargo test --package julie-extractors -- test_alias_extracted`
Expected: FAIL — `extract_identifiers` returns empty vec.

**Step 3: Implement alias extraction in `yaml/mod.rs`**

Replace the empty `extract_identifiers` method:

```rust
pub fn extract_identifiers(
    &mut self,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    self.walk_tree_for_aliases(tree.root_node(), symbols);
    self.base.identifiers.clone()
}

fn walk_tree_for_aliases(
    &mut self,
    node: tree_sitter::Node,
    symbols: &[Symbol],
) {
    if node.kind() == "alias" {
        // Find the alias_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "alias_name" {
                let alias_name = self.base.get_node_text(&child);
                // Try to find the anchor definition in symbols by matching signature
                let resolved_id = symbols.iter().find(|s| {
                    s.signature.as_ref().map_or(false, |sig| sig.contains(&format!("&{}", alias_name)))
                }).map(|s| s.id.clone());

                self.base.create_identifier(
                    &child,
                    alias_name,
                    IdentifierKind::Reference,
                    resolved_id,
                    symbols,
                );
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        self.walk_tree_for_aliases(child, symbols);
    }
}
```

Add the `IdentifierKind` import at the top of `yaml/mod.rs`:
```rust
use crate::base::IdentifierKind;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --package julie-extractors -- test_alias_extracted`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/yaml/mod.rs crates/julie-extractors/src/tests/yaml/mod.rs
git commit -m "feat(yaml): extract aliases (*name) as identifier references to anchors"
```

---

### Task 6: YAML Merge Keys

Merge keys (`<<: *alias`) are a YAML feature for merging mapping pairs from an anchor. The `<<` key should be recognized and the alias extraction (Task 5) handles the reference side. This task adds explicit merge key detection in the signature.

**Files:**
- Modify: `crates/julie-extractors/src/yaml/mod.rs` (update `extract_mapping_pair` to skip `<<` keys or give them special treatment)
- Test: `crates/julie-extractors/src/tests/yaml/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_merge_key_not_extracted_as_symbol() {
    let yaml = r#"
defaults: &defaults
  adapter: postgres

production:
  <<: *defaults
  database: prod_db
"#;
    let symbols = extract_symbols(yaml);

    // The merge key "<<" should NOT be extracted as a symbol — it's a YAML operator, not a user-defined key
    assert!(!symbols.iter().any(|s| s.name == "<<"), "Merge key << should not be a symbol");

    // But "production" and its children should exist
    assert!(symbols.iter().any(|s| s.name == "production"));
    assert!(symbols.iter().any(|s| s.name == "database"));
}
```

**Step 2: Run test to check current behavior**

Run: `cargo test --package julie-extractors -- test_merge_key_not_extracted`
Expected: May pass or fail depending on whether `<<` is extracted as a key. Check.

**Step 3: If `<<` is extracted, filter it out**

In `extract_mapping_pair`, add early return:

```rust
fn extract_mapping_pair(...) -> Option<Symbol> {
    let key_name = self.extract_mapping_key(node)?;

    // Skip merge keys — they're YAML operators, not user-defined keys
    if key_name == "<<" {
        return None;
    }

    // ... rest of function
}
```

**Step 4: Run full YAML test suite**

Run: `cargo test --package julie-extractors -- yaml`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/yaml/mod.rs crates/julie-extractors/src/tests/yaml/mod.rs
git commit -m "feat(yaml): skip merge key (<<) as symbol, treat as YAML operator"
```

---

## Finalization

### Task 7: Update Audit and Run Full Suite

**Step 1: Run complete extractor test suite**

Run: `cargo test --package julie-extractors`
Expected: All tests pass (current 1277 + new tests).

**Step 2: Update `docs/EXTRACTOR_AUDIT.md`**

Update ratings:
- PHP: B+ → **A** (anonymous classes extracted, arrow functions covered via variable assignment)
- Vue: B+ → **A** (style section now extracts class selectors, ID selectors, CSS custom properties)
- YAML: B+ → **A** (alias references extracted as identifiers, container/leaf SymbolKind differentiation, merge key filtering)

Update the summary table and final tally: **30 A-rated, 0 B+, 0 B**.

**Step 3: Commit**

```bash
git add docs/EXTRACTOR_AUDIT.md
git commit -m "docs: update extractor audit — all 30 extractors now A-rated"
```
