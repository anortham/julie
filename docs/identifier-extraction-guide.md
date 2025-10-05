# Identifier Extraction Implementation Guide

**Status**: 1 of 26 languages complete (Rust ✅)
**Goal**: Implement identifier extraction for remaining 25 languages
**Priority**: C# → Python → JavaScript → TypeScript → Java → Go → Others

---

## ✅ Reference Implementation: Rust (COMPLETE)

**File**: `~/Source/julie/src/extractors/rust.rs` (lines 1244-1323)

This is the **proven working pattern** to follow for all languages.

### What Was Implemented

**4 Methods Added:**

1. **`extract_identifiers()`** - Main entry point (lines 1250-1259)
2. **`walk_tree_for_identifiers()`** - Recursive tree walker (lines 1261-1273)
3. **`extract_identifier_from_node()`** - Node type matcher (lines 1275-1303)
4. **`find_containing_symbol_id()`** - File-scoped symbol lookup (lines 1305-1323)

### Code Structure

```rust
// 1. MAIN ENTRY POINT
pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();
    self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);
    self.base.identifiers.clone()
}

// 2. RECURSIVE WALKER
fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
    self.extract_identifier_from_node(node, symbol_map);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        self.walk_tree_for_identifiers(child, symbol_map);
    }
}

// 3. NODE TYPE MATCHER (LANGUAGE-SPECIFIC)
fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
    match node.kind() {
        "call_expression" => {
            // Extract function call
            if let Some(func_node) = node.child_by_field_name("function") {
                let name = self.base.get_node_text(&func_node);
                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                self.base.create_identifier(&func_node, name, IdentifierKind::Call, containing_symbol_id);
            }
        }
        "field_expression" => {
            // Extract member access (object.field)
            if let Some(field_node) = node.child_by_field_name("field") {
                let name = self.base.get_node_text(&field_node);
                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                self.base.create_identifier(&field_node, name, IdentifierKind::MemberAccess, containing_symbol_id);
            }
        }
        _ => {}
    }
}

// 4. SYMBOL FINDER (SAME FOR ALL LANGUAGES - DO NOT MODIFY)
fn find_containing_symbol_id(&self, node: Node, symbol_map: &HashMap<String, &Symbol>) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE
    let file_symbols: Vec<Symbol> = symbol_map.values()
        .filter(|s| s.file_path == self.base.file_path)  // FILE-SCOPED!
        .map(|&s| s.clone())
        .collect();
    self.base.find_containing_symbol(&node, &file_symbols).map(|s| s.id.clone())
}
```

---

## 🎯 Implementation Checklist (Per Language)

### Step 1: Find Tree-Sitter Node Types (5 min)

Each language has different node types. Find them using tree-sitter CLI:

```bash
# Example for C#:
echo 'class Foo { void Bar() { Baz(); this.field = 1; } }' > /tmp/test.cs
tree-sitter parse /tmp/test.cs

# Look for nodes like:
# - invocation_expression (function calls)
# - member_access_expression (field access)
# - type_argument_list (generic types)
```

**Common Node Types by Language:**

| Language | Call Expression | Member Access | Type Usage |
|----------|----------------|---------------|------------|
| **C#** | `invocation_expression` | `member_access_expression` | `type_argument_list` |
| **TypeScript/JavaScript** | `call_expression` | `member_expression` | `type_annotation` |
| **Python** | `call` | `attribute` | `type` |
| **Java** | `method_invocation` | `field_access` | `type_identifier` |
| **Go** | `call_expression` | `selector_expression` | `type_identifier` |
| **C/C++** | `call_expression` | `field_expression` | `type_identifier` |

### Step 2: Copy Rust Pattern (10 min)

1. **Copy the 4 methods** from `rust.rs` to your language extractor (e.g., `csharp.rs`)
2. **Update imports** at the top of the file:
   ```rust
   use crate::extractors::base::{self, Identifier, IdentifierKind, Symbol};
   use std::collections::HashMap;
   ```

3. **Update `extract_identifier_from_node()`** with language-specific node types:
   ```rust
   match node.kind() {
       "invocation_expression" => { /* C# function call */ }
       "member_access_expression" => { /* C# member access */ }
       _ => {}
   }
   ```

4. **Keep `find_containing_symbol_id()` IDENTICAL** - DO NOT MODIFY THIS METHOD

### Step 3: Test Extraction (5 min)

```bash
# Create test file
echo 'class Test { void Foo() { Bar(); this.field = 1; } }' > /tmp/test.{ext}

# Run extraction
~/Source/julie/target/release/julie-extract bulk \
  --directory /tmp \
  --output-db /tmp/test.db

# Verify identifiers extracted
sqlite3 /tmp/test.db "SELECT COUNT(*) FROM identifiers"
sqlite3 /tmp/test.db "SELECT name, kind, start_line FROM identifiers"
```

**Expected output:**
```
Bar|call|1
field|member_access|1
```

---

## 📋 Language-Specific Node Types

