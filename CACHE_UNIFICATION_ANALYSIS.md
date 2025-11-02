# Workspace Embeddings Cache Unification Analysis

## Executive Summary

Currently, Julie has **inconsistent cache location usage** causing:
- **Transient cache in temp_dir**: Handler initializes cache at `std::env::temp_dir()/julie_cache/embeddings`
- **Persistent cache in workspace**: `create_folder_structure()` creates `.julie/cache/embeddings` structure
- **Lost cache across runs**: Models downloaded to temp_dir are deleted between sessions
- **Unpredictable cleanup**: No systematic cleanup mechanism

This analysis documents the current state, proposes a unified strategy, and provides an implementation roadmap.

---

## Current State Analysis

### 1. Cache Location Usage Map

#### Location 1: Handler::initialize_embedding_engine (src/handler.rs:198)
```rust
// ❌ PROBLEM: Uses system temp directory (transient)
let cache_dir = std::env::temp_dir().join("julie_cache").join("embeddings");
std::fs::create_dir_all(&cache_dir)?;

let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
```

**Issues:**
- System temp directory is cleaned up periodically
- Cache lost between MCP server restarts
- No persistence across runs
- Blocks integration tests that expect cache to work

**Called by:**
- `handler.rs:165` - `initialize_embedding_engine()` when embedding engine first needed
- Used after workspace initialization

#### Location 2: Embeddings Indexing Background Task (src/tools/workspace/indexing/embeddings.rs:307)
```rust
// ✅ IMPROVEMENT: Uses workspace directory with fallback
let cache_dir = if let Some(root) = workspace_root {
    root.join(".julie").join("cache").join("embeddings")
} else {
    std::env::temp_dir().join("julie_cache").join("embeddings")
};

std::fs::create_dir_all(&cache_dir)?;
```

**Status:**
- Already uses workspace-level cache when available
- Has fallback to temp_dir if workspace not available
- But inconsistent with Handler's temp_dir-only approach

#### Location 3: Workspace Folder Structure (src/workspace/mod.rs:223-226)
```rust
// ✅ CORRECT: Creates persistent cache structure
let folders = [
    julie_dir.join("indexes"),
    julie_dir.join("models"),  // Cached FastEmbed models (shared)
    julie_dir.join("cache"),   // File hashes and parse cache (shared)
    julie_dir.join("cache").join("embeddings"),  // ← For embedding models
    julie_dir.join("cache").join("parse_cache"),
    julie_dir.join("logs"),
    julie_dir.join("config"),
];
```

**Status:**
- Creates `.julie/cache/embeddings` directory
- But Handler doesn't use it!
- Creates mismatch between created structure and actual usage

#### Location 4: Test Files
```rust
// src/tests/integration/tracing.rs:160
let cache_dir = temp_dir.path().join("cache");

// src/tests/integration/watcher.rs:99
let cache_dir = workspace_root.join(".julie/cache");
```

**Status:**
- Inconsistent: some tests use temp dirs, others use workspace
- Tests reflect the inconsistency in production code

### 2. Path Structure Architecture

**Workspace Structure Created:**
```
<root>/.julie/
├── indexes/              # Per-workspace databases + vectors
├── models/               # Shared ONNX model files (downloaded)
├── cache/                # Shared cache directory
│   ├── embeddings/       # ← CREATED but NOT USED by Handler
│   └── parse_cache/
├── logs/
└── config/
```

**Current Actual Usage:**
```
Handler creates:        /tmp/julie_cache/embeddings/  (system temp)
Embeddings task uses:   <root>/.julie/cache/embeddings/  (workspace)
Folder structure:       <root>/.julie/cache/embeddings/  (workspace)
```

**Result:** Triple mismatch - three different cache strategies in one codebase!

---

## Problem Statement

### The Core Issue
The workspace structure is **created but never used** by the Handler. This causes:

1. **Lost Cache Across Runs**
   - Models downloaded to `/tmp/julie_cache/`
   - `/tmp` is cleaned up at system reboot or periodic cleanup
   - Each restart requires re-downloading ONNX models (~200MB)
   - Wastes bandwidth and increases startup time

2. **Unpredictable Cleanup**
   - No systematic cleanup of cache
   - Temp directory cleanup is OS-dependent
   - No way to manually clear cache (except deleting `/tmp`)
   - Accumulates orphaned files

3. **Test Inconsistency**
   - Tests can't predict cache location
   - Some tests use temp_dir, others use workspace
   - Makes it hard to debug cache-related issues

4. **Architectural Misalignment**
   - Workspace creates infrastructure not used
   - Handler bypasses workspace cache structure
   - Embeddings task has to compensate with fallback logic
   - Violation of single source of truth principle

