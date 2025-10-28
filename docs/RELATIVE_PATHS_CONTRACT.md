# Relative Unix-Style Paths Contract

## Objective
Convert all extractors to store relative Unix-style paths instead of absolute native paths for:
- 7-12% token savings per search result
- Elimination of Windows UNC prefix spam
- Human-readable tool outputs
- Platform-independent path storage

## API Contract

### BaseExtractor::new() Signature Change

**Before:**
```rust
pub fn new(language: String, file_path: String, content: String) -> Self
```

**After:**
```rust
pub fn new(
    language: String,
    file_path: String,  // Still absolute - used for extraction
    content: String,
    workspace_root: &Path,  // NEW: Required for path conversion
) -> Self
```

### Symbol::file_path Field Contract

**Before:**
- Type: `String`
- Format: Absolute native path
- Example (Windows): `\\?\C:\Users\murphy\source\julie\src\tools\search.rs`
- Example (Linux): `/home/murphy/source/julie/src/tools/search.rs`

**After:**
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

## Implementation Pattern for Each Extractor

### Step 1: Update Constructor

```rust
// Before
impl TypeScriptExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }
}

// After
impl TypeScriptExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &Path,  // NEW parameter
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }
}
```

### Step 2: Update BaseExtractor Symbol Creation

```rust
use crate::utils::paths::to_relative_unix_style;

impl BaseExtractor {
    pub fn create_symbol(...) -> Symbol {
        Symbol {
            file_path: to_relative_unix_style(&canonical_path, workspace_root)?,
            // ... other fields
        }
    }
}
```

### Step 3: Update Extractor Instantiation in indexing/extractor.rs

```rust
// Before
let mut extractor = TypeScriptExtractor::new(
    language.to_string(),
    file_path.to_string(),
    content.to_string(),
);

// After
let workspace_root = handler.get_workspace()
    .await?
    .ok_or_else(|| anyhow!("No workspace loaded"))?
    .root;

let mut extractor = TypeScriptExtractor::new(
    language.to_string(),
    file_path.to_string(),
    content.to_string(),
    &workspace_root,  // NEW: Pass workspace_root
);
```

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

## Migration Requirements

### Affected Files (25 extractors + base)
1. `src/extractors/base.rs` - BaseExtractor::new() signature
2. All 25 language extractors:
   - rust, typescript, javascript, python, java, csharp, php, ruby, swift,
     kotlin, go, c, cpp, lua, gdscript, vue, razor, sql, html, css, regex,
     bash, powershell, zig, dart

### Affected Caller Sites
- `src/tools/workspace/indexing/extractor.rs` - All 25 extractor instantiations
- Test files - All extractor tests need workspace_root parameter

## Breaking Changes

**Database Format Change:**
- All existing symbols.db files have absolute paths
- **Requires workspace reindex** after upgrade
- No migration - document in README/CHANGELOG

## Success Criteria

- ✅ All 11 path utility tests pass
- ✅ All extractor tests pass with relative path verification
- ✅ Full test suite passes (636+ tests)
- ✅ Real-world dogfooding shows token savings
- ✅ Cross-platform validation (Windows, Linux, macOS)

---

**Status**: Phase 2.1 Complete (utilities), Phase 2.2 In Progress (extractors)
**Last Updated**: 2025-10-27
