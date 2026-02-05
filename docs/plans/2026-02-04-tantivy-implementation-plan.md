# Replace FTS5 with Tantivy + CodeTokenizer — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace Julie's FTS5 search with Tantivy + code-aware CodeTokenizer from Razorback, remove ORT/embeddings/HNSW, and complete unfinished language config features.

**Architecture:** SQLite keeps structural/relational data (symbols, files, identifiers, relations). Tantivy becomes the sole search engine with a custom CodeTokenizer that splits CamelCase/snake_case at index time. One Tantivy index per workspace, stored at `.julie/indexes/{workspace_id}/tantivy/`. Language configs embedded in binary via `include_str!`.

**Tech Stack:** Rust, Tantivy 0.22, tree-sitter (existing), SQLite (existing minus FTS5)

**Design Doc:** `docs/plans/2026-02-04-tantivy-search-engine-design.md`

**Source Code:** Port from `~/Source/razorback/crates/razorback-search/` and `~/Source/razorback/crates/razorback-languages/`

---

## Phase 1: Add Tantivy Alongside FTS5 (Additive Only)

No existing behavior changes. FTS5 continues working. We're only adding new code.

---

### Task 1: Add Tantivy Dependency and Create Module Scaffolding

**Files:**
- Modify: `Cargo.toml` (root — add tantivy dependency)
- Create: `src/search/mod.rs`
- Modify: `src/lib.rs:7` (add `pub mod search` declaration)

**Step 1: Add tantivy to Cargo.toml**

In `Cargo.toml`, add after the `rusqlite` dependency (around line 83):

```toml
# Full-text search engine with custom code tokenizer
tantivy = "0.22"
```

**Step 2: Create src/search/mod.rs**

```rust
//! Tantivy-based code search engine.
//!
//! Replaces FTS5 with a code-aware search using custom tokenization
//! that understands CamelCase, snake_case, and language-specific operators.

mod error;
pub mod index;
pub mod language_config;
pub mod query;
pub mod schema;
pub mod scoring;
pub mod tokenizer;

pub use error::*;
pub use index::*;
pub use language_config::LanguageConfigs;
pub use schema::*;
pub use tokenizer::CodeTokenizer;
```

**Step 3: Create src/search/error.rs**

Port from `~/Source/razorback/crates/razorback-search/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Tantivy error: {0}")]
    TantivyError(#[from] tantivy::TantivyError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Index not found at path: {0}")]
    IndexNotFound(String),
}

pub type Result<T> = std::result::Result<T, SearchError>;
```

**Step 4: Add module declaration to src/lib.rs**

Add `pub mod search;` after the `pub mod embeddings;` line (line 7). It will sit alongside embeddings until Phase 3 removes embeddings.

```rust
pub mod search;
```

**Step 5: Verify it compiles**

Run: `cargo check 2>&1 | head -20`

Expected: Compilation warnings about empty/missing modules (schema, index, etc.) but no errors on the error module. The other modules don't exist yet — we'll create placeholder empty files to satisfy `mod` declarations.

Create empty placeholder files so compilation succeeds:
- `src/search/schema.rs` — empty file
- `src/search/index.rs` — empty file
- `src/search/tokenizer.rs` — empty file
- `src/search/query.rs` — empty file
- `src/search/scoring.rs` — empty file
- `src/search/language_config.rs` — empty file

Run: `cargo check 2>&1 | head -20`

Expected: Clean compilation (possibly with unused warnings).

**Step 6: Commit**

```bash
git add src/search/ src/lib.rs Cargo.toml Cargo.lock
git commit -m "feat(search): add tantivy dependency and search module scaffolding"
```

---

### Task 2: Port Language Config Types and Embedded Configs

**Files:**
- Create: `src/search/language_config.rs` (replace empty placeholder)
- Copy: `~/Source/razorback/languages/*.toml` → `languages/` directory in Julie
- Test: `src/tests/tools/search/tantivy_language_config_tests.rs`

**Step 1: Write the failing test**

Create `src/tests/tools/search/tantivy_language_config_tests.rs`:

```rust
//! Tests for language configuration loading.

use crate::search::language_config::{LanguageConfig, LanguageConfigs};

#[test]
fn test_load_embedded_configs() {
    let configs = LanguageConfigs::load_embedded();
    // We have 30 supported languages
    assert!(configs.len() >= 28, "Expected at least 28 languages, got {}", configs.len());
}

#[test]
fn test_rust_config_has_expected_patterns() {
    let configs = LanguageConfigs::load_embedded();
    let rust = configs.get("rust").expect("rust config should exist");
    assert!(rust.tokenizer.preserve_patterns.contains(&"::".to_string()));
    assert!(rust.tokenizer.preserve_patterns.contains(&"->".to_string()));
    assert!(rust.tokenizer.naming_styles.contains(&"snake_case".to_string()));
}

#[test]
fn test_typescript_config_has_expected_patterns() {
    let configs = LanguageConfigs::load_embedded();
    let ts = configs.get("typescript").expect("typescript config should exist");
    assert!(ts.tokenizer.preserve_patterns.contains(&"?.".to_string()));
    assert!(ts.variants.strip_prefixes.contains(&"I".to_string()));
}

#[test]
fn test_config_defaults_for_optional_sections() {
    let toml_str = r#"
[tokenizer]
preserve_patterns = ["::"]
naming_styles = ["snake_case"]
"#;
    let config: LanguageConfig = toml::from_str(toml_str).unwrap();
    assert!(config.variants.strip_prefixes.is_empty());
    assert!(config.variants.strip_suffixes.is_empty());
    assert!(config.scoring.important_patterns.is_empty());
}

#[test]
fn test_all_preserve_patterns_collected() {
    let configs = LanguageConfigs::load_embedded();
    let all_patterns = configs.all_preserve_patterns();
    // Should include patterns from multiple languages
    assert!(all_patterns.contains(&"::".to_string()), "Missing Rust ::");
    assert!(all_patterns.contains(&"?.".to_string()), "Missing TS ?.");
    assert!(all_patterns.contains(&":=".to_string()), "Missing Go :=");
}
```

Register this test file in `src/tests/mod.rs` (add a `mod` declaration).

**Step 2: Run test to verify it fails**

Run: `cargo test tantivy_language_config --no-run 2>&1 | tail -5`

Expected: Compilation error — `LanguageConfig` and `LanguageConfigs` don't exist yet.

**Step 3: Copy language config TOML files from Razorback**

```bash
cp -r ~/Source/razorback/languages/ ./languages/
```

