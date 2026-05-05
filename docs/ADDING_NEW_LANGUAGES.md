# Adding New Language Support to Julie

This document describes all the locations that must be updated when adding a new programming language to Julie's extractors.

Julie currently supports 34 languages via tree-sitter. All extractor code lives in the `crates/julie-extractors/` workspace crate.

## Quick Checklist

When adding a new language (e.g., "mylang"), you MUST update these files:

- [ ] **1. Create extractor module** - `crates/julie-extractors/src/mylang.rs` or `crates/julie-extractors/src/mylang/mod.rs`
- [ ] **2. Declare module** - Add `pub mod mylang;` to `crates/julie-extractors/src/lib.rs`
- [ ] **3. Add parser dependency** - Add to `crates/julie-extractors/Cargo.toml` and `docs/TREE_SITTER_UPGRADES.md`
- [ ] **4. Add `LanguageSpec` row** - Add name, aliases, extensions, parser crate, capabilities, parser function, and doc comment styles in `crates/julie-extractors/src/language_spec.rs`
- [ ] **5. Add language detection** - Verify `crates/julie-extractors/src/language.rs` surfaces the new spec correctly
- [ ] **6. Register canonical extraction** - Add the registry extraction helper and entry in `crates/julie-extractors/src/registry.rs`
- [ ] **7. Add capability matrix row** - Update `fixtures/extraction/capabilities.json`
- [ ] **8. Add golden fixture** - Add at least one production-path case under `fixtures/extraction/mylang/`
- [ ] **9. Add focused tests** - Create real behavior tests in `crates/julie-extractors/src/tests/mylang/`
- [ ] **10. Add real-world contract** - Add a representative case in `src/tests/integration/real_world_contract.rs` when a real fixture exists

## Detailed Instructions

### 1. Create Extractor Module

**Location**: `crates/julie-extractors/src/mylang.rs` or `crates/julie-extractors/src/mylang/mod.rs`

Create a new extractor implementing the BaseExtractor trait:

```rust
use crate::base::{BaseExtractor, Symbol, Relationship};
use std::path::Path;

pub struct MyLangExtractor {
    base: BaseExtractor,
}

impl MyLangExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }

    pub fn extract_symbols(&mut self, tree: &tree_sitter::Tree) -> Vec<Symbol> {
        // Implementation...
    }

    pub fn extract_relationships(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        // Implementation (or return Vec::new() for non-code files)
    }
}
```

**For documentation/config languages** (like markdown, json, toml):
- Implement `extract_symbols()` only
- Return `Vec::new()` for relationships

### 2. Declare Module

**Location**: `crates/julie-extractors/src/lib.rs`

Add your module to the list (alphabetically):

```rust
pub mod mylang;  // Add this line
```

Also re-export any public types if needed:
```rust
pub use mylang::MyLangExtractor;
```

### 3. Add Tree-Sitter Parser

**Location**: `crates/julie-extractors/Cargo.toml`

Add the tree-sitter parser crate:

```toml
[dependencies]
tree-sitter-mylang = "x.y.z"  # Verify latest on crates.io first.
```

Also add a row to `docs/TREE_SITTER_UPGRADES.md` with the previous version, current version, latest checked version, decision, and evidence. Git parser dependencies must use a pinned `rev`.

### 4. Add LanguageSpec

**Location**: `crates/julie-extractors/src/language_spec.rs`

Add a `LanguageSpec` row. This is the canonical source for supported language names, aliases, extensions, parser crates, parser functions, doc comment styles, and capability flags.

The language name must be stable and must match the registry entry, capability matrix row, golden fixture directory, and tool-facing language string.

### 5. Add Language Detection

**Location**: `crates/julie-extractors/src/language.rs`

Language detection is spec-driven. Confirm the extension mapping is exposed through `detect_language_from_extension`:

```rust
pub fn detect_language_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        // ... existing cases ...

        // MyLang
        "ml" | "mylang" => Some("mylang"),

        // ... rest of cases ...
        _ => None,
    }
}
```

Confirm the parser registration is exposed through `get_tree_sitter_language`:

```rust
pub fn get_tree_sitter_language(language: &str) -> Result<tree_sitter::Language> {
    match language {
        // ... existing cases ...
        "mylang" => Ok(tree_sitter_mylang::LANGUAGE.into()),
        // ... rest of cases ...
    }
}
```

Also confirm `"mylang"` appears in `supported_languages()` and its extensions appear in `supported_extensions()`.

**Note**: `src/tools/workspace/language.rs` delegates directly to `detect_language_from_extension` from this module. You do NOT need to edit the workspace language.rs file.

**CRITICAL**: The language string returned here MUST match the registry entry in step 4.

### 6. Register in the Canonical Registry

**Location**: `crates/julie-extractors/src/registry.rs`

