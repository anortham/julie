# Token-Saving Edit Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two DMP-powered MCP editing tools (`edit_file`, `edit_symbol`) that let agents edit files without reading them first, plus fix `get_symbols` to return full markdown section content.

**Architecture:** New `src/tools/editing/` module wrapping `diff-match-patch-rs` (already in Cargo.toml) for fuzzy text matching. Atomic file writes via `EditingTransaction` (temp file + rename). Symbol lookup via existing database queries. Golden master tests with SOURCE/CONTROL fixture files. Tools registered in the existing `#[tool_router]` handler.

**Tech Stack:** `diff-match-patch-rs 0.5.1` (existing dependency), `tree-sitter` (existing), `rmcp` (existing MCP framework), `serde`/`schemars` (existing)

**Spec:** `docs/superpowers/specs/2026-04-03-token-saving-edit-tools-design.md`

---

## File Map

**New files:**
- `src/tools/editing/mod.rs` -- module root, re-exports
- `src/tools/editing/edit_file.rs` -- `EditFileTool` struct + DMP logic
- `src/tools/editing/edit_symbol.rs` -- `EditSymbolTool` struct + symbol lookup + editing
- `src/tools/editing/transaction.rs` -- `EditingTransaction` (atomic temp+rename writes)
- `src/tools/editing/validation.rs` -- bracket balance check, diff preview formatting
- `src/tests/tools/editing/edit_file_tests.rs` -- golden master tests for edit_file
- `src/tests/tools/editing/edit_symbol_tests.rs` -- golden master tests for edit_symbol
- `src/tests/tools/editing/security_tests.rs` -- path traversal tests
- `src/tests/tools/editing/markdown_section_tests.rs` -- markdown line range tests
- `fixtures/editing/sources/dmp_rust_module.rs` -- golden master source (Rust)
- `fixtures/editing/sources/dmp_python_class.py` -- golden master source (Python)
- `fixtures/editing/sources/dmp_markdown_doc.md` -- golden master source (Markdown)
- `fixtures/editing/controls/edit-file/*.rs|*.py|*.md` -- golden master expected outputs
- `fixtures/editing/controls/edit-symbol/*.rs|*.py` -- golden master expected outputs

**Modified files:**
- `crates/julie-extractors/src/markdown/mod.rs:227-256` -- fix `extract_section` line ranges
- `src/tools/mod.rs` -- add `pub mod editing;`
- `src/handler.rs` -- add `edit_file` and `edit_symbol` tool methods in `#[tool_router]` block
- `src/tests/tools/editing/mod.rs` -- add new test submodules
- `xtask/test_tiers.toml` -- add editing tests to `tools-misc` bucket

---

### Task 1: Markdown Section Line Range Fix

**Files:**
- Modify: `crates/julie-extractors/src/markdown/mod.rs:227-256`
- Create: `src/tests/tools/editing/markdown_section_tests.rs`
- Modify: `src/tests/tools/editing/mod.rs`

- [ ] **Step 1: Write failing test for markdown section line ranges**

Create `src/tests/tools/editing/markdown_section_tests.rs`:

```rust
//! Tests for markdown section line ranges covering full content, not just headings.

use crate::extractors::markdown::MarkdownExtractor;
use std::path::Path;

fn extract_markdown_symbols(source: &str) -> Vec<crate::extractors::base::Symbol> {
    let mut extractor = MarkdownExtractor::new(
        "markdown".to_string(),
        "test.md".to_string(),
        source.to_string(),
        Path::new("/test"),
    );
    let tree = extractor.base.parse_source();
    extractor.extract_symbols(&tree)
}

#[test]
fn test_section_line_range_covers_content() {
    let markdown = "# Title\n\nFirst paragraph.\n\n## Section A\n\nContent of section A.\n\nMore content.\n\n## Section B\n\nContent of section B.\n";
    //              line 1      line 3           line 5            line 7                  line 9        line 11           line 13

    let symbols = extract_markdown_symbols(markdown);

    // Find "Section A" symbol
    let section_a = symbols
        .iter()
        .find(|s| s.name == "Section A")
        .expect("Should find Section A");

    // Section A starts at line 5 (## Section A) and ends at line 9 (last content line before Section B)
    assert!(
        section_a.end_line > section_a.start_line + 1,
        "Section A end_line ({}) should extend well beyond start_line ({}) to cover content",
        section_a.end_line,
        section_a.start_line
    );

    // Section B should start after Section A ends
    let section_b = symbols
        .iter()
        .find(|s| s.name == "Section B")
        .expect("Should find Section B");

    assert!(
        section_b.start_line > section_a.end_line || section_b.start_line == section_a.end_line + 1,
        "Section B start ({}) should be after Section A end ({})",
        section_b.start_line,
        section_a.end_line
    );
}

#[test]
fn test_section_content_accessible_via_byte_range() {
    let markdown = "# Doc\n\n## Quick Reference\n\n```bash\ncargo build\ncargo test\n```\n\nSome notes here.\n\n## Next Section\n\nOther stuff.\n";

    let symbols = extract_markdown_symbols(markdown);

    let quick_ref = symbols
        .iter()
        .find(|s| s.name == "Quick Reference")
        .expect("Should find Quick Reference");

    // Extract the content using byte range (same logic as get_symbols mode=minimal)
    let content = &markdown[quick_ref.start_byte as usize..quick_ref.end_byte as usize];

    assert!(
        content.contains("cargo build"),
        "Section byte range should include code block content. Got: {}",
        content
    );
    assert!(
        content.contains("Some notes here"),
        "Section byte range should include paragraph content. Got: {}",
        content
    );
}
```