This brings over all 30+ `.toml` files. Verify: `ls languages/*.toml | wc -l` should show ~30.

**Step 4: Implement language_config.rs**

Write `src/search/language_config.rs`:

```rust
//! Language configuration for code-aware tokenization.
//!
//! Each language has a TOML config defining tokenizer patterns,
//! naming conventions, and scoring rules. Configs are embedded
//! in the binary via include_str!.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Configuration for a programming language's tokenization and matching rules.
#[derive(Debug, Clone, Deserialize)]
pub struct LanguageConfig {
    pub tokenizer: TokenizerConfig,
    #[serde(default)]
    pub variants: VariantsConfig,
    #[serde(default)]
    pub scoring: ScoringConfig,
}

/// Tokenizer configuration for code-aware text processing.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenizerConfig {
    #[serde(default)]
    pub preserve_patterns: Vec<String>,
    #[serde(default)]
    pub naming_styles: Vec<String>,
    #[serde(default)]
    pub meaningful_affixes: Vec<String>,
}

/// Configuration for generating naming variants.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VariantsConfig {
    #[serde(default)]
    pub strip_prefixes: Vec<String>,
    #[serde(default)]
    pub strip_suffixes: Vec<String>,
}

/// Configuration for search result scoring/boosting.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScoringConfig {
    #[serde(default)]
    pub important_patterns: Vec<String>,
}

/// Registry of all language configurations.
pub struct LanguageConfigs {
    configs: HashMap<String, LanguageConfig>,
}

impl LanguageConfigs {
    /// Load all embedded language configurations.
    pub fn load_embedded() -> Self {
        let mut configs = HashMap::new();

        // Each entry: (language_name, toml_content)
        let embedded: &[(&str, &str)] = &[
            ("bash", include_str!("../../languages/bash.toml")),
            ("c", include_str!("../../languages/c.toml")),
            ("cpp", include_str!("../../languages/cpp.toml")),
            ("csharp", include_str!("../../languages/csharp.toml")),
            ("css", include_str!("../../languages/css.toml")),
            ("dart", include_str!("../../languages/dart.toml")),
            ("gdscript", include_str!("../../languages/gdscript.toml")),
            ("go", include_str!("../../languages/go.toml")),
            ("html", include_str!("../../languages/html.toml")),
            ("java", include_str!("../../languages/java.toml")),
            ("javascript", include_str!("../../languages/javascript.toml")),
            ("json", include_str!("../../languages/json.toml")),
            ("kotlin", include_str!("../../languages/kotlin.toml")),
            ("lua", include_str!("../../languages/lua.toml")),
            ("markdown", include_str!("../../languages/markdown.toml")),
            ("php", include_str!("../../languages/php.toml")),
            ("powershell", include_str!("../../languages/powershell.toml")),
            ("python", include_str!("../../languages/python.toml")),
            ("qml", include_str!("../../languages/qml.toml")),
            ("r", include_str!("../../languages/r.toml")),
            ("razor", include_str!("../../languages/razor.toml")),
            ("regex", include_str!("../../languages/regex.toml")),
            ("ruby", include_str!("../../languages/ruby.toml")),
            ("rust", include_str!("../../languages/rust.toml")),
            ("sql", include_str!("../../languages/sql.toml")),
            ("swift", include_str!("../../languages/swift.toml")),
            ("toml", include_str!("../../languages/toml.toml")),
            ("typescript", include_str!("../../languages/typescript.toml")),
            ("vue", include_str!("../../languages/vue.toml")),
            ("yaml", include_str!("../../languages/yaml.toml")),
            ("zig", include_str!("../../languages/zig.toml")),
        ];

        for (name, content) in embedded {
            match toml::from_str::<LanguageConfig>(content) {
                Ok(config) => { configs.insert(name.to_string(), config); }
                Err(e) => {
                    tracing::warn!("Failed to parse language config for {}: {}", name, e);
                }
            }
        }

        Self { configs }
    }

    pub fn get(&self, language: &str) -> Option<&LanguageConfig> {
        self.configs.get(language)
    }

    pub fn len(&self) -> usize {
        self.configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Collect all preserve_patterns from all languages into a single union set.
    pub fn all_preserve_patterns(&self) -> Vec<String> {
        let mut patterns: HashSet<String> = HashSet::new();
        for config in self.configs.values() {
            for pattern in &config.tokenizer.preserve_patterns {
                patterns.insert(pattern.clone());
            }
        }
        let mut result: Vec<String> = patterns.into_iter().collect();
        // Sort by length descending so longer patterns match first
        result.sort_by_key(|b| std::cmp::Reverse(b.len()));
        result
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test tantivy_language_config -v 2>&1 | tail -20`

Expected: All 5 tests pass.

**Step 6: Commit**

```bash
git add languages/ src/search/language_config.rs src/tests/
git commit -m "feat(search): port language configs from razorback with embedded loading"
```

---

### Task 3: Port CodeTokenizer from Razorback

**Files:**
- Create: `src/search/tokenizer.rs` (replace empty placeholder)
- Test: `src/tests/tools/search/tantivy_tokenizer_tests.rs`

**Step 1: Write the failing tests**

Create `src/tests/tools/search/tantivy_tokenizer_tests.rs`:

```rust
//! Tests for the code-aware tokenizer.

use tantivy::tokenizer::{TextAnalyzer, TokenStream};
use crate::search::tokenizer::CodeTokenizer;
use crate::search::tokenizer::{split_camel_case, split_snake_case};

#[test]
fn test_camel_case_split() {
    assert_eq!(split_camel_case("UserService"), vec!["User", "Service"]);
}

#[test]
fn test_camel_case_acronym() {
    assert_eq!(split_camel_case("XMLParser"), vec!["XML", "Parser"]);
}

#[test]
fn test_camel_case_mixed() {
    let result = split_camel_case("getHTTPResponse");
    assert_eq!(result, vec!["get", "HTTP", "Response"]);
}

#[test]
fn test_snake_case_split() {
    assert_eq!(split_snake_case("user_service"), vec!["user", "service"]);
}

#[test]
fn test_snake_case_screaming() {
    assert_eq!(split_snake_case("MAX_BUFFER_SIZE"), vec!["MAX", "BUFFER", "SIZE"]);
}

#[test]
fn test_tokenizer_camel_case_produces_all_variants() {
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("UserService");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"userservice".to_string()), "Missing original: {:?}", tokens);
    assert!(tokens.contains(&"user".to_string()), "Missing 'user': {:?}", tokens);
    assert!(tokens.contains(&"service".to_string()), "Missing 'service': {:?}", tokens);
}

#[test]
fn test_tokenizer_preserves_rust_patterns() {
    let tokenizer = CodeTokenizer::new(vec!["::".to_string(), "->".to_string()]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("std::io::Read");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"std".to_string()));
    assert!(tokens.contains(&"::".to_string()));
    assert!(tokens.contains(&"io".to_string()));
    assert!(tokens.contains(&"read".to_string()));
}

#[test]
fn test_tokenizer_preserves_typescript_patterns() {
    let tokenizer = CodeTokenizer::new(vec!["?.".to_string(), "??".to_string()]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("user?.profile ?? default");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"user".to_string()));
    assert!(tokens.contains(&"?.".to_string()));
    assert!(tokens.contains(&"profile".to_string()));
    assert!(tokens.contains(&"??".to_string()));
}

#[test]
fn test_tokenizer_snake_case_produces_parts() {
    let tokenizer = CodeTokenizer::new(vec![]);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("get_user_data");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"get_user_data".to_string()), "Missing original: {:?}", tokens);
    assert!(tokens.contains(&"get".to_string()), "Missing 'get': {:?}", tokens);
    assert!(tokens.contains(&"user".to_string()), "Missing 'user': {:?}", tokens);
    assert!(tokens.contains(&"data".to_string()), "Missing 'data': {:?}", tokens);
}

#[test]
fn test_tokenizer_from_language_configs() {
    use crate::search::language_config::LanguageConfigs;
    let configs = LanguageConfigs::load_embedded();
    let tokenizer = CodeTokenizer::from_language_configs(&configs);
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("std::io::Result");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }
    assert!(tokens.contains(&"::".to_string()), "Should preserve :: from configs: {:?}", tokens);
}
```

Register this test file in `src/tests/mod.rs`.

**Step 2: Run test to verify it fails**

Run: `cargo test tantivy_tokenizer --no-run 2>&1 | tail -5`

Expected: Compilation error — `CodeTokenizer`, `split_camel_case`, `split_snake_case` don't exist.

**Step 3: Implement tokenizer.rs**

Port directly from `~/Source/razorback/crates/razorback-search/src/tokenizer.rs` (310 lines).

The exact code is at `/Users/murphy/Source/razorback/crates/razorback-search/src/tokenizer.rs`. Copy the entire file, then adapt:

1. Change the `from_language_configs` method to accept `&LanguageConfigs` instead of Razorback's `LanguageRegistry`:

```rust
use crate::search::language_config::LanguageConfigs;

impl CodeTokenizer {
    pub fn from_language_configs(configs: &LanguageConfigs) -> Self {
        let patterns = configs.all_preserve_patterns();
        Self::new(patterns)
    }
}
```

2. Keep everything else (the Tokenizer impl, CodeTokenStream, tokenize_code, extract_segments, split_camel_case, split_snake_case) exactly as-is from Razorback.

3. Make `split_camel_case` and `split_snake_case` `pub` so tests can access them.

**Step 4: Run tests to verify they pass**

Run: `cargo test tantivy_tokenizer -v 2>&1 | tail -20`

Expected: All 11 tests pass.

**Step 5: Commit**

```bash
git add src/search/tokenizer.rs src/tests/
git commit -m "feat(search): port CodeTokenizer with CamelCase/snake_case splitting"
```

---

### Task 4: Port Search Schema and Index Management

**Files:**
- Create: `src/search/schema.rs` (replace empty placeholder)
- Create: `src/search/index.rs` (replace empty placeholder)
- Create: `src/search/query.rs` (replace empty placeholder)
- Test: `src/tests/tools/search/tantivy_index_tests.rs`

**Step 1: Write the failing tests**

Create `src/tests/tools/search/tantivy_index_tests.rs`:

```rust
//! Tests for Tantivy search index.

use tempfile::TempDir;
use crate::search::index::{SearchIndex, SymbolDocument, FileDocument, SearchFilter};

#[test]
fn test_create_index() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
    assert!(temp_dir.path().join("meta.json").exists());
}

#[test]
fn test_open_existing_index() {
    let temp_dir = TempDir::new().unwrap();
    { let _index = SearchIndex::create(temp_dir.path()).unwrap(); }
    let index = SearchIndex::open(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_open_or_create() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::open_or_create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_add_symbol_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index.add_symbol(&SymbolDocument {
        id: "1".into(),
        name: "UserService".into(),
        signature: "pub struct UserService".into(),
        doc_comment: "Manages users".into(),
        code_body: "pub struct UserService { db: Database }".into(),
        file_path: "src/user.rs".into(),
        kind: "class".into(),
        language: "rust".into(),
        start_line: 10,
    }).unwrap();
    index.commit().unwrap();

    let results = index.search_symbols("user", &SearchFilter::default(), 10).unwrap();
    assert!(!results.is_empty(), "Should find UserService when searching 'user'");
    assert_eq!(results[0].name, "UserService");
}

#[test]
fn test_add_file_content_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index.add_file_content(&FileDocument {
        file_path: "src/main.rs".into(),
        content: "fn main() { println!(\"hello world\"); }".into(),
        language: "rust".into(),
    }).unwrap();
    index.commit().unwrap();

    let results = index.search_content("println", &SearchFilter::default(), 10).unwrap();
    assert!(!results.is_empty(), "Should find file containing 'println'");
    assert_eq!(results[0].file_path, "src/main.rs");
}

#[test]
fn test_name_match_ranks_higher_than_body() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // "process" in name
    index.add_symbol(&SymbolDocument {
        id: "1".into(), name: "process_data".into(),
        signature: "fn process_data()".into(), doc_comment: "".into(),
        code_body: "fn process_data() {}".into(), file_path: "src/a.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();

    // "process" only in doc comment
    index.add_symbol(&SymbolDocument {
        id: "2".into(), name: "handle_request".into(),
        signature: "fn handle_request()".into(),
        doc_comment: "This will process the data".into(),
        code_body: "fn handle_request() {}".into(), file_path: "src/b.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();
    index.commit().unwrap();

    let results = index.search_symbols("process", &SearchFilter::default(), 10).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "process_data", "Name match should rank first");
}

#[test]
fn test_language_filter() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index.add_symbol(&SymbolDocument {
        id: "1".into(), name: "process".into(), signature: "fn process()".into(),
        doc_comment: "".into(), code_body: "".into(), file_path: "src/lib.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();
    index.add_symbol(&SymbolDocument {
        id: "2".into(), name: "process".into(), signature: "function process()".into(),
        doc_comment: "".into(), code_body: "".into(), file_path: "src/lib.ts".into(),
        kind: "function".into(), language: "typescript".into(), start_line: 1,
    }).unwrap();
    index.commit().unwrap();

    let filter = SearchFilter { language: Some("rust".into()), ..Default::default() };
    let results = index.search_symbols("process", &filter, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].language, "rust");
}

#[test]
fn test_delete_by_file_path() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index.add_symbol(&SymbolDocument {
        id: "1".into(), name: "foo".into(), signature: "fn foo()".into(),
        doc_comment: "".into(), code_body: "".into(), file_path: "src/a.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();
    index.add_symbol(&SymbolDocument {
        id: "2".into(), name: "bar".into(), signature: "fn bar()".into(),
        doc_comment: "".into(), code_body: "".into(), file_path: "src/b.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 2);

    index.remove_by_file_path("src/a.rs").unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 1);
}

#[test]
fn test_camel_case_cross_convention_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index.add_symbol(&SymbolDocument {
        id: "1".into(), name: "getUserData".into(),
        signature: "fn getUserData()".into(), doc_comment: "".into(),
        code_body: "".into(), file_path: "src/api.ts".into(),
        kind: "function".into(), language: "typescript".into(), start_line: 1,
    }).unwrap();
    index.add_symbol(&SymbolDocument {
        id: "2".into(), name: "get_user_data".into(),
        signature: "fn get_user_data()".into(), doc_comment: "".into(),
        code_body: "".into(), file_path: "src/api.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 1,
    }).unwrap();
    index.commit().unwrap();

    // Searching "user" should find BOTH (CamelCase + snake_case splitting)
    let results = index.search_symbols("user", &SearchFilter::default(), 10).unwrap();
    assert_eq!(results.len(), 2, "Should find both getUserData and get_user_data when searching 'user'");
}
```

