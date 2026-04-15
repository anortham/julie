# Adding New Language Support to Julie

This document describes all the locations that must be updated when adding a new programming language to Julie's extractors.

Julie currently supports 33 languages via tree-sitter. All extractor code lives in the `crates/julie-extractors/` workspace crate.

## Quick Checklist

When adding a new language (e.g., "mylang"), you MUST update these files:

- [ ] **1. Create extractor module** - `crates/julie-extractors/src/mylang.rs` or `crates/julie-extractors/src/mylang/mod.rs`
- [ ] **2. Declare module** - Add `pub mod mylang;` to `crates/julie-extractors/src/lib.rs`
- [ ] **3. Add language detection** - Add extension mapping in `crates/julie-extractors/src/language.rs` (both `detect_language_from_extension` and `get_tree_sitter_language`)
- [ ] **4. Register in factory** - Add match arm in `crates/julie-extractors/src/factory.rs`
- [ ] **5. Add tree-sitter parser** - Add to `crates/julie-extractors/Cargo.toml`
- [ ] **6. Create comprehensive tests** - At least 100 test cases in `crates/julie-extractors/src/tests/mylang/`

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

### 3. Add Language Detection

**Location**: `crates/julie-extractors/src/language.rs`

Add the file extension mapping in `detect_language_from_extension`:

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

Add the tree-sitter parser registration in `get_tree_sitter_language`:

```rust
pub fn get_tree_sitter_language(language: &str) -> Result<tree_sitter::Language> {
    match language {
        // ... existing cases ...
        "mylang" => Ok(tree_sitter_mylang::LANGUAGE.into()),
        // ... rest of cases ...
    }
}
```

Also add `"mylang"` to the `supported_languages()` list and its extensions to `supported_extensions()`.

**Note**: `src/tools/workspace/language.rs` delegates directly to `detect_language_from_extension` from this module. You do NOT need to edit the workspace language.rs file.

**CRITICAL**: The language string returned here MUST match the registry entry in step 4.

### 4. Register in the Canonical Registry

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

### 5. Add Tree-Sitter Parser

**Location**: `crates/julie-extractors/Cargo.toml`

Add the tree-sitter parser crate:

```toml
[dependencies]
tree-sitter-mylang = "x.y.z"  # Find latest version on crates.io
```

The `get_tree_sitter_language` registration in step 3 handles the Rust-side binding.

### 6. Create Comprehensive Tests

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

    // Add 100+ tests covering:
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

## Validation

### Factory Consistency Test

The factory module has a built-in test that validates all languages are registered:

```bash
cargo test -p julie-extractors factory_consistency_tests
```

This test will **FAIL** if:
- A language is in `ExtractorManager::supported_languages()` but missing from the factory
- A language in the factory returns "No extractor available" error

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

### Mismatch Between Language Detection and Factory

```rust
// language.rs
"ml" => Some("my-lang"),  // Returns "my-lang"

// factory.rs
"mylang" => { ... }  // Expects "mylang" - MISMATCH!
```

**Fix**: Use consistent language strings across all three locations.

### Missing Factory Case

Adding language detection without factory registration results in:
```
Error: No extractor available for language 'mylang' (file: test.ml)
```

**Fix**: Always add both language detection AND factory case together.

### Using Old Return Type

The factory returns `ExtractionResults`, not a `(Vec<Symbol>, Vec<Relationship>)` tuple.

```rust
// WRONG
let (symbols, relationships) = extract_symbols_and_relationships(...)?;

// CORRECT
let results = extract_symbols_and_relationships(...)?;
let symbols = results.symbols;
let relationships = results.relationships;
```

## Future Improvements

**Centralization Opportunity**: Consider consolidating language registration into a single macro or configuration file to eliminate the need to update multiple locations.

---

**Last Updated**: 2026-03-25
**Maintained By**: Julie Development Team