- [ ] **Step 2: Register test module**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod markdown_section_tests;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib tests::tools::editing::markdown_section_tests 2>&1 | tail -15`

Expected: FAIL -- section end_line equals start_line (only covers heading, not content).

- [ ] **Step 4: Fix extract_section to use section node's line range**

In `crates/julie-extractors/src/markdown/mod.rs`, modify `extract_section` (around line 227). After the call to `extract_heading`, patch the symbol's range to cover the full section node:

```rust
fn extract_section(
    &mut self,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut heading_node = None;
    let mut section_content = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "atx_heading" || child.kind() == "heading" {
            heading_node = Some(child);
        } else if self.is_content_node(&child) {
            let content_text = self.base.get_node_text(&child);
            if !section_content.is_empty() {
                section_content.push_str("\n\n");
            }
            section_content.push_str(&content_text);
        }
    }

    if let Some(heading) = heading_node {
        let mut symbol = self.extract_heading(heading, parent_id, Some(section_content))?;
        // Fix: expand range to cover the full section, not just the heading line.
        // The section node spans from the heading through all content until the next section.
        symbol.start_line = (node.start_position().row + 1) as u32;
        symbol.end_line = (node.end_position().row + 1) as u32;
        symbol.start_byte = node.start_byte() as u32;
        symbol.end_byte = node.end_byte() as u32;
        return Some(symbol);
    }

    None
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib tests::tools::editing::markdown_section_tests 2>&1 | tail -15`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/julie-extractors/src/markdown/mod.rs src/tests/tools/editing/markdown_section_tests.rs src/tests/tools/editing/mod.rs
git commit -m "fix(markdown): expand section line ranges to cover full content

Section symbols now span from the heading through all content
paragraphs, code blocks, and lists until the next section.
Enables get_symbols(target='Section Name', mode='minimal')
to return section body content, not just the heading text."
```

---

### Task 2: Editing Module Foundation

**Files:**
- Create: `src/tools/editing/mod.rs`
- Create: `src/tools/editing/transaction.rs`
- Create: `src/tools/editing/validation.rs`
- Modify: `src/tools/mod.rs`
- Modify: `src/tests/tools/editing/mod.rs`

- [ ] **Step 1: Create editing module skeleton**

Create `src/tools/editing/mod.rs`:

```rust
//! DMP-powered editing tools for token-efficient file modifications.
//!
//! Two tools:
//! - `edit_file`: fuzzy find-and-replace using diff-match-patch (works on any file)
//! - `edit_symbol`: symbol-aware editing using Julie's indexed symbol boundaries

pub mod edit_file;
pub mod edit_symbol;
pub mod transaction;
pub mod validation;

pub use edit_file::EditFileTool;
pub use edit_symbol::EditSymbolTool;
```

Register the module in `src/tools/mod.rs`. Find the existing module declarations and add:

```rust
pub mod editing;
```

- [ ] **Step 2: Write failing test for EditingTransaction**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod transaction_tests;
```

Create `src/tests/tools/editing/transaction_tests.rs`:

```rust
//! Tests for EditingTransaction atomic file writes.

use crate::tools::editing::transaction::EditingTransaction;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_transaction_commit_writes_file() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.rs");
    fs::write(&file_path, "original content").unwrap();

    let path_str = file_path.to_string_lossy().to_string();
    let txn = EditingTransaction::begin(&path_str).unwrap();
    txn.commit("modified content").unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    assert_eq!(result, "modified content");
}

#[test]
fn test_transaction_no_backup_files_left() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.rs");
    fs::write(&file_path, "content").unwrap();

    let path_str = file_path.to_string_lossy().to_string();
    let txn = EditingTransaction::begin(&path_str).unwrap();
    txn.commit("new content").unwrap();

    // No .tmp or .backup files should remain
    for entry in fs::read_dir(dir.path()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".tmp.") && !name.ends_with(".backup"),
            "Found leftover temp file: {}",
            name
        );
    }
}

#[test]
fn test_transaction_preserves_original_on_drop() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.rs");
    fs::write(&file_path, "original").unwrap();

    {
        let path_str = file_path.to_string_lossy().to_string();
        let _txn = EditingTransaction::begin(&path_str).unwrap();
        // txn dropped without commit
    }

    let result = fs::read_to_string(&file_path).unwrap();
    assert_eq!(result, "original", "File should be unchanged if transaction is dropped");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib tests::tools::editing::transaction_tests 2>&1 | tail -10`

Expected: FAIL -- `EditingTransaction` doesn't exist yet.

- [ ] **Step 4: Implement EditingTransaction**

Create `src/tools/editing/transaction.rs`:

```rust
//! Atomic file write operations using temp file + rename pattern.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Atomic file write transaction.
///
/// Writes to a temp file first, then renames to the target path.
/// If dropped without calling `commit()`, the original file is untouched.
pub struct EditingTransaction {
    file_path: PathBuf,
    temp_path: Option<PathBuf>,
}

impl EditingTransaction {
    /// Begin a transaction for the given file path.
    /// The file must already exist.
    pub fn begin(file_path: &str) -> Result<Self> {
        let path = PathBuf::from(file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("File does not exist: {}", file_path));
        }
        // Pre-flight: check the file is writable
        let metadata = fs::metadata(&path)?;
        if metadata.permissions().readonly() {
            return Err(anyhow::anyhow!("File is read-only: {}", file_path));
        }
        Ok(Self {
            file_path: path,
            temp_path: None,
        })
    }

    /// Commit the new content atomically.
    /// Writes to a temp file, then renames over the original.
    pub fn commit(mut self, content: &str) -> Result<()> {
        let file_name = self
            .file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let temp_name = format!("{}.tmp.{}", file_name, Uuid::new_v4().simple());
        let temp_path = self.file_path.with_file_name(&temp_name);

        fs::write(&temp_path, content)?;
        self.temp_path = Some(temp_path.clone());

        fs::rename(&temp_path, &self.file_path)?;
        self.temp_path = None; // Rename succeeded, no temp to clean up
        Ok(())
    }
}