Register this test file in `src/tests/mod.rs`.

**Step 2: Run test to verify it fails**

Run: `cargo test tantivy_index --no-run 2>&1 | tail -5`

Expected: Compilation error — `SearchIndex`, `SymbolDocument`, etc. don't exist.

**Step 3: Implement schema.rs**

Port from `~/Source/razorback/crates/razorback-search/src/schema.rs` and extend with file content document type:

```rust
//! Tantivy schema definition for code symbol and file content indexing.

use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, STORED, STRING, Schema,
    TextFieldIndexing, TextOptions,
};

pub mod fields {
    // Common fields
    pub const DOC_TYPE: &str = "doc_type";
    pub const ID: &str = "id";
    pub const FILE_PATH: &str = "file_path";
    pub const LANGUAGE: &str = "language";

    // Symbol-specific fields
    pub const NAME: &str = "name";
    pub const SIGNATURE: &str = "signature";
    pub const DOC_COMMENT: &str = "doc_comment";
    pub const CODE_BODY: &str = "code_body";
    pub const KIND: &str = "kind";
    pub const START_LINE: &str = "start_line";

    // File content fields
    pub const CONTENT: &str = "content";
}

pub fn create_schema() -> Schema {
    let mut builder = Schema::builder();

    let code_text_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("code")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let code_text_not_stored = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("code")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    // Common fields
    builder.add_text_field(fields::DOC_TYPE, STRING | STORED);
    builder.add_text_field(fields::ID, STRING | STORED);
    builder.add_text_field(fields::FILE_PATH, STRING | STORED);
    builder.add_text_field(fields::LANGUAGE, STRING | STORED);

    // Symbol fields
    builder.add_text_field(fields::NAME, code_text_options.clone());
    builder.add_text_field(fields::SIGNATURE, code_text_options.clone());
    builder.add_text_field(fields::DOC_COMMENT, code_text_options.clone());
    builder.add_text_field(fields::CODE_BODY, code_text_not_stored.clone());
    builder.add_text_field(fields::KIND, STRING | STORED);
    builder.add_u64_field(fields::START_LINE, NumericOptions::default().set_stored());

    // File content fields
    builder.add_text_field(fields::CONTENT, code_text_not_stored);

    builder.build()
}

/// Helper struct for accessing schema fields.
#[derive(Clone)]
pub struct SchemaFields {
    pub doc_type: Field,
    pub id: Field,
    pub file_path: Field,
    pub language: Field,
    pub name: Field,
    pub signature: Field,
    pub doc_comment: Field,
    pub code_body: Field,
    pub kind: Field,
    pub start_line: Field,
    pub content: Field,
}

impl SchemaFields {
    pub fn new(schema: &Schema) -> Self {
        Self {
            doc_type: schema.get_field(fields::DOC_TYPE).unwrap(),
            id: schema.get_field(fields::ID).unwrap(),
            file_path: schema.get_field(fields::FILE_PATH).unwrap(),
            language: schema.get_field(fields::LANGUAGE).unwrap(),
            name: schema.get_field(fields::NAME).unwrap(),
            signature: schema.get_field(fields::SIGNATURE).unwrap(),
            doc_comment: schema.get_field(fields::DOC_COMMENT).unwrap(),
            code_body: schema.get_field(fields::CODE_BODY).unwrap(),
            kind: schema.get_field(fields::KIND).unwrap(),
            start_line: schema.get_field(fields::START_LINE).unwrap(),
            content: schema.get_field(fields::CONTENT).unwrap(),
        }
    }
}
```

**Step 4: Implement index.rs**

Port from `~/Source/razorback/crates/razorback-search/src/index.rs` and extend with:
- File content document support (`add_file_content`, `search_content`)
- `doc_type` field to distinguish symbols from files
- `remove_by_file_path` that deletes both symbol and file docs

Key differences from Razorback's version:
- `SymbolDocument` and `FileDocument` are separate types
- `search_symbols` filters by `doc_type = "symbol"`
- `search_content` filters by `doc_type = "file"`
- `SearchFilter` has `file_pattern` (post-filter on results, same as Razorback)

The `SearchIndex` struct and its methods follow the same pattern as Razorback's `index.rs` (561 lines). Adapt to use `crate::search::schema::*` and `crate::search::tokenizer::*`.

**Step 5: Implement query.rs**

Create `src/search/query.rs` with the boosted query builder:

