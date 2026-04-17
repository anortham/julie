# Relative Unix-Style Paths Contract

## Status: Complete

All 34 language extractors store relative Unix-style paths. This document describes the contract and utilities.

## Motivation

Relative Unix-style paths were adopted for:
- 7-12% token savings per search result
- Elimination of Windows UNC prefix spam
- Human-readable tool outputs
- Platform-independent path storage

## API Contract

### BaseExtractor::new() Signature

```rust
pub fn new(
    language: String,
    file_path: String,  // Absolute path - used for extraction only
    content: String,
    workspace_root: &Path,  // Required for relative path conversion
) -> Self
```

### Symbol::file_path Field Contract

- Type: `String`
- Format: **Relative Unix-style path** (always `/` separators)
- Example (all platforms): `src/tools/search.rs`

### Path Conversion Utilities

**Location**: `src/utils/paths.rs`

```rust
/// Convert absolute path to relative Unix-style
pub fn to_relative_unix_style(
    absolute: &Path,
    workspace_root: &Path
) -> Result<String>

/// Convert relative Unix-style to absolute native
pub fn to_absolute_native(
    relative_unix: &str,
    workspace_root: &Path
) -> PathBuf
```

## Implementation Pattern

All extractor constructors accept `workspace_root: &Path` and pass it to `BaseExtractor::new()`:

```rust
impl TypeScriptExtractor {
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
}
```

`BaseExtractor` uses `src/utils/paths.rs::to_relative_unix_style()` internally when storing symbol file paths. The factory at `crates/julie-extractors/src/factory.rs` passes `workspace_root` through to every extractor.

## Test Contract

### Test Expectations

**Every extractor test MUST verify:**
1. Symbol.file_path is relative (no leading `/` or drive letter)
2. Symbol.file_path uses Unix separators (`/`, not `\`)
3. Symbol.file_path is within workspace (no `../` escapes)
4. Round-trip conversion works (relative → absolute → relative)

### Example Test Pattern

```rust
#[test]
fn test_typescript_extractor_stores_relative_paths() {
    let workspace_root = PathBuf::from("/home/murphy/source/julie");
    let file_path = workspace_root.join("src/tools/search.rs");
    let content = "function getUserData() { return data; }";

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        file_path.to_string_lossy().to_string(),
        content.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Contract verification
    assert_eq!(symbols[0].file_path, "src/tools/search.rs");
    assert!(!symbols[0].file_path.contains('\\'), "No backslashes");
    assert!(symbols[0].file_path.contains('/'), "Uses forward slashes");
    assert!(!symbols[0].file_path.starts_with('/'), "Not absolute");
}
```

## Error Conditions

**Files outside workspace MUST be rejected:**
```rust
to_relative_unix_style("/etc/passwd", workspace_root)
// => Err("File path is not within workspace root")
```

## Implementation

- `src/utils/paths.rs` - `to_relative_unix_style()` and `to_absolute_native()` utilities
- `crates/julie-extractors/src/base/extractor.rs` - `BaseExtractor::new()` accepts `workspace_root: &Path`
- All 34 language extractors in `crates/julie-extractors/src/` pass `workspace_root` through

---

**Status**: Complete
**Last Updated**: 2026-03-25