### C# (Priority 1 - 5,252 symbols)

```rust
match node.kind() {
    "invocation_expression" => {
        // method calls: Foo(), obj.Bar()
        if let Some(func) = node.child_by_field_name("function") {
            let name = self.base.get_node_text(&func);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&func, name, IdentifierKind::Call, containing);
        }
    }
    "member_access_expression" => {
        // field/property access: obj.field
        if let Some(member) = node.child_by_field_name("name") {
            let name = self.base.get_node_text(&member);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&member, name, IdentifierKind::MemberAccess, containing);
        }
    }
    "type_argument_list" => {
        // Generic types: List<T>
        for child in node.named_children(&mut node.walk()) {
            if child.kind() == "type_identifier" {
                let name = self.base.get_node_text(&child);
                let containing = self.find_containing_symbol_id(node, symbol_map);
                self.base.create_identifier(&child, name, IdentifierKind::TypeUsage, containing);
            }
        }
    }
    _ => {}
}
```

### Python (Priority 2 - 1,162 symbols)

```rust
match node.kind() {
    "call" => {
        // function calls: foo(), obj.method()
        if let Some(func) = node.child_by_field_name("function") {
            let name = self.base.get_node_text(&func);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&func, name, IdentifierKind::Call, containing);
        }
    }
    "attribute" => {
        // member access: obj.attr
        if let Some(attr) = node.child_by_field_name("attribute") {
            let name = self.base.get_node_text(&attr);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&attr, name, IdentifierKind::MemberAccess, containing);
        }
    }
    _ => {}
}
```

### JavaScript/TypeScript (Priority 3 - 1,077 symbols combined)

```rust
match node.kind() {
    "call_expression" => {
        // function calls: foo(), obj.method()
        if let Some(func) = node.child_by_field_name("function") {
            let name = self.base.get_node_text(&func);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&func, name, IdentifierKind::Call, containing);
        }
    }
    "member_expression" => {
        // member access: obj.property
        if let Some(prop) = node.child_by_field_name("property") {
            let name = self.base.get_node_text(&prop);
            let containing = self.find_containing_symbol_id(node, symbol_map);
            self.base.create_identifier(&prop, name, IdentifierKind::MemberAccess, containing);
        }
    }
    _ => {}
}
```

---

## 🚀 After Implementation

### Update julie-codesearch to Extract All Languages

**File**: `~/Source/julie/src/bin/codesearch.rs` (line 503)

**BEFORE** (only Rust):
```rust
// Only extract from Rust files for now (other languages pending)
if !file_path.to_string_lossy().ends_with(".rs") {
    return;
}
```

**AFTER** (all languages):
```rust
// Extract identifiers for all supported languages
// (No filter - let tree-sitter handle language detection)
```

### Rebuild and Test

```bash
cd ~/Source/julie
cargo build --release --bin julie-codesearch

# Copy to CodeSearch
cp target/release/julie-codesearch ~/Source/coa-codesearch-mcp/bin/julie-binaries/julie-codesearch-macos-arm64

# Test on CodeSearch project
rm -rf ~/Source/coa-codesearch-mcp/.coa/codesearch/indexes/*
# Restart CodeSearch, trigger reindex
```

**Expected Results:**
- **Before**: 126 identifiers (Rust only)
- **After**: ~200,000+ identifiers (all languages)
  - C# (5,252 symbols → ~136,000 identifiers)
  - Python (1,162 symbols → ~30,000 identifiers)
  - JavaScript (1,048 symbols → ~27,000 identifiers)

---

## 📝 Per-Language Progress Tracking

- [ ] **C#** (5,252 symbols) - Priority 1
- [ ] **Python** (1,162 symbols) - Priority 2
- [ ] **JavaScript** (1,048 symbols) - Priority 3
- [ ] **TypeScript** (29 symbols) - Priority 4
- [ ] **Java** (58 symbols) - Priority 5
- [ ] **Go** (77 symbols) - Priority 6
- [ ] Bash
- [ ] C
- [ ] C++
- [ ] CSS
- [ ] Dart
- [ ] GDScript
- [ ] HTML
- [ ] Kotlin
- [ ] Lua
- [ ] PHP
- [ ] PowerShell
- [ ] Razor
- [ ] Ruby
- [ ] SQL
- [ ] Swift
- [ ] Vue
- [ ] Zig

---

## ⚠️ Common Pitfalls

1. **DON'T modify `find_containing_symbol_id()`** - It's already fixed for file-scoped lookups
2. **DON'T forget imports** - Add `Identifier`, `IdentifierKind` to imports
3. **DON'T guess node types** - Use `tree-sitter parse` to verify
4. **DO keep `find_containing_symbol_id()` identical across all languages**

---

## 🎯 Success Criteria

For each language:
- ✅ Tests pass (identifiers extracted)
- ✅ Identifiers have correct `kind` (call, member_access, etc.)
- ✅ `containing_symbol_id` links to symbols in same file
- ✅ No false positives (comments/strings ignored by tree-sitter)