```rust
//! Query building for Tantivy search.

use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::Term;

/// Build a boosted symbol search query.
///
/// Each query term is searched across all symbol fields with field-specific boosts:
/// - name: 5.0x (highest priority)
/// - signature: 3.0x
/// - doc_comment: 2.0x
/// - code_body: 1.0x (base)
pub fn build_symbol_query(
    terms: &[String],
    name_field: Field,
    sig_field: Field,
    doc_field: Field,
    body_field: Field,
    doc_type_field: Field,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Filter to symbol documents only
    let type_term = Term::from_field_text(doc_type_field, "symbol");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    for term in terms {
        let term_lower = term.to_lowercase();

        let name_term = Term::from_field_text(name_field, &term_lower);
        let name_query = TermQuery::new(name_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Should, Box::new(BoostQuery::new(Box::new(name_query), 5.0))));

        let sig_term = Term::from_field_text(sig_field, &term_lower);
        let sig_query = TermQuery::new(sig_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Should, Box::new(BoostQuery::new(Box::new(sig_query), 3.0))));

        let doc_term = Term::from_field_text(doc_field, &term_lower);
        let doc_query = TermQuery::new(doc_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Should, Box::new(BoostQuery::new(Box::new(doc_query), 2.0))));

        let body_term = Term::from_field_text(body_field, &term_lower);
        let body_query = TermQuery::new(body_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Should, Box::new(body_query)));
    }

    BooleanQuery::new(subqueries)
}

/// Build a file content search query.
pub fn build_content_query(
    terms: &[String],
    content_field: Field,
    doc_type_field: Field,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Filter to file documents only
    let type_term = Term::from_field_text(doc_type_field, "file");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    for term in terms {
        let term_lower = term.to_lowercase();
        let content_term = Term::from_field_text(content_field, &term_lower);
        let content_query = TermQuery::new(content_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Should, Box::new(content_query)));
    }

    BooleanQuery::new(subqueries)
}

/// Add filter clauses to an existing set of subqueries.
pub fn add_filter_clauses(
    subqueries: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
    language: Option<&str>,
    kind: Option<&str>,
    language_field: Field,
    kind_field: Field,
) {
    if let Some(lang) = language {
        let lang_term = Term::from_field_text(language_field, lang);
        let lang_query = TermQuery::new(lang_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Must, Box::new(lang_query)));
    }
    if let Some(k) = kind {
        let kind_term = Term::from_field_text(kind_field, k);
        let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
        subqueries.push((Occur::Must, Box::new(kind_query)));
    }
}
```

**Step 6: Run tests to verify they pass**

Run: `cargo test tantivy_index -v 2>&1 | tail -30`

Expected: All 10 tests pass. The critical one is `test_camel_case_cross_convention_search` — searching "user" must find both `getUserData` and `get_user_data`.

**Step 7: Commit**

```bash
git add src/search/schema.rs src/search/index.rs src/search/query.rs src/tests/
git commit -m "feat(search): port search index with schema, query building, and doc type support"
```

---

### Task 5: Create scoring.rs Placeholder

**Files:**
- Create: `src/search/scoring.rs` (replace empty placeholder)

This will be fully implemented in Phase 4. For now, create a minimal placeholder:

```rust
//! Post-search scoring and reranking.
//!
//! Will be implemented in Phase 4 to support:
//! - important_patterns boosting from language configs
//! - Symbol kind boosting
```

**Commit:**

```bash
git add src/search/scoring.rs
git commit -m "feat(search): add scoring module placeholder for Phase 4"
```

---

## Phase 2: Switch Search Queries to Tantivy

Now we wire the new search engine into Julie's tool layer.

---

### Task 6: Wire Tantivy Index into Workspace Initialization

**Files:**
- Modify: `src/workspace/mod.rs` — add SearchIndex alongside Database
- Modify: `src/handler.rs` — add SearchIndex to JulieServerHandler

**Context:** Currently `workspace/mod.rs` creates the database at `.julie/indexes/{workspace_id}/db/symbols.db`. We need to also create/open the Tantivy index at `.julie/indexes/{workspace_id}/tantivy/`.

**Step 1: Add SearchIndex to workspace struct**

In `src/workspace/mod.rs`, add a field to hold the search index. Add `SearchIndex` alongside the existing `db` field. The workspace already has path helpers — add:

```rust
pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
    self.indexes_root_path().join(workspace_id).join("tantivy")
}
```

Add a method `initialize_search_index(&mut self)` that:
1. Computes the workspace ID
2. Gets the tantivy path
3. Calls `SearchIndex::open_or_create(path)` with language configs
4. Stores it as `Option<Arc<Mutex<SearchIndex>>>` similar to the db field

**Step 2: Wire into handler**

In `src/handler.rs:64-83` (`JulieServerHandler`), add access to the search index via the workspace.

**Step 3: Populate Tantivy during indexing**

In `src/tools/workspace/indexing/processor.rs`, after the bulk SQLite store (around line 219-300), add code to also add documents to the Tantivy index:

```rust
// After bulk_store_symbols, also index in Tantivy
if let Some(search_index) = workspace.search_index() {
    let mut index = search_index.lock().unwrap();
    for symbol in &all_symbols {
        index.add_symbol(&SymbolDocument::from(symbol))?;
    }
    for file_info in &all_file_infos {
        index.add_file_content(&FileDocument::from(file_info))?;
    }
    index.commit()?;
}
```

This requires implementing `From<Symbol>` for `SymbolDocument` and `From<FileInfo>` for `FileDocument`.

**Step 4: Verify indexing works**

Run: `cargo build 2>&1 | tail -10`

Expected: Clean build. The Tantivy index is now populated during workspace indexing but not yet used for search queries.

**Step 5: Commit**

```bash
git add src/workspace/ src/handler.rs src/tools/workspace/indexing/
git commit -m "feat(search): wire tantivy index into workspace init and indexing pipeline"
```

---

### Task 7: Rewrite text_search_impl to Use Tantivy

**Files:**
- Modify: `src/tools/search/text_search.rs` — replace FTS5 calls with Tantivy calls
- Test: Run existing search tests + new validation tests

**Context:** `text_search_impl()` (line 22-219) is the main entry point. Currently it:
1. Expands query (CamelCase/snake_case variants)
2. Routes to `database_search_with_workspace_filter()` for symbols (uses `find_symbols_by_pattern` → FTS5)
3. Routes to `sqlite_fts_search()` for content (uses `search_file_content_fts` → FTS5)

**Step 1: Rewrite text_search_impl**

The new flow is dramatically simpler:
1. Get SearchIndex from handler
2. Build SearchFilter from parameters
3. If `search_target == "definitions"`: call `search_index.search_symbols(query, filter, limit)`
4. If `search_target == "content"`: call `search_index.search_content(query, filter, limit)`
5. Convert results to `Vec<Symbol>` for compatibility with existing tool output