### Impact on Users
- **Development**: Must re-download models on every restart
- **Production**: Cache lost on system reboot
- **Testing**: Flaky tests due to inconsistent cache behavior
- **Maintenance**: Multiple places to look for cache issues

---

## Unified Caching Strategy Design

### Design Goals
1. **Single Cache Location**: All embeddings cache in `.julie/cache/embeddings/`
2. **Persistent Across Runs**: Cache survives MCP server restarts
3. **Workspace-Aware**: Use workspace structure when available
4. **Graceful Fallback**: Handle edge cases (workspace not ready)
5. **Predictable Cleanup**: Systematic cache lifecycle

### Proposed Architecture

#### 1. Add Cache Path Method to JulieWorkspace
```rust
// In src/workspace/mod.rs - NEW METHOD
impl JulieWorkspace {
    /// Get the embedding cache directory for this workspace
    /// Returns .julie/cache/embeddings/
    pub fn get_embedding_cache_dir(&self) -> PathBuf {
        self.julie_dir.join("cache").join("embeddings")
    }

    /// Ensure embedding cache directory exists
    pub fn ensure_embedding_cache_dir(&self) -> Result<PathBuf> {
        let cache_dir = self.get_embedding_cache_dir();
        std::fs::create_dir_all(&cache_dir)?;
        Ok(cache_dir)
    }

    /// Get all cache directories for cleanup (including parse_cache, etc)
    pub fn get_all_cache_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.julie_dir.join("cache").join("embeddings"),
            self.julie_dir.join("cache").join("parse_cache"),
        ]
    }

    /// Clear embedding cache (idempotent)
    /// Useful for recovery from corrupted cache
    pub fn clear_embedding_cache(&self) -> Result<()> {
        let cache_dir = self.get_embedding_cache_dir();
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir)?;
            std::fs::create_dir_all(&cache_dir)?;
        }
        Ok(())
    }
}
```

#### 2. Update Handler to Use Workspace Cache
```rust
// In src/handler.rs - REPLACE handler.rs:198
// BEFORE:
let cache_dir = std::env::temp_dir().join("julie_cache").join("embeddings");

// AFTER:
let workspace_guard = self.workspace.read().await;
let workspace = workspace_guard
    .as_ref()
    .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;

let cache_dir = workspace.ensure_embedding_cache_dir()?;
```

#### 3. Align Embeddings Indexing Task
```rust
// In src/tools/workspace/indexing/embeddings.rs - SIMPLIFY
// BEFORE: Complex fallback logic
let cache_dir = if let Some(root) = workspace_root {
    root.join(".julie").join("cache").join("embeddings")
} else {
    std::env::temp_dir().join("julie_cache").join("embeddings")
};

// AFTER: Always use workspace (always available here)
let cache_dir = workspace_root
    .join(".julie")
    .join("cache")
    .join("embeddings");
```

#### 4. Update Tests for Consistency
```rust
// Pattern for all test files:
// 1. Initialize workspace with JulieWorkspace::initialize()
// 2. Get cache dir from workspace.get_embedding_cache_dir()
// 3. Never use temp_dir for cache

// Before:
let cache_dir = temp_dir.path().join("cache");

// After:
let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).await?;
let cache_dir = workspace.ensure_embedding_cache_dir()?;
```

### Cache Persistence Guarantee

With this design:
- Cache is stored in `.julie/cache/embeddings/`
- `.julie/` is managed by workspace lifecycle
- Cache persists across MCP server restarts
- Cache is deleted only when workspace is deleted
- Explicit cleanup via `workspace.clear_embedding_cache()` if needed

### Cleanup Strategy

**Automatic Cleanup** (integrated into workspace operations):
```rust
// When loading workspace
pub async fn detect_and_load(start_path: PathBuf) -> Result<Option<Self>> {
    // ... existing logic ...
    // Could add: workspace.verify_cache_integrity()?
}

// When reinitializing workspace
pub async fn initialize_with_force(root: PathBuf) -> Result<Self> {
    // ... existing logic ...
    // Could add: clear_embedding_cache() for fresh start
}
```

**Manual Cleanup** (for users who want to reclaim space):
```rust
// User can call this via tool or directly:
workspace.clear_embedding_cache()?;
```

---

## Implementation Changes Required

### File Changes

#### 1. `src/workspace/mod.rs` (Add cache methods)
**Lines to add:** ~80 lines
- `get_embedding_cache_dir()` method
- `ensure_embedding_cache_dir()` method
- `get_all_cache_dirs()` method
- `clear_embedding_cache()` method
- Documentation

**Impact:** Minimal - just adding new public methods