impl Drop for EditingTransaction {
    fn drop(&mut self) {
        // Clean up temp file if commit was interrupted
        if let Some(temp_path) = &self.temp_path {
            let _ = fs::remove_file(temp_path);
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib tests::tools::editing::transaction_tests 2>&1 | tail -10`

Expected: PASS

- [ ] **Step 6: Write failing test for balance validation**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod validation_tests;
```

Create `src/tests/tools/editing/validation_tests.rs`:

```rust
//! Tests for bracket balance validation.

use crate::tools::editing::validation::{check_bracket_balance, format_unified_diff};

#[test]
fn test_balanced_code_passes() {
    let code = "fn main() {\n    let x = vec![1, 2, 3];\n    println!(\"{:?}\", x);\n}\n";
    assert!(check_bracket_balance(code).is_ok());
}

#[test]
fn test_unmatched_open_brace_fails() {
    let code = "fn main() {\n    let x = 1;\n";
    let result = check_bracket_balance(code);
    assert!(result.is_err(), "Unmatched open brace should fail");
}

#[test]
fn test_unmatched_close_paren_fails() {
    let code = "fn main() {\n    let x = foo());\n}\n";
    let result = check_bracket_balance(code);
    assert!(result.is_err(), "Unmatched close paren should fail");
}

#[test]
fn test_empty_string_passes() {
    assert!(check_bracket_balance("").is_ok());
}

#[test]
fn test_unified_diff_format() {
    let before = "line1\nline2\nline3\n";
    let after = "line1\nmodified\nline3\n";
    let diff = format_unified_diff(before, after, "test.rs");
    assert!(diff.contains("--- test.rs"), "Should have before header");
    assert!(diff.contains("+++ test.rs"), "Should have after header");
    assert!(diff.contains("-line2"), "Should show removed line");
    assert!(diff.contains("+modified"), "Should show added line");
}
```

- [ ] **Step 7: Implement validation module**

Create `src/tools/editing/validation.rs`:

```rust
//! Shared validation and formatting utilities for editing tools.

use anyhow::Result;

/// Check that all brackets, braces, and parentheses are matched in the content.
///
/// Returns Ok(()) if balanced, Err with details if unmatched.
/// Skips content inside string literals and comments (simplified: only checks raw counts).
pub fn check_bracket_balance(content: &str) -> Result<()> {
    let mut stack: Vec<char> = Vec::new();

    for ch in content.chars() {
        match ch {
            '{' | '[' | '(' => stack.push(ch),
            '}' => {
                if stack.last() == Some(&'{') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing brace '}}' -- edit would create invalid syntax"
                    ));
                }
            }
            ']' => {
                if stack.last() == Some(&'[') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing bracket ']' -- edit would create invalid syntax"
                    ));
                }
            }
            ')' => {
                if stack.last() == Some(&'(') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing paren ')' -- edit would create invalid syntax"
                    ));
                }
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        let unmatched: String = stack.iter().collect();
        return Err(anyhow::anyhow!(
            "Unmatched opening bracket(s): '{}' -- edit would create invalid syntax",
            unmatched
        ));
    }

    Ok(())
}

/// Determine if a file should have bracket balance checked based on extension.
/// Non-code files (markdown, yaml, json, toml) skip the check.
pub fn should_check_balance(file_path: &str) -> bool {
    let skip_extensions = [".md", ".yaml", ".yml", ".json", ".toml", ".txt", ".csv", ".xml", ".html"];
    !skip_extensions.iter().any(|ext| file_path.ends_with(ext))
}

/// Format a unified diff between before and after content.
/// Returns a compact diff string with 3 lines of context.
pub fn format_unified_diff(before: &str, after: &str, file_path: &str) -> String {
    use std::fmt::Write;

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut output = String::new();

    writeln!(output, "--- {}", file_path).unwrap();
    writeln!(output, "+++ {}", file_path).unwrap();

    // Simple line-by-line diff (sufficient for edit previews)
    let max_len = before_lines.len().max(after_lines.len());
    let mut in_hunk = false;
    let mut hunk_start = 0;
    let context = 3;

    for i in 0..max_len {
        let b = before_lines.get(i).copied();
        let a = after_lines.get(i).copied();

        if b != a {
            if !in_hunk {
                hunk_start = i.saturating_sub(context);
                // Print context lines before the change
                for j in hunk_start..i {
                    if let Some(line) = before_lines.get(j) {
                        writeln!(output, " {}", line).unwrap();
                    }
                }
                in_hunk = true;
            }
            if let Some(line) = b {
                writeln!(output, "-{}", line).unwrap();
            }
            if let Some(line) = a {
                writeln!(output, "+{}", line).unwrap();
            }
        } else if in_hunk {
            // Print trailing context
            if let Some(line) = b {
                writeln!(output, " {}", line).unwrap();
            }
            if i >= hunk_start + context * 2 {
                in_hunk = false;
            }
        }
    }

    output
}
```

- [ ] **Step 8: Run validation tests**

Run: `cargo test --lib tests::tools::editing::validation_tests 2>&1 | tail -10`

Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/tools/editing/ src/tests/tools/editing/ src/tools/mod.rs
git commit -m "feat(editing): add module foundation with EditingTransaction and balance validation

EditingTransaction provides atomic file writes (temp + rename).
Balance validation checks for unmatched brackets before committing edits.
format_unified_diff produces compact diff previews for dry_run output."
```

---

### Task 3: edit_file Tool

**Files:**
- Create: `src/tools/editing/edit_file.rs`
- Create: `fixtures/editing/sources/dmp_rust_module.rs`
- Create: `fixtures/editing/controls/edit-file/rust_exact_replace.rs`
- Create: `fixtures/editing/controls/edit-file/rust_fuzzy_replace.rs`
- Create: `fixtures/editing/controls/edit-file/rust_replace_all.rs`
- Create: `fixtures/editing/sources/dmp_markdown_doc.md`
- Create: `fixtures/editing/controls/edit-file/markdown_edit.md`
- Create: `src/tests/tools/editing/edit_file_tests.rs`
- Modify: `src/tests/tools/editing/mod.rs`

- [ ] **Step 1: Create golden master SOURCE fixture for Rust**

Create `fixtures/editing/sources/dmp_rust_module.rs`:

```rust
use std::collections::HashMap;

pub struct UserService {
    users: HashMap<u64, String>,
    api_url: String,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            users: HashMap::new(),
            api_url,
        }
    }

    pub fn get_user(&self, id: u64) -> Option<&String> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, id: u64, name: String) {
        self.users.insert(id, name);
    }

    pub fn count(&self) -> usize {
        self.users.len()
    }
}
```

- [ ] **Step 2: Create golden master CONTROL fixtures**

Create `fixtures/editing/controls/edit-file/rust_exact_replace.rs` (expected output after replacing `get_user` return type):

```rust
use std::collections::HashMap;