No query expansion needed. No FTS5 sanitization needed. The CodeTokenizer handles everything at index time.

**Step 2: Keep helper functions**

Keep `is_useful_line()`, `extract_context_lines()`, `find_intelligent_context()` — these are used for formatting content search results and are independent of the search backend.

**Step 3: Remove deprecated functions**

Mark `database_search_with_workspace_filter()` and `sqlite_fts_search()` as dead code (or remove them). They're replaced by Tantivy.

**Step 4: Run existing search tests**

Run: `cargo test search -v 2>&1 | tail -30`

Fix any test failures caused by the switchover. Tests that relied on FTS5-specific behavior (query sanitization, etc.) will need updating.

**Step 5: Commit**

```bash
git add src/tools/search/
git commit -m "feat(search): rewrite text_search_impl to use tantivy instead of FTS5"
```

---

### Task 8: Simplify Query Preprocessor and Remove Query Expansion

**Files:**
- Modify: `src/tools/search/query_preprocessor.rs` — gut FTS5 sanitization
- Modify: `src/utils/query_expansion.rs` — remove or simplify

**Context:** `query_preprocessor.rs` has ~500 lines of FTS5 workarounds. With Tantivy + CodeTokenizer, most of this is unnecessary.

**Step 1: Simplify query_preprocessor.rs**

Keep:
- `detect_query_type()` — still useful for routing (Symbol vs Pattern vs Glob vs Standard)
- `validate_query()` — reject pure wildcards
- `PreprocessedQuery` struct

Remove:
- `sanitize_for_fts5()` and all its sub-functions (~150 lines)
- `sanitize_symbol_for_fts5()`, `sanitize_pattern_for_fts5()`, `sanitize_glob_for_fts5()`, `sanitize_standard_for_fts5()`
- FTS5-specific processing logic

**Step 2: Simplify or remove query_expansion.rs**

The `expand_query()` function generates CamelCase/snake_case variants at query time. With CodeTokenizer splitting at index time, this is unnecessary. Either:
- Remove the file entirely
- Keep a minimal version that just returns the original query

**Step 3: Run tests**

Run: `cargo test query_preprocessor -v 2>&1 | tail -20`

Update any tests that expected FTS5-specific behavior.

**Step 4: Commit**

```bash
git add src/tools/search/ src/utils/
git commit -m "refactor(search): simplify query preprocessor, remove FTS5 sanitization and query expansion"
```

---

### Task 9: Update Search Method Routing

**Files:**
- Modify: `src/tools/search/mod.rs` — update `detect_search_method` and tool dispatch

**Context:** The `FastSearchTool` currently routes between "text", "semantic", and "hybrid" search methods. With semantic search removed and Tantivy handling all text search:

**Step 1: Simplify search method handling**

- "text" → Tantivy search (was FTS5)
- "semantic" → Tantivy search (semantic is gone, gracefully degrade)
- "hybrid" → Tantivy search (no more blending needed)
- "auto" → Tantivy search

All roads lead to Tantivy now. Simplify `detect_search_method` and the dispatch logic in `call_tool`.

**Step 2: Remove semantic_search.rs dependency**

The `semantic_search_impl` function in `semantic_search.rs` can't be called anymore (no embeddings). Remove the import and routing to it. Keep the file for now (Phase 3 cleans it up) or remove if it causes compilation issues.

**Step 3: Run full test suite**

Run: `cargo test 2>&1 | tail -30`

Fix any failures.

**Step 4: Commit**

```bash
git add src/tools/search/
git commit -m "refactor(search): simplify search method routing, all search goes through tantivy"
```

---

## Phase 3: Remove FTS5 and ORT

The big cleanup. Tantivy is now handling all search. FTS5 and embeddings are dead code.

---

### Task 10: Remove FTS5 from Database Layer

**Files:**
- Modify: `src/database/schema.rs` — remove FTS5 table creation, triggers, rebuild
- Modify: `src/database/mod.rs` — remove `check_and_rebuild_fts5_indexes()`
- Modify: `src/database/symbols/queries.rs` — remove `find_symbols_by_pattern()`, `sanitize_fts5_query()`
- Modify: `src/database/files.rs` — remove `search_file_content_fts()`
- Modify: `src/database/symbols/bulk.rs` — remove FTS5 trigger dance
- Modify: `src/database/bulk_operations.rs` — remove FTS5 rebuild
- Modify: `src/database/migrations.rs` — add migration to drop FTS5

**Step 1: Remove from schema.rs**

Remove these functions:
- `create_files_fts_table()` (line 105-119)
- `create_files_fts_triggers()` (line 122-152)
- `rebuild_files_fts()` (line 175-185)
- `create_symbols_fts_table()` (line 266-282)
- `create_symbols_fts_triggers()` (line 284-318)
- `disable_symbols_fts_triggers()` (line 323)
- `enable_symbols_fts_triggers()` (line 332)
- `rebuild_symbols_fts()` (line 341-353)

Remove calls to these functions from `initialize_schema()`.

**Step 2: Remove from mod.rs**

Remove `check_and_rebuild_fts5_indexes()` (line 120-186) and its call from `new()` (line 112).

**Step 3: Remove from queries.rs**

Remove `sanitize_fts5_query()` (line 103-208) and `find_symbols_by_pattern()` (line 214-301).

**Step 4: Remove from files.rs**

Remove `search_file_content_fts()` and associated FTS5 sanitization.

**Step 5: Remove from bulk.rs and bulk_operations.rs**

Remove FTS5 trigger disable/enable and FTS5 rebuild calls from bulk operations.

**Step 6: Add FTS5 drop migration**

In `src/database/migrations.rs`, add a new migration that drops FTS5 tables and triggers for existing databases:

```rust
fn migrate_drop_fts5(&self) -> Result<()> {
    self.conn.execute("DROP TABLE IF EXISTS symbols_fts", [])?;
    self.conn.execute("DROP TABLE IF EXISTS files_fts", [])?;
    for trigger in &["symbols_ai", "symbols_ad", "symbols_au",
                      "files_ai", "files_ad", "files_au"] {
        self.conn.execute(&format!("DROP TRIGGER IF EXISTS {trigger}"), [])?;
    }
    Ok(())
}
```

**Step 7: Run tests**

Run: `cargo test 2>&1 | tail -30`

Many tests will need updating — especially those in `src/tests/core/database.rs` and `src/tests/tools/search_quality/`.