#### 2. `src/handler.rs` (Use workspace cache)
**Lines to change:** ~15 lines
- Line 198: Replace temp_dir usage with workspace cache
- Get workspace reference before cache initialization
- Use `workspace.ensure_embedding_cache_dir()?`

**Impact:** Low - single initialization path

#### 3. `src/tools/workspace/indexing/embeddings.rs` (Simplify fallback)
**Lines to change:** ~10 lines
- Line 307-311: Simplify to always use workspace
- Remove fallback to temp_dir (always have workspace here)

**Impact:** Low - simplification

#### 4. `src/tests/integration/tracing.rs` (Fix test)
**Lines to change:** ~5-10 lines
- Line 160: Use workspace cache instead of temp_dir
- Ensure workspace initialized properly

**Impact:** Low - test fix

#### 5. `src/tests/integration/watcher.rs` (Verify correct usage)
**Lines to change:** ~0 lines
- Already using workspace cache correctly
- Just verify it works with new unified approach

**Impact:** None - already correct

### Total Scope
- **4-5 files modified**
- **~120 lines changed/added** (mostly additions)
- **0 breaking changes to public API**
- **100% backward compatible** (old cache ignored, new cache used)

---

## Testing Strategy

### Unit Tests
- `workspace::tests::test_get_embedding_cache_dir` - Path is correct
- `workspace::tests::test_ensure_embedding_cache_dir` - Directory created
- `workspace::tests::test_clear_embedding_cache` - Cache cleared properly

### Integration Tests
- Verify cache persists between workspace restarts
- Verify cache location matches expected structure
- Verify Handler uses workspace cache
- Verify embeddings task uses workspace cache

### Manual Validation
1. Build and run: `cargo build --release`
2. Verify cache created at: `.julie/cache/embeddings/`
3. Restart MCP server, verify cache reused
4. Check models aren't re-downloaded on second start

---

## Migration Path

### Phase 1: Add Methods (Non-Breaking)
1. Add `get_embedding_cache_dir()` to JulieWorkspace
2. Keep existing Handler code unchanged
3. All tests pass (no functional change yet)

### Phase 2: Update Handler
1. Update Handler to use workspace cache
2. Old cache in temp_dir becomes orphaned (harmless)
3. New cache created in workspace (persistent)

### Phase 3: Update Tests
1. Update tests to use workspace cache
2. Tests now more realistic (match production)
3. All tests pass

### Phase 4: Cleanup (Optional Future)
1. Add tool to clear orphaned temp cache
2. Add telemetry to monitor cache hits/misses
3. Add cache size monitoring

---

## Risks and Mitigations

### Risk: Workspace Not Available
**Scenario:** Handler tries to use workspace cache before workspace initialized
**Mitigation:** Handler always initializes workspace first (current pattern)
**Verification:** Check handler.rs initialization order

### Risk: Cache Directory Corruption
**Scenario:** Corrupted cache files prevent model loading
**Mitigation:** Add `clear_embedding_cache()` for recovery
**Verification:** Test with manually corrupted cache files

### Risk: Disk Space
**Scenario:** Cache grows too large (ONNX models ~200MB)
**Mitigation:** Document cache size, add monitoring, provide cleanup
**Verification:** Check cache directory size regularly

### Risk: Permissions
**Scenario:** Can't write to `.julie/cache/` (read-only workspace)
**Mitigation:** Graceful fallback to temp_dir if workspace cache unavailable
**Verification:** Test with read-only `.julie` directory

---

## Success Criteria

- [x] Cache location analysis complete
- [ ] Unified strategy designed and documented
- [ ] `JulieWorkspace` has cache methods added
- [ ] Handler uses workspace cache
- [ ] Embeddings indexing simplified
- [ ] All tests updated to use workspace cache
- [ ] Tests pass (100% pass rate)
- [ ] Cache persists across server restarts
- [ ] No cache lost on system reboot
- [ ] Cleanup mechanism documented

---

## References

### Related Code Locations
- **Workspace**: `src/workspace/mod.rs` (lines 40-350)
- **Handler**: `src/handler.rs` (lines 165-212)
- **Embeddings**: `src/tools/workspace/indexing/embeddings.rs` (lines 300-320)
- **Tests**: `src/tests/integration/tracing.rs` (lines 150-165)
- **Tests**: `src/tests/integration/watcher.rs` (lines 90-120)

### Workspace Structure Docs
- See CLAUDE.md section "Workspace Storage Architecture"
- `.julie/` directory structure layout
- Cache directory purpose and organization

---

**Analysis completed**: 2025-11-02
**Next step**: Implement Phase 1 (Add cache methods to JulieWorkspace)