pub struct UserService {
    users: HashMap<u64, String>,
    api_url: String,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            users: HashMap::new(),
            api_url,
        }
    }

    pub fn get_user(&self, id: u64) -> Result<&String, NotFoundError> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, id: u64, name: String) {
        self.users.insert(id, name);
    }

    pub fn count(&self) -> usize {
        self.users.len()
    }
}
```

Create `fixtures/editing/controls/edit-file/rust_replace_all.rs` (expected output after replacing all `&self` with `&mut self`):

```rust
use std::collections::HashMap;

pub struct UserService {
    users: HashMap<u64, String>,
    api_url: String,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            users: HashMap::new(),
            api_url,
        }
    }

    pub fn get_user(&mut self, id: u64) -> Option<&String> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, id: u64, name: String) {
        self.users.insert(id, name);
    }

    pub fn count(&mut self) -> usize {
        self.users.len()
    }
}
```

Create `fixtures/editing/sources/dmp_markdown_doc.md`:

```markdown
# Project Plan

## Phase 1

Build the core module with basic functionality.

- Task A: Setup
- Task B: Implementation

## Phase 2

Add advanced features and testing.

- Task C: Integration tests
- Task D: Performance tuning

## Phase 3

Documentation and release.
```

Create `fixtures/editing/controls/edit-file/markdown_edit.md` (expected output after replacing Phase 2 content):

```markdown
# Project Plan

## Phase 1

Build the core module with basic functionality.

- Task A: Setup
- Task B: Implementation

## Phase 2

Redesigned to focus on security hardening.

- Task C: Security audit
- Task D: Penetration testing
- Task E: Fix vulnerabilities

## Phase 3

Documentation and release.
```

- [ ] **Step 3: Write failing tests for edit_file**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod edit_file_tests;
```

Create `src/tests/tools/editing/edit_file_tests.rs`:

```rust
//! Golden master tests for the edit_file tool.

use crate::tools::editing::edit_file::apply_edit;
use std::fs;
use std::path::PathBuf;

fn fixture_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/sources")
        .join(name)
}

fn fixture_control(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/controls/edit-file")
        .join(name)
}

fn load(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e))
}

#[test]
fn test_exact_replace() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_exact_replace.rs"));

    let result = apply_edit(
        &source,
        "pub fn get_user(&self, id: u64) -> Option<&String> {",
        "pub fn get_user(&self, id: u64) -> Result<&String, NotFoundError> {",
        "first",
    )
    .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (exact replace)");
}

#[test]
fn test_replace_all_occurrences() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_replace_all.rs"));

    // Replace all "&self" with "&mut self" (but not "&mut self" which already exists)
    // The source has "(&self" in get_user and count, and "(&mut self" in add_user
    let result = apply_edit(
        &source,
        "(&self,",
        "(&mut self,",
        "all",
    )
    .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (replace all)");
}

#[test]
fn test_markdown_edit() {
    let source = load(&fixture_source("dmp_markdown_doc.md"));
    let expected = load(&fixture_control("markdown_edit.md"));

    let old_text = "Add advanced features and testing.\n\n- Task C: Integration tests\n- Task D: Performance tuning";
    let new_text = "Redesigned to focus on security hardening.\n\n- Task C: Security audit\n- Task D: Penetration testing\n- Task E: Fix vulnerabilities";

    let result = apply_edit(&source, old_text, new_text, "first")
        .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (markdown edit)");
}

#[test]
fn test_no_match_returns_error() {
    let source = "fn main() {}\n";
    let result = apply_edit(source, "fn nonexistent()", "fn replacement()", "first");
    assert!(result.is_err(), "Should return error when no match found");
}

#[test]
fn test_dry_run_does_not_modify_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let file_path = dir.path().join("test.rs");
    let original = "fn hello() { println!(\"hi\"); }\n";
    fs::write(&file_path, original).unwrap();

    // apply_edit only transforms text, doesn't write -- dry_run is handled by the tool layer
    let result = apply_edit(original, "hello", "world", "first").unwrap();
    assert_ne!(result, original, "Result should be modified");

    // Original file untouched (apply_edit is pure)
    let on_disk = fs::read_to_string(&file_path).unwrap();
    assert_eq!(on_disk, original, "File on disk should be unchanged");
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::editing::edit_file_tests 2>&1 | tail -10`

Expected: FAIL -- `apply_edit` doesn't exist yet.

- [ ] **Step 5: Implement edit_file tool**

Create `src/tools/editing/edit_file.rs`:

```rust
//! edit_file tool: DMP-powered fuzzy find-and-replace.
//!
//! Lets agents edit files without reading them first. The agent provides
//! old_text (what to find) and new_text (what to replace with). DMP's
//! fuzzy matching tolerates minor differences.

use anyhow::{anyhow, Result};
use diff_match_patch_rs::DiffMatchPatch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::utils::secure_path_resolution;
use rmcp::model::{CallToolResult, Content};

use super::transaction::EditingTransaction;
use super::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

fn default_dry_run() -> bool {
    true
}

fn default_occurrence() -> String {
    "first".to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditFileTool {
    /// File path relative to workspace root
    pub file_path: String,

    /// Text to find in the file (fuzzy-matched via diff-match-patch)
    pub old_text: String,

    /// Replacement text
    pub new_text: String,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,

    /// Which occurrence to replace: "first" (default), "last", or "all"
    #[serde(default = "default_occurrence")]
    pub occurrence: String,
}

/// Pure function: apply an edit to content string. Returns modified content.
/// Separated from tool struct for testability.
pub fn apply_edit(
    content: &str,
    old_text: &str,
    new_text: &str,
    occurrence: &str,
) -> Result<String> {
    if old_text.is_empty() {
        return Err(anyhow!("old_text cannot be empty"));
    }

    let positions = find_all_matches(content, old_text)?;

    if positions.is_empty() {
        return Err(anyhow!(
            "No match found for the provided old_text ({} chars). \
             Verify the text exists in the file.",
            old_text.len()
        ));
    }

    let selected: Vec<usize> = match occurrence {
        "first" => vec![positions[0]],
        "last" => vec![*positions.last().unwrap()],
        "all" => positions,
        _ => return Err(anyhow!("Invalid occurrence '{}': must be 'first', 'last', or 'all'", occurrence)),
    };

    // Apply replacements in reverse order so positions don't shift
    let old_char_len = old_text.chars().count();
    let new_chars: Vec<char> = new_text.chars().collect();
    let mut result_chars: Vec<char> = content.chars().collect();

    for &pos in selected.iter().rev() {
        result_chars.splice(pos..pos + old_char_len, new_chars.iter().copied());
    }

    Ok(result_chars.into_iter().collect())
}

/// Find all match positions for old_text in content using DMP fuzzy matching.
fn find_all_matches(content: &str, old_text: &str) -> Result<Vec<usize>> {
    let dmp = DiffMatchPatch::new();
    let old_char_len = old_text.chars().count();
    let mut positions = Vec::new();
    let mut search_from: usize = 0;

    loop {
        match dmp.match_main(content, old_text, search_from) {
            Some(pos) if pos >= search_from => {
                positions.push(pos);
                search_from = pos + old_char_len;
            }
            _ => break,
        }
    }

    Ok(positions)
}

impl EditFileTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate parameters
        if self.old_text.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: old_text is required and cannot be empty".to_string(),
            )]));
        }

        // Resolve and validate file path (security check)
        let workspace_root = handler.workspace_root();
        let resolved_path = secure_path_resolution(&self.file_path, workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        // Read file content internally (not costing agent context tokens)
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Cannot read file '{}': {}", self.file_path, e))?;

        // Apply the edit
        let modified_content = match apply_edit(
            &original_content,
            &self.old_text,
            &self.new_text,
            &self.occurrence,
        ) {
            Ok(content) => content,
            Err(e) => {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    format!("Error: {}", e),
                )]));
            }
        };

        // Balance validation for code files
        if should_check_balance(&self.file_path) {
            if let Err(e) = check_bracket_balance(&modified_content) {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    format!(
                        "Edit rejected: {}. The edit would create unbalanced brackets. \
                         Review old_text/new_text and try again.",
                        e
                    ),
                )]));
            }
        }

        // Generate diff preview
        let diff = format_unified_diff(&original_content, &modified_content, &self.file_path);

        if self.dry_run {
            debug!("edit_file dry_run for {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!("Dry run preview (set dry_run=false to apply):\n\n{}", diff),
            )]));
        }

        // Commit the edit atomically
        let txn = EditingTransaction::begin(&resolved_str)?;
        txn.commit(&modified_content)?;

        debug!("edit_file applied to {}", self.file_path);
        Ok(CallToolResult::text_content(vec![Content::text(
            format!("Applied edit to {}:\n\n{}", self.file_path, diff),
        )]))
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::editing::edit_file_tests 2>&1 | tail -15`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/tools/editing/edit_file.rs src/tests/tools/editing/edit_file_tests.rs fixtures/editing/sources/dmp_* fixtures/editing/controls/edit-file/
git commit -m "feat(editing): add edit_file tool with DMP fuzzy find-and-replace

DMP-powered text matching finds old_text in files without exact match.
Supports first/last/all occurrence modes. Balance validation catches
structural damage. Golden master tests verify byte-for-byte correctness."
```

---

### Task 4: edit_symbol Tool

**Files:**
- Create: `src/tools/editing/edit_symbol.rs`
- Create: `src/tests/tools/editing/edit_symbol_tests.rs`
- Modify: `src/tests/tools/editing/mod.rs`

- [ ] **Step 1: Write failing tests for edit_symbol**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod edit_symbol_tests;
```

Create `src/tests/tools/editing/edit_symbol_tests.rs`:

```rust
//! Tests for the edit_symbol tool's symbol lookup and editing logic.
//!
//! These test the pure editing functions, not the full MCP tool flow
//! (which requires a running workspace with indexed symbols).

use crate::tools::editing::edit_symbol::{replace_symbol_body, insert_near_symbol};

#[test]
fn test_replace_symbol_body() {
    let source = r#"fn hello() {
    println!("hello");
}

fn world() {
    println!("world");
}
"#;

    let result = replace_symbol_body(source, 1, 3, "fn hello() {\n    println!(\"goodbye\");\n}")
        .expect("Replace should succeed");

    assert!(result.contains("goodbye"), "Should contain new body");
    assert!(result.contains("fn world()"), "Should preserve other functions");
    assert!(!result.contains("println!(\"hello\")"), "Should not contain old body");
}

#[test]
fn test_insert_after_symbol() {
    let source = "struct Foo {\n    x: i32,\n}\n\nfn bar() {}\n";

    let result = insert_near_symbol(source, 3, "\nimpl Foo {\n    fn new() -> Self { Self { x: 0 } }\n}\n", "after")
        .expect("Insert after should succeed");

    assert!(result.contains("impl Foo"), "Should contain inserted code");
    // Verify ordering: struct Foo, then impl Foo, then fn bar
    let struct_pos = result.find("struct Foo").unwrap();
    let impl_pos = result.find("impl Foo").unwrap();
    let bar_pos = result.find("fn bar").unwrap();
    assert!(struct_pos < impl_pos, "impl should be after struct");
    assert!(impl_pos < bar_pos, "impl should be before bar");
}

#[test]
fn test_insert_before_symbol() {
    let source = "fn process() {\n    // work\n}\n";

    let result = insert_near_symbol(source, 1, "/// Process all items.\n", "before")
        .expect("Insert before should succeed");

    let doc_pos = result.find("/// Process all items.").unwrap();
    let fn_pos = result.find("fn process()").unwrap();
    assert!(doc_pos < fn_pos, "Doc comment should be before function");
}

#[test]
fn test_replace_preserves_surrounding_content() {
    let source = "// header comment\n\nfn target() {\n    old_code();\n}\n\n// footer comment\n";

    let result = replace_symbol_body(source, 3, 5, "fn target() {\n    new_code();\n}")
        .expect("Replace should succeed");

    assert!(result.contains("// header comment"), "Should preserve header");
    assert!(result.contains("// footer comment"), "Should preserve footer");
    assert!(result.contains("new_code()"), "Should contain new code");
}

#[test]
fn test_invalid_line_range() {
    let source = "fn hello() {}\n";
    let result = replace_symbol_body(source, 5, 10, "new code");
    assert!(result.is_err(), "Should fail for out-of-range lines");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::tools::editing::edit_symbol_tests 2>&1 | tail -10`

Expected: FAIL -- `replace_symbol_body` and `insert_near_symbol` don't exist.

- [ ] **Step 3: Implement edit_symbol tool**