**Step 8: Commit**

```bash
git add src/database/ src/tests/
git commit -m "refactor(database): remove FTS5 tables, triggers, and queries"
```

---

### Task 11: Remove Embeddings Module and ORT Dependencies

**Files:**
- Delete: `src/embeddings/` (entire directory — 6 files, ~3400 lines)
- Delete: `src/bin/semantic.rs` (standalone semantic CLI)
- Modify: `src/lib.rs` — remove `pub mod embeddings`
- Modify: `src/handler.rs` — remove EmbeddingEngine, IndexingStatus HNSW fields
- Modify: `src/workspace/mod.rs` — remove EmbeddingStore, VectorIndex type aliases, embedding init
- Modify: `src/watcher/mod.rs` — remove embedding sync
- Modify: `src/tools/search/semantic_search.rs` — delete or gut
- Modify: `src/tools/search/mod.rs` — remove semantic_search module
- Modify: `src/tools/workspace/indexing/embeddings.rs` — delete
- Modify: `src/tools/exploration/find_logic/search.rs` — remove embedding refs
- Modify: `src/tools/trace_call_path/mod.rs` — remove embedding refs
- Modify: `src/tools/navigation/semantic_matching.rs` — remove or adapt
- Modify: `src/tools/navigation/reference_workspace.rs` — remove embedding refs
- Remove test files: `src/tests/core/embeddings/`, `src/tests/integration/watcher_embeddings.rs`, `src/tests/embedding_batch_sizing_tests.rs`

**Step 1: Delete embeddings module**

```bash
rm -rf src/embeddings/
rm src/bin/semantic.rs
```

**Step 2: Remove `pub mod embeddings` from lib.rs**

Remove line 7: `pub mod embeddings;`

**Step 3: Remove the `julie-semantic` binary from Cargo.toml**

Remove lines:
```toml
[[bin]]
name = "julie-semantic"
path = "src/bin/semantic.rs"
```

**Step 4: Clean up handler.rs**

- Remove `use crate::embeddings::EmbeddingEngine;` (line 15)
- Simplify `IndexingStatus` — remove HNSW-related fields (line 33-38)
- Remove embedding engine initialization from workspace setup
- Remove embedding-related fields from `JulieServerHandler`

**Step 5: Clean up workspace/mod.rs**

- Remove `EmbeddingStore` and `VectorIndex` type aliases (line 26-27)
- Remove `EmbeddingEngine::new()` call (line 580)
- Remove `VectorStore::new()` call (line 715-722)
- Remove `ensure_embedding_cache_dir()`, `clear_embedding_cache()`
- Remove vectors directory creation

**Step 6: Clean up remaining references**

Fix every file that imports from `crate::embeddings`. The explore agents found these:
- `src/watcher/mod.rs:29`
- `src/tracing/mod.rs:10`
- `src/tools/search/semantic_search.rs` — delete entire file
- `src/tools/workspace/indexing/embeddings.rs` — delete entire file
- `src/tools/exploration/find_logic/search.rs:274`
- `src/tools/trace_call_path/mod.rs:234`
- `src/tools/navigation/reference_workspace.rs:11`
- `src/tools/navigation/semantic_matching.rs:13`

For each: either remove the import and the code that uses it, or replace with Tantivy-based alternative.

**Step 7: Remove embedding-related test files**

```bash
rm -rf src/tests/core/embeddings/
rm src/tests/integration/watcher_embeddings.rs
rm src/tests/embedding_batch_sizing_tests.rs
```

Update `src/tests/mod.rs` to remove these module declarations.

**Step 8: Remove ORT and related dependencies from Cargo.toml**

Remove from root `Cargo.toml`:
- `tokenizers` (line ~60)
- `hf-hub` (line ~63)
- `hnsw_rs` (line ~66)
- `ndarray` (line ~69)
- All platform-specific `ort` dependencies (lines ~141, 147, 151)
- The `windows` crate (line ~143 — GPU enumeration)
- `network_models` feature flag
- The `julie-semantic` binary entry

**Step 9: Compile and fix**

Run: `cargo check 2>&1 | head -50`

This will likely have cascading errors. Fix them iteratively. The key is: anywhere that references embeddings, HNSW, or vector stores needs to be removed or replaced.

**Step 10: Run tests**

Run: `cargo test 2>&1 | tail -30`

Many test files reference embeddings. Remove or adapt them.

**Step 11: Commit**

```bash
git add -A
git commit -m "refactor: remove ORT, HNSW, embeddings module, and all semantic search"
```

---

### Task 12: Final Cleanup and Verification

**Files:** Various

**Step 1: Remove dead code**

Run: `cargo check 2>&1 | grep "warning: unused"` and clean up any dead code warnings.

**Step 2: Remove query_expansion.rs if not already done**

If `src/utils/query_expansion.rs` still exists, check if anything uses it. If not, delete it.

**Step 3: Clean up hybrid_search.rs**

`src/tools/search/hybrid_search.rs` blended text + semantic results. With semantic gone, this is dead code. Remove or simplify to just call text search.

**Step 4: Full test suite**

Run: `cargo test 2>&1 | tail -30`

Expected: All tests pass. No FTS5, no embeddings, no ORT.

**Step 5: Build release to verify binary size improvement**

Run: `cargo build --release 2>&1 | tail -5`

Note the build time and binary size. Should be significantly smaller/faster without ORT.

**Step 6: Commit**

```bash
git add -A
git commit -m "chore: final cleanup of dead code from FTS5/ORT removal"
```

---

## Phase 4: Complete Language Config Features

Finish the Razorback TODO items: meaningful_affixes, strip_prefixes/suffixes, important_patterns scoring.

---

### Task 13: Wire meaningful_affixes into CodeTokenizer

**Files:**
- Modify: `src/search/tokenizer.rs`
- Modify: `src/search/language_config.rs`
- Test: `src/tests/tools/search/tantivy_affix_tests.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_meaningful_affix_stripping() {
    use crate::search::tokenizer::CodeTokenizer;
    use tantivy::tokenizer::{TextAnalyzer, TokenStream};

    // Create tokenizer with meaningful affixes
    let mut tokenizer = CodeTokenizer::new(vec![]);
    tokenizer.set_meaningful_affixes(vec!["is_".into(), "has_".into(), "_mut".into()]);

    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("is_valid");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }

    // Should have original + snake parts + affix-stripped
    assert!(tokens.contains(&"valid".to_string()), "Should strip is_ prefix: {:?}", tokens);
}
```

