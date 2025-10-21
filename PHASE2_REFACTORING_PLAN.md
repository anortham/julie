# Phase 2: Integration Test Refactoring Plan

## Current State Analysis

**Files to Modify:**
- `src/tests/integration/reference_workspace.rs` (3 tests, 2 ignored)

**Problems Identified:**
1. **TempDir overuse** - 4 TempDir::new() calls creating dynamic test data
2. **Sleep() timing dependencies** - 4 sleep() calls (2000ms, 1000ms delays)
3. **2 Ignored tests** that need fixing:
   - `test_reference_workspace_end_to_end` - FTS5 corruption issue
   - `test_semantic_search_workspace_filtering` - Timing issue

**Current Test Pattern:**
```rust
// BAD: Current pattern
let primary_temp = TempDir::new()?;
fs::write(primary_file, "...dynamic content...")?;
// ... index workspace ...
sleep(Duration::from_millis(2000)).await;  // ❌ Timing dependency
mark_index_ready(&handler).await;
```

---

## Refactoring Strategy

### 1. Replace TempDir with Fixtures

**Before:**
```rust
let primary_temp = TempDir::new()?;
let primary_src = primary_temp.path().join("src");
fs::create_dir_all(&primary_src)?;
fs::write(primary_src.join("file.rs"), "...")?;
```

**After:**
```rust
let primary_fixture = get_fixture_path("tiny-primary");
// No file creation needed - fixtures already exist!
```

**Helper Function:**
```rust
fn get_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/test-workspaces")
        .join(name)
}
```

**Benefits:**
- ✅ No dynamic file creation (faster)
- ✅ Predictable content for assertions
- ✅ Reusable across tests
- ✅ Already contains distinct markers (PRIMARY_WORKSPACE_MARKER, REFERENCE_WORKSPACE_MARKER)

---

### 2. Remove sleep() Calls

**Current Pattern:**
```rust
index_primary_tool.call_tool(&handler).await?;
sleep(Duration::from_millis(2000)).await;  // ❌ Guessing when ready
mark_index_ready(&handler).await;
```

**Root Cause:** Tests wait for background workspace registration

**Solution Options:**

**Option A: Synchronous Registration (Preferred)**
- Don't wait for background tasks in tests
- Call `mark_index_ready()` immediately after registration
- Tests verify registry state, not indexing completion

**Option B: Channel-based Synchronization**
- Use tokio channels to signal completion
- More complex but more realistic

**Recommended: Option A**
```rust
// GOOD: Immediate synchronization
index_primary_tool.call_tool(&handler).await?;
mark_index_ready(&handler).await;  // No sleep needed!
```

**Why This Works:**
- Tests verify registry operations (add/remove workspaces)
- Registry operations are synchronous (no background delay)
- Indexing flags are just test harness setup

---

### 3. Fix Ignored Tests

**Test 1: `test_reference_workspace_end_to_end`**

**Issue:** FTS5 corruption when searching primary workspace after adding reference

**Root Cause:** Likely timing - primary workspace DB modified while reference workspace being added

**Fix Strategy:**
1. Use fixtures (eliminates dynamic file creation race)
2. Remove sleep() (eliminates timing uncertainty)
3. Ensure mark_index_ready() called after each registration
4. Verify workspace isolation is actually working

**Test 2: `test_semantic_search_workspace_filtering`**

**Issue:** Workspace registration timing issue

**Root Cause:** Same as Test 1 - sleep() timing doesn't guarantee registration completion

**Fix Strategy:**
1. Use fixtures
2. Remove sleep()
3. Mark ready immediately after registration

---

### 4. Add Fast Workspace Isolation Smoke Tests

**New Test File:** `src/tests/integration/workspace_isolation_smoke.rs`

**Tests to Add:**
```rust
#[tokio::test]
async fn test_search_never_crosses_workspaces() -> Result<()> {
    // Use fixtures
    // Register both workspaces
    // Search primary for reference content → should find nothing
    // Search reference for primary content → should find nothing
    // Should complete in <500ms (no indexing needed)
}

#[tokio::test]
async fn test_reference_workspace_fallback() -> Result<()> {
    // Verify reference workspace search falls back correctly
    // when primary workspace doesn't have content
}

#[tokio::test]
async fn test_workspace_id_resolution() -> Result<()> {
    // Test resolve_workspace_filter() logic
    // "primary" → None (use handler.get_workspace().db)
    // "workspace_id" → Some(workspace_id) (open separate DB)
}
```

**Design Principles:**
- Fast (<500ms each)
- No sleep() calls
- Use fixtures exclusively
- Focus on isolation boundaries

---

## Implementation Steps

### Step 1: Create Helper Functions
```rust
// In src/tests/integration/mod.rs or reference_workspace.rs
fn get_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/test-workspaces")
        .join(name)
}

async fn setup_test_workspaces(handler: &JulieServerHandler)
    -> Result<(String, String)>
{
    // Register primary fixture
    // Register reference fixture
    // Mark both ready
    // Return (primary_id, reference_id)
}
```

### Step 2: Refactor Test 1 (test_invalid_reference_workspace_id_error)
- ✅ Already doesn't use fixtures (just tests error handling)
- Remove TempDir, use any path
- Simplest test - validate refactoring pattern

### Step 3: Refactor Test 2 (test_reference_workspace_end_to_end)
- Replace TempDir with fixtures
- Remove all sleep() calls
- Un-ignore
- Verify FTS5 issue is resolved

### Step 4: Refactor Test 3 (test_semantic_search_workspace_filtering)
- Replace TempDir with fixtures
- Remove all sleep() calls
- Un-ignore
- Verify timing issue is resolved

### Step 5: Add Smoke Tests
- Create workspace_isolation_smoke.rs
- Add 3 fast isolation tests
- Verify all pass in <2s total

---

## Success Criteria

**Phase 2 Complete When:**
- ✅ All 3 integration tests use fixtures (0 TempDir calls)
- ✅ All sleep() calls removed (0 remaining)
- ✅ All tests un-ignored and passing
- ✅ 3 new fast smoke tests added and passing
- ✅ Total integration test time <5s (down from ~10s+)

**Metrics:**
- TempDir usage: 4 → 0
- sleep() calls: 4 → 0
- Ignored tests: 2 → 0
- Test reliability: Flaky → Deterministic

---

## Potential Issues

**Issue 1: Fixture paths in CI**
- **Risk:** Fixtures not found in CI environment
- **Mitigation:** Use `env!("CARGO_MANIFEST_DIR")` for absolute paths

**Issue 2: Fixture content assumptions**
- **Risk:** Tests break if fixture content changes
- **Mitigation:** Document expected fixture structure in README

**Issue 3: Index state pollution**
- **Risk:** One test affects another via shared fixture state
- **Mitigation:** Each test gets fresh JulieServerHandler instance

**Issue 4: FTS5 corruption persists**
- **Risk:** Fixtures don't solve underlying FTS5 issue
- **Mitigation:** Add detailed logging to diagnose if still occurs

---

## Estimated Effort

**Time Estimate:**
- Helper functions: 15 min
- Test 1 refactor: 10 min (simplest)
- Test 2 refactor: 20 min (FTS5 issue investigation)
- Test 3 refactor: 20 min (semantic search)
- Smoke tests: 30 min
- **Total: ~1.5 hours**

**Complexity:** Medium
- Straightforward refactoring (replace TempDir → fixtures)
- May uncover underlying issues (FTS5 corruption)
- Smoke tests are new code (not just refactoring)