Create `src/tools/editing/edit_symbol.rs`:

```rust
//! edit_symbol tool: symbol-aware editing using Julie's indexed boundaries.
//!
//! The agent references a symbol by name. Julie looks up its location in the
//! index, then applies the edit. No file read required by the agent.

use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::utils::secure_path_resolution;
use rmcp::model::{CallToolResult, Content};

use super::transaction::EditingTransaction;
use super::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditSymbolTool {
    /// Symbol name to edit (supports qualified names like `MyClass::method`)
    pub symbol: String,

    /// Operation: "replace" (swap entire definition), "insert_after", "insert_before"
    pub operation: String,

    /// New code/text content for the operation
    pub content: String,

    /// Disambiguate when multiple symbols share a name (partial file path match)
    #[serde(default)]
    pub file_path: Option<String>,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,
}

/// Replace lines start_line..=end_line (1-indexed) with new content.
pub fn replace_symbol_body(
    source: &str,
    start_line: u32,
    end_line: u32,
    new_content: &str,
) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = (start_line as usize).saturating_sub(1);
    let end_idx = end_line as usize; // 1-indexed end becomes exclusive index

    if start_idx >= lines.len() || end_idx > lines.len() {
        return Err(anyhow!(
            "Line range {}-{} is outside file bounds (file has {} lines)",
            start_line,
            end_line,
            lines.len()
        ));
    }

    let mut result = String::new();
    // Lines before the symbol
    for line in &lines[..start_idx] {
        result.push_str(line);
        result.push('\n');
    }
    // New content (replacing the symbol)
    result.push_str(new_content);
    if !new_content.ends_with('\n') {
        result.push('\n');
    }
    // Lines after the symbol
    for line in &lines[end_idx..] {
        result.push_str(line);
        result.push('\n');
    }

    // Preserve original trailing newline behavior
    if !source.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    Ok(result)
}

/// Insert content before or after a specific line (1-indexed).
pub fn insert_near_symbol(
    source: &str,
    anchor_line: u32,
    new_content: &str,
    position: &str,  // "before" or "after"
) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let anchor_idx = (anchor_line as usize).saturating_sub(1);

    if anchor_idx >= lines.len() {
        return Err(anyhow!(
            "Line {} is outside file bounds (file has {} lines)",
            anchor_line,
            lines.len()
        ));
    }

    let insert_at = match position {
        "before" => anchor_idx,
        "after" => anchor_idx + 1,
        _ => return Err(anyhow!("Invalid position '{}': must be 'before' or 'after'", position)),
    };

    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == insert_at && position == "before" {
            result.push_str(new_content);
            if !new_content.ends_with('\n') {
                result.push('\n');
            }
        }
        result.push_str(line);
        result.push('\n');
        if i == insert_at - 1 && position == "after" {
            result.push_str(new_content);
            if !new_content.ends_with('\n') {
                result.push('\n');
            }
        }
    }
    // Handle insert_after the last line
    if position == "after" && insert_at >= lines.len() {
        result.push_str(new_content);
        if !new_content.ends_with('\n') {
            result.push('\n');
        }
    }

    Ok(result)
}

impl EditSymbolTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate parameters
        if self.symbol.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: symbol name is required".to_string(),
            )]));
        }
        if !["replace", "insert_after", "insert_before"].contains(&self.operation.as_str()) {
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!(
                    "Error: operation must be 'replace', 'insert_after', or 'insert_before', got '{}'",
                    self.operation
                ),
            )]));
        }

        // Get workspace and database
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run manage_workspace(operation=\"index\") first.")
        })?;
        let db_arc = workspace.db.as_ref().ok_or_else(|| {
            anyhow!("Database not available. Run manage_workspace(operation=\"index\") first.")
        })?.clone();

        // Look up symbol using the same logic as deep_dive (handles qualified names,
        // flat namespaces, parent/child resolution). Runs in spawn_blocking since SQLite is sync.
        let symbol_name = self.symbol.clone();
        let file_path_filter = self.file_path.clone();
        let matches = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String, u32, u32)>> {
            let db = db_arc.lock().map_err(|e| anyhow!("Database lock error: {}", e))?;
            let symbols = crate::tools::deep_dive::data::find_symbol(
                &db, &symbol_name, file_path_filter.as_deref()
            )?;
            Ok(symbols.iter().map(|s| {
                (s.name.clone(), s.file_path.clone(), s.start_line, s.end_line)
            }).collect())
        }).await??;

        // find_symbol already filters by file_path (context_file parameter)
        if matches.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!(
                    "Error: symbol '{}' not found in index. Use fast_search or get_symbols to verify the name.",
                    self.symbol
                ),
            )]));
        }

        if matches.len() > 1 {
            let locations: Vec<String> = matches
                .iter()
                .map(|(name, path, start, end)| format!("  {} at {}:{}-{}", name, path, start, end))
                .collect();
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!(
                    "Error: '{}' matches {} symbols. Provide file_path to disambiguate:\n{}",
                    self.symbol,
                    matches.len(),
                    locations.join("\n")
                ),
            )]));
        }

        let (_, symbol_file, start_line, end_line) = &matches[0];

        // Resolve the file path
        let workspace_root = handler.workspace_root();
        let resolved_path = secure_path_resolution(symbol_file, workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        // Read file content internally
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Cannot read file '{}': {}", symbol_file, e))?;

        // Apply the operation
        let modified_content = match self.operation.as_str() {
            "replace" => replace_symbol_body(&original_content, *start_line, *end_line, &self.content)?,
            "insert_after" => insert_near_symbol(&original_content, *end_line, &self.content, "after")?,
            "insert_before" => insert_near_symbol(&original_content, *start_line, &self.content, "before")?,
            _ => unreachable!(),
        };

        // Balance validation for code files
        if should_check_balance(symbol_file) {
            if let Err(e) = check_bracket_balance(&modified_content) {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    format!("Edit rejected: {}. Review the content and try again.", e),
                )]));
            }
        }

        // Generate diff
        let diff = format_unified_diff(&original_content, &modified_content, symbol_file);

        if self.dry_run {
            debug!("edit_symbol dry_run for {} in {}", self.symbol, symbol_file);
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!("Dry run preview (set dry_run=false to apply):\n\n{}", diff),
            )]));
        }

        // Commit atomically
        let txn = EditingTransaction::begin(&resolved_str)?;
        txn.commit(&modified_content)?;

        debug!("edit_symbol {} applied to {}", self.operation, symbol_file);
        Ok(CallToolResult::text_content(vec![Content::text(
            format!("Applied {} on '{}' in {}:\n\n{}", self.operation, self.symbol, symbol_file, diff),
        )]))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::tools::editing::edit_symbol_tests 2>&1 | tail -15`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/tools/editing/edit_symbol.rs src/tests/tools/editing/edit_symbol_tests.rs