**Step 2: Implement**

Add a `meaningful_affixes` field to `CodeTokenizer`. In `tokenize_code()`, after CamelCase/snake_case splitting, check if any affix matches and emit the stripped version as an additional token.

**Step 3: Wire from_language_configs to pass affixes**

Update `from_language_configs()` to collect all `meaningful_affixes` from all configs and pass them to the tokenizer.

**Step 4: Run tests, commit**

---

### Task 14: Wire strip_prefixes/suffixes into CodeTokenizer

**Files:**
- Modify: `src/search/tokenizer.rs`
- Test: `src/tests/tools/search/tantivy_variants_tests.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_strip_prefix_interface() {
    let mut tokenizer = CodeTokenizer::new(vec![]);
    tokenizer.set_strip_rules(vec!["I".into()], vec!["Service".into(), "Controller".into()]);

    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream("IUserService");
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        tokens.push(token.text.clone());
    }

    assert!(tokens.contains(&"userservice".to_string()), "Should strip I prefix: {:?}", tokens);
    assert!(tokens.contains(&"iuser".to_string()), "Should strip Service suffix: {:?}", tokens);
}
```

**Step 2: Implement**

Add strip logic to `tokenize_code()`. For each identifier, check if it starts with a strip_prefix or ends with a strip_suffix. If so, emit the stripped version as an additional token.

**Step 3: Run tests, commit**

---

### Task 15: Implement important_patterns Scoring

**Files:**
- Modify: `src/search/scoring.rs`
- Modify: `src/search/index.rs` (add post-search reranking)
- Test: `src/tests/tools/search/tantivy_scoring_tests.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_important_patterns_boost() {
    use crate::search::scoring::apply_important_patterns_boost;
    use crate::search::language_config::LanguageConfigs;
    use crate::search::index::SymbolSearchResult;

    let configs = LanguageConfigs::load_embedded();

    let mut results = vec![
        SymbolSearchResult {
            name: "process".into(), signature: "fn process()".into(),
            language: "rust".into(), score: 1.0, ..Default::default()
        },
        SymbolSearchResult {
            name: "process".into(), signature: "pub fn process()".into(),
            language: "rust".into(), score: 1.0, ..Default::default()
        },
    ];

    apply_important_patterns_boost(&mut results, &configs);

    // "pub fn" matches an important pattern — should be boosted
    assert!(results[0].signature.contains("pub fn"), "pub fn should rank first after boost");
}
```

**Step 2: Implement**

```rust
pub fn apply_important_patterns_boost(
    results: &mut Vec<SymbolSearchResult>,
    configs: &LanguageConfigs,
) {
    for result in results.iter_mut() {
        if let Some(config) = configs.get(&result.language) {
            for pattern in &config.scoring.important_patterns {
                if result.signature.contains(pattern) {
                    result.score *= 1.5;
                    break; // Only boost once
                }
            }
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
}
```

**Step 3: Wire into search_symbols**

In `SearchIndex::search_symbols()`, after getting Tantivy results, call `apply_important_patterns_boost()`.

**Step 4: Run tests, commit**

---

### Task 16: Final Integration Test and Verification

**Files:**
- Test: `src/tests/tools/search/tantivy_integration_tests.rs`

**Step 1: Write comprehensive integration test**

```rust
#[test]
fn test_full_search_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create_with_configs(temp_dir.path()).unwrap();

    // Index some real-world-like code
    index.add_symbol(&SymbolDocument {
        id: "1".into(), name: "getUserProfile".into(),
        signature: "async function getUserProfile(id: string): Promise<User>".into(),
        doc_comment: "Fetches user profile from API".into(),
        code_body: "const response = await fetch(`/api/users/${id}`);".into(),
        file_path: "src/services/user.ts".into(),
        kind: "function".into(), language: "typescript".into(), start_line: 15,
    }).unwrap();

    index.add_symbol(&SymbolDocument {
        id: "2".into(), name: "get_user_profile".into(),
        signature: "pub async fn get_user_profile(id: &str) -> Result<User>".into(),
        doc_comment: "Fetches user profile from database".into(),
        code_body: "let user = db.query_one(\"SELECT * FROM users WHERE id = $1\", &[id]).await?;".into(),
        file_path: "src/services/user.rs".into(),
        kind: "function".into(), language: "rust".into(), start_line: 42,
    }).unwrap();
    index.commit().unwrap();

    // Test 1: Cross-convention matching
    let results = index.search_symbols("user profile", &SearchFilter::default(), 10).unwrap();
    assert_eq!(results.len(), 2, "Should find both TS camelCase and Rust snake_case");

    // Test 2: Language filtering
    let filter = SearchFilter { language: Some("rust".into()), ..Default::default() };
    let results = index.search_symbols("user", &filter, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].language, "rust");

    // Test 3: Name match ranks highest
    let results = index.search_symbols("getUserProfile", &SearchFilter::default(), 10).unwrap();
    assert_eq!(results[0].name, "getUserProfile", "Exact name match should rank first");
}
```

**Step 2: Run full test suite**

Run: `cargo test 2>&1 | tail -30`

Expected: ALL tests pass. No FTS5, no ORT, full Tantivy search with code-aware tokenization.

**Step 3: Commit**

```bash
git add src/tests/ src/search/
git commit -m "feat(search): complete language config features and integration tests"
```

---

## Summary of Key Commands

```bash
# Run specific test group
cargo test tantivy_language_config -v
cargo test tantivy_tokenizer -v
cargo test tantivy_index -v
cargo test tantivy_affix -v
cargo test tantivy_scoring -v

# Run all search tests
cargo test search -v

# Run full test suite
cargo test

# Check compilation
cargo check

# Build release (verify size/speed improvement)
cargo build --release
```

## Post-Implementation Checklist

- [ ] All 30 language configs embedded and loading
- [ ] CodeTokenizer splits CamelCase and snake_case at index time
- [ ] Language-specific operators preserved (::, ?., =>)
- [ ] Search "user" finds UserService, getUserData, user_service
- [ ] Name matches rank higher than body matches
- [ ] Language and kind filters work
- [ ] File content search works
- [ ] Incremental indexing updates Tantivy
- [ ] File deletion removes from Tantivy
- [ ] No FTS5 code remains
- [ ] No embeddings/ORT code remains
- [ ] All existing tests pass (adapted)
- [ ] meaningful_affixes working
- [ ] strip_prefixes/suffixes working
- [ ] important_patterns scoring working
- [ ] Binary size reduced
- [ ] Build time reduced