Add a registry entry that constructs the language extractor through the canonical parse-and-dispatch pipeline. The registry entry returns `ExtractionResults`:

```rust
fn extract_mylang(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut extractor = crate::mylang::MyLangExtractor::new(
        "mylang".to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = extractor.extract_symbols(tree);
    let relationships = extractor.extract_relationships(tree, &symbols);
    let identifiers = extractor.extract_identifiers(tree, &symbols);
    let pending_relationships = extractor.get_pending_relationships();
    let structured_pending_relationships = extractor.get_structured_pending_relationships();
    let types = convert_types_map(extractor.infer_types(&symbols), "mylang");

    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types,
    })
}

register_language!(
    entry_mylang,
    "mylang",
    LanguageCapabilities {
        symbols: true,
        relationships: true,
        pending_relationships: true,
        identifiers: true,
        types: true,
    },
    extract_mylang
);
```

**Pattern to follow**: Look at existing registry entries plus their `extract_<language>()` helpers. `extract_canonical()` and `ExtractorManager::extract_all()` are the supported public entrypoints; the old parsed-tree factory helper is test-only.

### 7. Add Capability Matrix Row

**Location**: `fixtures/extraction/capabilities.json`

Add exactly one row for the new registry language. The capability flags must match `capabilities_for_language()`, and the `parser_crate` must match `LanguageSpec`.

The capability matrix tests fail if a registry entry has no row or if a row has no golden fixture.

### 8. Add Golden Fixture

**Location**: `fixtures/extraction/mylang/`

Add at least one fixture that runs through `extract_canonical`, not a direct extractor helper. Include meaningful expected output for symbols, relationships, pending relationships, identifiers, types, parse diagnostics, signatures, doc comments, and parent links when the language supports them.

Use:

```bash
UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden
cargo nextest run -p julie-extractors golden
```

Review the generated expected JSON. Do not accept missing symbols or empty relationships just because the file is small.

### 9. Create Focused Tests

**Location**: `crates/julie-extractors/src/tests/mylang/`

Create a test module:

```rust
// crates/julie-extractors/src/tests/mylang/mod.rs

use crate::base::SymbolKind;
use crate::mylang::MyLangExtractor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_function() {
        let code = "function hello() { return 'world'; }";
        // Test implementation...
    }

    // Add focused tests covering:
    // - Basic constructs (functions, classes, variables)
    // - Edge cases (nested structures, special characters)
    // - Real-world code samples
    // - Error handling (malformed code)
}
```

Register the test module in `crates/julie-extractors/src/tests/mod.rs`:

```rust
pub mod mylang;  // Add this
```

### 10. Add Real-World Contract

**Location**: `src/tests/integration/real_world_contract.rs`

When a representative fixture exists under `fixtures/real-world/`, add a contract row for the new language. Keep expected values stable and high-signal: at least one key symbol and one representative identifier when the extractor supports identifiers.

## Validation

Run the narrowest tests while developing, then run the extractor gates before handoff:

```bash
cargo nextest run -p julie-extractors <exact_test_name>
cargo nextest run -p julie-extractors golden
cargo nextest run -p julie-extractors capability_matrix
cargo xtask test bucket parser-upgrade
```

The `parser-upgrade` bucket is required for any parser dependency change, parser git revision change, grammar-driven extractor adaptation, or golden expected-output update caused by parser drift.

### Manual Testing

1. **Create test file**:
   ```bash
   echo "test code" > test.ml
   ```

2. **Index workspace** via MCP and verify symbols appear:
   ```bash
   sqlite3 .julie/indexes/primary_*/db/symbols.db "SELECT * FROM symbols WHERE file_path LIKE '%test.ml%';"
   ```

## Common Mistakes

### Mismatch Between LanguageSpec and Registry

```rust
// language_spec.rs
"ml" => Some("my-lang"),  // Returns "my-lang"

// registry.rs
"mylang" => { ... }  // Expects "mylang" - MISMATCH!
```

**Fix**: Use one stable language string across `LanguageSpec`, registry, capability matrix, golden fixture directories, and tests.

### Missing Registry Entry

Adding language detection without registry registration results in:
```
Error: No extractor available for language 'mylang' (file: test.ml)
```

**Fix**: Always add `LanguageSpec`, language detection, registry extraction, capability matrix, and golden fixture coverage together.

### Using Old Return Type

The registry returns `ExtractionResults`, not a `(Vec<Symbol>, Vec<Relationship>)` tuple.

```rust
// WRONG
let (symbols, relationships) = extract_symbols_and_relationships(...)?;

// CORRECT
let results = extract_symbols_and_relationships(...)?;
let symbols = results.symbols;
let relationships = results.relationships;
```

**Last Updated**: 2026-05-05
**Maintained By**: Julie Development Team