git commit -m "feat(editing): add edit_symbol tool with symbol-aware editing

Agents reference symbols by name (supports qualified MyClass::method).
Three operations: replace (swap definition), insert_after, insert_before.
Symbol boundaries from Julie's index, no file read needed by the agent."
```

---

### Task 5: Handler Registration

**Files:**
- Modify: `src/handler.rs`

- [ ] **Step 1: Identify the tool_router block**

Read `src/handler.rs` and find the `#[tool_router]` impl block. Look for the last `#[tool(...)]` method (likely `query_metrics`). The new tools will be added after it.

- [ ] **Step 2: Add edit_file handler method**

Add inside the `#[tool_router] impl JulieServerHandler` block:

```rust
#[tool(
    name = "edit_file",
    description = "Edit a file without reading it first. Provide old_text (fuzzy-matched via diff-match-patch) and new_text. Saves the full Read step that the built-in Edit tool requires. Use occurrence to control which match: \"first\" (default), \"last\", or \"all\". Always dry_run=true first to preview, then dry_run=false to apply.",
    annotations(
        title = "Edit File",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = false
    )
)]
async fn edit_file(
    &self,
    Parameters(params): Parameters<crate::tools::editing::EditFileTool>,
) -> Result<CallToolResult, McpError> {
    debug!("✏️ edit_file: {} (dry_run={})", params.file_path, params.dry_run);
    let start = std::time::Instant::now();
    let metadata = serde_json::json!({
        "file": params.file_path,
        "occurrence": params.occurrence,
        "dry_run": params.dry_run,
    });
    let result = params
        .call_tool(self)
        .await
        .map_err(|e| McpError::internal_error(format!("edit_file failed: {}", e), None))?;
    let output_bytes = Self::output_bytes_from_result(&result);
    let source_file_paths = Self::extract_paths_from_result(&result);
    let report = ToolCallReport {
        result_count: None,
        source_bytes: None,
        output_bytes,
        metadata,
        source_file_paths,
    };
    self.record_tool_call("edit_file", start.elapsed(), &report);
    Ok(result)
}
```

- [ ] **Step 3: Add edit_symbol handler method**

Add inside the same `#[tool_router]` block:

```rust
#[tool(
    name = "edit_symbol",
    description = "Edit a symbol by name without reading the file. Operations: replace (swap entire definition), insert_after, insert_before. The symbol is looked up from Julie's index. Combine with deep_dive or get_symbols for zero-read editing workflows. Always dry_run=true first to preview, then dry_run=false to apply.",
    annotations(
        title = "Edit Symbol",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = false
    )
)]
async fn edit_symbol(
    &self,
    Parameters(params): Parameters<crate::tools::editing::EditSymbolTool>,
) -> Result<CallToolResult, McpError> {
    debug!("✏️ edit_symbol: {} {} (dry_run={})", params.operation, params.symbol, params.dry_run);
    let start = std::time::Instant::now();
    let metadata = serde_json::json!({
        "symbol": params.symbol,
        "operation": params.operation,
        "dry_run": params.dry_run,
    });
    let result = params
        .call_tool(self)
        .await
        .map_err(|e| McpError::internal_error(format!("edit_symbol failed: {}", e), None))?;
    let output_bytes = Self::output_bytes_from_result(&result);
    let source_file_paths = Self::extract_paths_from_result(&result);
    let report = ToolCallReport {
        result_count: None,
        source_bytes: None,
        output_bytes,
        metadata,
        source_file_paths,
    };
    self.record_tool_call("edit_symbol", start.elapsed(), &report);
    Ok(result)
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build 2>&1 | tail -10`

Expected: Build succeeds. If there are import issues, add the necessary `use` statements at the top of handler.rs.

- [ ] **Step 5: Commit**

```bash
git add src/handler.rs
git commit -m "feat(editing): register edit_file and edit_symbol in MCP tool router

Both tools exposed via MCP with tool call metrics recording.
edit_file: DMP fuzzy find-and-replace for any file.
edit_symbol: symbol-aware editing via indexed boundaries."
```

---

### Task 6: Security Tests

**Files:**
- Create: `src/tests/tools/editing/security_tests.rs`
- Modify: `src/tests/tools/editing/mod.rs`

- [ ] **Step 1: Write security tests**

Add to `src/tests/tools/editing/mod.rs`:

```rust
mod security_tests;
```

Create `src/tests/tools/editing/security_tests.rs`:

```rust
//! Path traversal security tests for editing tools.
//! These verify that edit_file and edit_symbol cannot write outside the workspace.

use crate::utils::secure_path_resolution;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_absolute_path_outside_workspace_rejected() {
    let workspace = TempDir::new().unwrap();
    let result = secure_path_resolution("/etc/passwd", workspace.path());
    assert!(
        result.is_err(),
        "Absolute path outside workspace should be rejected"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Security") || err.contains("traversal"),
        "Error should mention security/traversal: {}",
        err
    );
}

#[test]
fn test_relative_traversal_rejected() {
    let workspace = TempDir::new().unwrap();
    let result = secure_path_resolution("../../../../etc/passwd", workspace.path());
    assert!(
        result.is_err(),
        "Relative traversal should be rejected"
    );
}

#[test]
#[cfg(unix)]
fn test_symlink_outside_workspace_rejected() {
    use std::os::unix::fs::symlink;

    let workspace = TempDir::new().unwrap();
    let link_path = workspace.path().join("evil_link");
    symlink("/etc/passwd", &link_path).unwrap();

    let result = secure_path_resolution("evil_link", workspace.path());
    assert!(
        result.is_err(),
        "Symlink pointing outside workspace should be rejected"
    );
}

#[test]
fn test_valid_path_within_workspace_accepted() {
    let workspace = TempDir::new().unwrap();
    let file_path = workspace.path().join("src/main.rs");
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(&file_path, "fn main() {}").unwrap();

    let result = secure_path_resolution("src/main.rs", workspace.path());
    assert!(result.is_ok(), "Valid path should be accepted");
}
```

