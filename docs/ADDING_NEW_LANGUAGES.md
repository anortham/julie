# Adding New Language Support to Julie

This document describes all the locations that must be updated when adding a new programming language to Julie's extractors.

## Quick Checklist

When adding a new language (e.g., "mylang"), you MUST update these files:

- [ ] **1. Create extractor module** - `src/extractors/mylang.rs` or `src/extractors/mylang/mod.rs`
- [ ] **2. Declare module** - Add `pub mod mylang;` to `src/extractors/mod.rs`
- [ ] **3. Add language detection** - Map file extensions in `src/tools/workspace/language.rs`
- [ ] **4. Register in factory** - Add match arm in `src/extractors/factory.rs`
- [ ] **5. Add tree-sitter parser** - Add to `Cargo.toml` and `src/language/mod.rs`
- [ ] **6. Create comprehensive tests** - At least 100 test cases in `src/tests/extractors/mylang/`

## Detailed Instructions

### 1. Create Extractor Module

**Location**: `src/extractors/mylang.rs` or `src/extractors/mylang/mod.rs`

Create a new extractor implementing the BaseExtractor trait:

```rust
use crate::extractors::base::{BaseExtractor, Symbol, Relationship};
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

### 2. Declare Module in Extractors

**Location**: `src/extractors/mod.rs`

Add your module to the list:

```rust
pub mod mylang;  // Add this line alphabetically
```

### 3. Add Language Detection

**Location**: `src/tools/workspace/language.rs`

Map file extensions to your language string:

```rust
pub(crate) fn detect_language(&self, file_path: &Path) -> String {
    let extension = file_path.extension()...;

    match extension.to_lowercase().as_str() {
        // ... existing cases ...

        // MyLang (add your extension mappings)
        "ml" | "mylang" => "mylang".to_string(),

        // ... rest of cases ...
    }
}
```

**CRITICAL**: The language string returned here MUST match the factory match arm (step 4).

### 4. Register in Extractor Factory

**Location**: `src/extractors/factory.rs`

Add a match arm in `extract_symbols_and_relationships()`:

```rust
pub fn extract_symbols_and_relationships(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    language: &str,
    workspace_root: &Path,
) -> Result<(Vec<Symbol>, Vec<Relationship>), anyhow::Error> {
    let (symbols, relationships) = match language {
        // ... existing cases ...

        "mylang" => {
            let mut extractor = crate::extractors::mylang::MyLangExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }

        // OR for documentation/config languages:
        "mylang" => {
            let mut extractor = crate::extractors::mylang::MyLangExtractor::new(...);
            let symbols = extractor.extract_symbols(tree);
            // MyLang is documentation/config - no code relationships
            (symbols, Vec::new())
        }

        // ... default case ...
    };
}
```

**Pattern to follow**: Look at existing languages (rust, typescript, etc.) for code, or markdown/json/toml for non-code.

### 5. Add Tree-Sitter Parser

**Location**: `Cargo.toml` and `src/language/mod.rs`

#### Cargo.toml Dependencies

Add the tree-sitter parser crate:

```toml
[dependencies]
tree-sitter-mylang = "x.y.z"  # Find latest version on crates.io
```

#### Language Module

**Location**: `src/language/mod.rs`

Register the parser:

```rust
pub fn get_tree_sitter_language(language: &str) -> Result<tree_sitter::Language> {
    let lang = match language {
        // ... existing cases ...
        "mylang" => tree_sitter_mylang::language(),
        // ... rest of cases ...
    };
    Ok(lang)
}
```

### 6. Create Comprehensive Tests

**Location**: `src/tests/extractors/mylang/`

Create a test module with extensive coverage:

```rust
// src/tests/extractors/mylang/mod.rs

use crate::extractors::base::SymbolKind;
use crate::extractors::mylang::MyLangExtractor;

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

Register the test module in `src/tests/mod.rs`:

```rust
pub mod extractors {
    // ... existing extractors ...
    pub mod mylang;  // Add this
}
```

## Validation

### Factory Consistency Test

The factory module has a built-in test that validates all languages are registered:

```bash
cargo test factory_consistency_tests --lib
```

This test will **FAIL** if:
- A language is in `ExtractorManager::supported_languages()` but missing from factory
- A language in factory returns "No extractor available" error

### Manual Testing

1. **Create test file**:
   ```bash
   echo "test code" > test.ml
   ```

2. **Index workspace**:
   ```bash
   cargo run --release
   # Use MCP to index workspace
   ```

3. **Verify extraction**:
   ```bash
   # Check database for symbols
   sqlite3 .julie/indexes/primary_*/db/symbols.db "SELECT * FROM symbols WHERE file_path LIKE '%test.ml%';"
   ```

## Common Mistakes

### ❌ Mismatch Between Language Detection and Factory

```rust
// language.rs
"ml" => "my-lang".to_string(),  // Returns "my-lang"

// factory.rs
"mylang" => { ... }  // Expects "mylang" - MISMATCH!
```

**Fix**: Use consistent language strings across both files.

### ❌ Missing Factory Case

Adding language detection without factory registration results in:
```
Error: No extractor available for language 'mylang' (file: test.ml)
```

**Fix**: Always add both language detection AND factory case together.

### ❌ Calling extract_relationships on Non-Code Extractors

Documentation/config extractors may not have relationship extraction:

```rust
// WRONG
"markdown" => {
    let symbols = extractor.extract_symbols(tree);
    let relationships = extractor.extract_relationships(tree, &symbols);  // Method doesn't exist!
    (symbols, relationships)
}

// CORRECT
"markdown" => {
    let symbols = extractor.extract_symbols(tree);
    (symbols, Vec::new())  // No relationships for documentation
}
```

## Recent Examples

### Markdown, JSON, TOML (Documentation Languages)

Added in 2025-11-05 for RAG POC:

1. ✅ Created extractors: `src/extractors/{markdown,json,toml}.rs`
2. ✅ Declared modules: `src/extractors/mod.rs`
3. ✅ Added detection: `src/tools/workspace/language.rs:110-113`
4. ✅ Registered factory: `src/extractors/factory.rs:336-368`
5. ✅ Tests: `src/tests/extractors/{markdown,json,toml}/`

**Key insight**: These are documentation/config languages, so they return `Vec::new()` for relationships.

## Future Improvements

**Centralization Opportunity**: Consider consolidating language registration into a single macro or configuration file to eliminate the need to update multiple locations.

Example conceptual approach:
```rust
// Hypothetical future API
register_language! {
    name: "mylang",
    extensions: ["ml", "mylang"],
    tree_sitter: tree_sitter_mylang::language(),
    extractor: MyLangExtractor,
    has_relationships: true,
}
```

This would automatically generate:
- Language detection mapping
- Factory match arm
- Language module registration

For now, follow the 6-step checklist above until centralization is implemented.

---

**Last Updated**: 2025-11-05
**Maintained By**: Julie Development Team