- [ ] **Step 2: Run security tests**

Run: `cargo test --lib tests::tools::editing::security_tests 2>&1 | tail -10`

Expected: PASS (these test the existing `secure_path_resolution`, which both edit tools use).

- [ ] **Step 3: Commit**

```bash
git add src/tests/tools/editing/security_tests.rs src/tests/tools/editing/mod.rs
git commit -m "test(editing): add path traversal security tests for edit tools

Verifies absolute paths, relative traversal, and symlinks outside
the workspace are all rejected by secure_path_resolution."
```

---

### Task 7: xtask Integration and Plugin Adoption

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: Julie plugin SessionStart hook content
- Modify: Julie plugin hooks configuration

- [ ] **Step 1: Add editing tests to xtask tiers**

Read `xtask/test_tiers.toml` and find the `tools-misc` bucket. Add the editing tests to its commands list:

```toml
# In [buckets.tools-misc], add to the commands array:
"cargo test --lib tests::tools::editing::edit_file_tests -- --skip search_quality",
"cargo test --lib tests::tools::editing::edit_symbol_tests -- --skip search_quality",
"cargo test --lib tests::tools::editing::security_tests -- --skip search_quality",
"cargo test --lib tests::tools::editing::markdown_section_tests -- --skip search_quality",
"cargo test --lib tests::tools::editing::transaction_tests -- --skip search_quality",
"cargo test --lib tests::tools::editing::validation_tests -- --skip search_quality",
```

- [ ] **Step 2: Run xtask dev to verify all tests pass**

Run: `cargo xtask test dev 2>&1 | tail -20`

Expected: All tiers green.

- [ ] **Step 3: Update JULIE_AGENT_INSTRUCTIONS.md**

Add `edit_file` and `edit_symbol` to the tools section and add rule 6:

```markdown
6. **Edit without reading**: Use `edit_file` or `edit_symbol` instead of Read + Edit.
   They don't require reading the file first.
   - `edit_file`: fuzzy find-and-replace (any file). Always `dry_run=true` first.
   - `edit_symbol`: edit by symbol name (code files). Operations: replace, insert_after, insert_before.
```

Add to the Tools section:

```markdown
- `edit_file`: Edit a file without reading it first. DMP fuzzy matching for old_text.
  Always `dry_run=true` first to preview.
- `edit_symbol`: Edit a symbol by name. Operations: replace (swap definition),
  insert_after, insert_before. Uses Julie's indexed boundaries.
```

- [ ] **Step 4: Add PreToolUse hook for Edit nudge**

In the Julie plugin's hooks configuration (either `.claude/hooks/hooks.json` or the plugin distribution hooks), add:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit",
        "command": "echo 'Tip: mcp__julie__edit_file and mcp__julie__edit_symbol edit without reading first, saving tokens. Consider using them instead.'"
      }
    ]
  }
}
```

- [ ] **Step 5: Create editing skill**

Create `.claude/skills/editing/SKILL.md`:

```markdown
---
name: editing
description: Use when editing code files to save tokens. Guides usage of edit_file and edit_symbol tools which don't require reading files first.
allowed-tools: mcp__julie__edit_file, mcp__julie__edit_symbol, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__fast_search
---

# Token-Efficient Editing Workflow

Use Julie's editing tools to modify files without the Read + Edit cycle.

## Workflow

1. **Understand the target** using `get_symbols` or `deep_dive`
2. **For code changes by symbol name**: use `edit_symbol`
   - `operation: "replace"` to swap an entire definition
   - `operation: "insert_after"` to add code after a symbol
   - `operation: "insert_before"` to add code before a symbol
3. **For arbitrary text changes**: use `edit_file`
   - Provide `old_text` (what to find) and `new_text` (replacement)
   - DMP fuzzy matching tolerates minor whitespace differences
   - Use `occurrence: "all"` to replace every match
4. **Always preview first**: `dry_run=true` (the default), review the diff, then `dry_run=false`
5. **Fall back to Read + Edit** only if Julie's tools can't handle the case (e.g., creating a new file)
```

- [ ] **Step 6: Update .claude/settings.local.json**

Add the new tools to the allow list:

```json
"mcp__julie__edit_file",
"mcp__julie__edit_symbol"
```

- [ ] **Step 7: Commit**

```bash
git add xtask/test_tiers.toml JULIE_AGENT_INSTRUCTIONS.md .claude/hooks/ .claude/skills/editing/ .claude/settings.local.json
git commit -m "feat(plugin): add editing tool adoption layer

SessionStart hook rule, PreToolUse nudge on built-in Edit,
editing skill with workflow guidance, xtask test integration."
```

---

### Task 8: Final Verification

- [ ] **Step 1: Run full dev test tier**

Run: `cargo xtask test dev 2>&1 | tail -20`

Expected: All green.

- [ ] **Step 2: Build release binary**

Run: `cargo build --release 2>&1 | tail -5`

Expected: Build succeeds.

- [ ] **Step 3: Verify tool registration**

The two new tools should appear in the MCP tool list when a client connects. Verify by checking the handler's tool count or inspecting the tool list output.

- [ ] **Step 4: Manual smoke test (optional)**

If possible, restart Claude Code with the new release binary and test:
1. `edit_file(file_path="some_file.rs", old_text="fn old", new_text="fn new", dry_run=true)` -- should show diff preview
2. `edit_symbol(symbol="some_function", operation="replace", content="fn some_function() { new_code() }", dry_run=true)` -- should show diff preview

- [ ] **Step 5: Final commit with version bump if needed**

```bash
git add -A
git commit -m "feat: token-saving edit tools (edit_file, edit_symbol) complete

Two DMP-powered MCP tools that let agents edit files without reading
them first. edit_file does fuzzy find-and-replace on any file.
edit_symbol edits by symbol name using indexed boundaries.
Markdown section fix enables structured reading of non-code files.
Golden master tests, security tests, plugin adoption layer included."
```
