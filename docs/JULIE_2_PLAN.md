# Julie 2.0: Unified Project Intelligence System

**📊 PROJECT STATUS (2025-11-11)**
- ✅ **Phase 1: Immutable Memory System** - **COMPLETE**
  - checkpoint/recall tools fully functional
  - .memories/ architecture implemented
  - Text search working on Windows (critical bug fixed)
  - 26/26 memory tests passing
  - **Goldfish deprecated** - Julie's memory system is the replacement
- ✅ **Phase 1.5: Mutable Plans** - **COMPLETE**
  - plan() tool with 6 actions (save/get/list/activate/update/complete)
  - Atomic updates with temp + rename pattern
  - SQL views for plan searchability
  - 22 unit tests + 8 integration tests passing
  - Custom slash commands (/checkpoint, /recall) implemented
- ✅ **Phase 2: Memory Embeddings Optimization** - **COMPLETE (v1.6.1)**
  - 88.7% embedding reduction for .memories/ files (355 → 40 per workspace)
  - Custom RAG pipeline: "{type}: {description}" format
  - Critical bug fixes: search ranking (2.0x boost) + escaped quotes (serde_json)
  - 7 comprehensive tests passing
  - 80% database savings with production-ready parsing
- 🔄 **Phase 3: Skills System** - **COMPLETE (Modified Approach)**
  - Skills implemented in Julie AND Sherpa (complementary, not replacement)
  - Skills bridge Julie (code+memory intelligence) with Sherpa (workflow orchestration)
  - Behavioral adoption approach > Hooks for agent usage patterns
- ⏸️ **Phase 4: Cross-Workspace Intelligence** - **DEFERRED**
  - Not a priority - focus on polishing existing capabilities
  - Reference workspaces already provide multi-project search
  - May revisit if strong user demand emerges

## Executive Summary

Julie has evolved from a code intelligence tool into a comprehensive project intelligence system that integrates code search with project memory. Through strategic consolidation and complementary tool design, we've created a focused, maintainable architecture.

**Vision**: A code+memory intelligence backend that understands not just *what* your code does, but *why* it exists and *how* you've worked with it over time.

**Current Architecture** (2025-11-11):
- **Julie**: Code intelligence + Project memory (checkpoint/recall/plan)
  - Replaced Goldfish with native memory system ✅
  - Integrated mutable plans for task tracking ✅
  - Optimized embeddings for RAG performance ✅
- **Sherpa**: Workflow orchestration (systematic development guidance)
  - Remains separate - different concern (process vs intelligence)
  - Skills bridge Julie and Sherpa (complementary, not replacement)
- **Skills**: Workflow templates that leverage both Julie and Sherpa
  - Implemented in both tools where appropriate
  - Drive agent behavior through behavioral adoption patterns

**What Changed from Original Plan:**
- ❌ **Not replacing Sherpa** - It solves a different problem (systematic workflows vs code intelligence)
- ✅ **Goldfish replaced** - Julie's memory system is superior (git-tracked, project-level)
- ⏸️ **Cross-workspace deferred** - Reference workspaces already provide this, focus on polish first
- ✅ **Skills as bridges** - Connect tools instead of replacing them

## Architecture Overview

### Core Principles
1. **Simplicity First**: Leverage existing Julie infrastructure, minimal changes
2. **Project-Level Storage**: Memories live with code, git-trackable
3. **Progressive Enhancement**: Each phase builds on the previous
4. **No Breaking Changes**: Existing Julie functionality remains intact

### Key Design Decisions

**Individual JSON Files (Not JSONL):**
- ✅ Perfect git mergeability (separate files = no conflicts)
- ✅ Human-readable (pretty-printed with indentation)
- ✅ Easy file operations (atomic write via temp + rename)
- ✅ Manual inspection/editing possible
- ❌ More files (mitigated by per-day directories)

**Flexible Schema (Minimal Core + Flatten):**
- Required: `id`, `timestamp`, `type` (3 fields only)
- Optional common: `git` context
- Everything else: type-specific via serde `flatten`
- No schema validation - pure flexibility
- Enables new memory types without breaking changes

**Immutable First, Mutable Later:**
- Phase 1: Append-only memories (checkpoint, decision, learning)
- Phase 1.5: Mutable plans (task tracking, status updates)
- Rationale: Validate foundation before adding complexity

**`.memories/` Directory (Not `.julie/memories/`):**
- ✅ Clear separation: `.julie/` = ephemeral cache, `.memories/` = permanent records
- ✅ Users can delete `.julie/` to rebuild without losing memories
- ✅ Simple indexing: Just whitelist `.memories` as a known dotfile
- ✅ No complex path exceptions or special cases in discovery logic
- ✅ Matches conventions like `.git`, `.vscode` (tool-specific, but user data)

**Tool Names:**
- `checkpoint` - Save immutable memory (clear "snapshot" semantics)
- `recall` - Retrieve any memory type (works for both immutable/mutable)
- `plan` - Create/update mutable plans (distinct from checkpoint)

### Storage Architecture

```
# Project Level (with code)
<project>/.memories/              # NEW: Project memories (individual JSON files)
├── 2025-01-09/
│   ├── 143022_abc123.json
│   ├── 150534_def456.json
│   └── 163012_ghi789.json
├── 2025-01-10/
│   └── 093012_jkl012.json
└── plans/                        # Mutable plans (Phase 2)
    ├── plan_add-search.json
    └── plan_refactor-db.json

<project>/.julie/                 # Julie's internal state (ephemeral, can be deleted)
├── indexes/
│   └── {workspace_id}/
│       ├── db/symbols.db         # Existing + memory views
│       └── vectors/              # HNSW index (code + memories)
└── workspace_registry.json       # Existing

# User Level (cross-project)
~/.julie/
└── workspace_registry.json       # NEW: All registered workspaces
```

### Data Flow

```
Memory Creation (Immutable):
User → checkpoint tool → Pretty-printed JSON file → File watcher → Tree-sitter → symbols.db → Embeddings → HNSW

Memory Creation (Mutable - Phase 2):
User → plan tool → Pretty-printed JSON file → Update in-place → Reindex

Memory Recall:
User → recall tool → SQL view → Chronological results
User → fast_search → FTS5/HNSW → Unified results (code + memories)

Cross-Workspace:
User → search --all-workspaces → Registry → Parallel queries → Merged results
```

---

## Phase 1: Immutable Memory System

### Goals
- Add **immutable** memory capabilities (checkpoints, decisions, learnings)
- Store memories as pretty-printed JSON files (one per memory, organized by day)
- Enable both chronological recall and semantic search of memories
- Keep memories git-trackable and human-readable for team knowledge sharing
- **Defer mutable plans to Phase 2** - start simple with append-only semantics

### Implementation

#### 1.1 Memory Storage Format

**Storage Structure:**
```
.memories/                      # ✅ IMPLEMENTED - Clean separation from .julie/
├── 2025-01-09/
│   ├── 143022_abc123.json    # Individual memory files
│   ├── 150534_def456.json    # Pretty-printed for readability
│   └── 163012_ghi789.json    # Git-mergeable (separate files)
└── 2025-01-10/
    └── 093012_jkl012.json
```

**Schema Philosophy:**
- **Minimal Core**: Only 3 required fields (id, timestamp, type)
- **Optional Common**: git context (useful across all types)
- **Type-Specific**: Everything else depends on memory type
- **Flexible**: No schema enforcement, use `serde flatten` for extensibility

**Example - Checkpoint Memory:**
```json
{
  "id": "mem_1736422822_abc123",
  "timestamp": 1736422822,
  "type": "checkpoint",
  "description": "Fixed race condition in auth flow by adding mutex",
  "tags": ["bug", "auth", "concurrency"],
  "git": {
    "branch": "fix/auth-race",
    "commit": "abc123def",
    "dirty": false,
    "files_changed": ["src/auth.rs", "src/lib.rs"]
  }
}
```

**Example - Decision Memory:**
```json
{
  "id": "dec_1736423000_xyz789",
  "timestamp": 1736423000,
  "type": "decision",
  "question": "Which database for memory storage?",
  "chosen": "SQLite with JSON extraction",
  "alternatives": ["Separate JSONL parser", "Postgres"],
  "rationale": "Leverage existing indexing, zero new dependencies",
  "git": {
    "branch": "feature/memory-system",
    "commit": "def456abc",
    "dirty": true
  }
}
```

**Rust Implementation:**
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Memory {
    id: String,
    timestamp: i64,
    #[serde(rename = "type")]
    memory_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<GitContext>,

    // Everything else flattened at top level (flexible schema)
    #[serde(flatten)]
    extra: serde_json::Value,
}

// Serialization: one line
let json = serde_json::to_string_pretty(&memory)?;
std::fs::write(path, json)?;
```

#### 1.2 New Tools (Phase 1)

**checkpoint** - Save an immutable memory
```rust
// MCP tool parameters
{
  "description": "Fixed auth race condition by adding mutex",
  "tags": ["bug", "auth", "concurrency"],        // optional
  "type": "checkpoint"                            // optional, defaults to "checkpoint"
}

// Other type examples:
// type: "decision" - architectural decisions
// type: "learning" - insights discovered
// type: "observation" - noteworthy patterns
```

**recall** - Retrieve memories
```rust
// MCP tool parameters
{
  "limit": 10,                    // max results
  "since": "2025-01-01",          // date filter (optional)
  "tags": ["bug", "auth"],        // tag filter (optional)
  "type": "checkpoint"            // type filter (optional)
}

// Recall integrates with fast_search for semantic queries
// "recall --semantic 'auth bug'" becomes fast_search with .julie/memories/ filter
```

**Note:** `plan` tool deferred to Phase 2 (mutable memories)

#### 1.3 Database Enhancements

Add SQL view for memories in symbols.db:

```sql
-- View leveraging existing files table + JSON extraction
CREATE VIEW memories AS
SELECT
  f.path,
  f.content_hash,
  json_extract(f.content, '$.id') as id,
  json_extract(f.content, '$.timestamp') as timestamp,
  json_extract(f.content, '$.type') as type,
  json_extract(f.content, '$.description') as description,  -- type-specific field
  json_extract(f.content, '$.tags') as tags,                -- type-specific field
  json_extract(f.content, '$.git.branch') as git_branch,
  json_extract(f.content, '$.git.commit') as git_commit,
  json_extract(f.content, '$.git.dirty') as git_dirty
FROM files f
WHERE f.path LIKE '.memories/%'
  AND f.path LIKE '%.json'
  AND f.path NOT LIKE '%.memories/plans/%';  -- Exclude mutable plans (Phase 2)

-- Index for chronological queries (fast recall)
CREATE INDEX idx_memories_timestamp ON files(
  json_extract(content, '$.timestamp')
) WHERE path LIKE '.memories/%' AND path LIKE '%.json';

-- Index for type filtering
CREATE INDEX idx_memories_type ON files(
  json_extract(content, '$.type')
) WHERE path LIKE '.memories/%' AND path LIKE '%.json';
```

**Why This Works:**
- Reuses existing `files` table (already indexed by tree-sitter)
- JSON extraction is fast (SQLite's json_extract is optimized)
- FTS5 already indexes all text fields (description, tags, etc.)
- No schema changes needed - just a view + indexes

#### 1.4 Tree-Sitter Integration ✅ **COMPLETE**

**JSON files already supported!** No changes needed to tree-sitter.

Julie's existing JSON extractor:
- Parses each `.json` file in `.memories/` ✅
- Extracts all fields into the `files` table ✅
- Indexes text content in FTS5 for full-text search ✅
- Generates embeddings for semantic search ✅

**Benefit:** Zero new code - memories are just JSON files that get indexed like any other code file.

**Note:** `.memories/` is whitelisted in discovery logic for automatic indexing.

#### 1.5 Git Integration

Capture git context automatically when creating memories:

```rust
pub fn get_git_context(workspace_root: &Path) -> Option<GitContext> {
    // Use existing git integration from workspace module
    let repo = gix::open(workspace_root).ok()?;

    Some(GitContext {
        branch: repo.head().ok()?.name()?.as_bstr().to_string(),
        commit: repo.head().ok()?.id().to_string(),
        dirty: !repo.is_clean()?,
        files_changed: get_changed_files(&repo),
    })
}
```

**Why git context matters:**
- Links memories to code state
- Enables "what was I working on?" queries
- Team collaboration: see what branch a decision was made on

### Phase 1 Deliverables (Immutable Memories Only) ✅ **COMPLETE**
- [x] Memory data structures with flexible schema (serde flatten) ✅
- [x] checkpoint tool implementation (save immutable memories) ✅
- [x] recall tool implementation (chronological + type/tag filtering) ✅
- [x] JSON file writer with atomic operations (temp file + rename) ✅
- [x] SQL views and indexes for memory queries ✅
- [x] Git context capture integration ✅
- [x] Integration tests for memory operations (26/26 passing) ✅
- [x] Documentation and examples ✅

**CRITICAL BUG FIX (2025-11-10):**
- Fixed Windows file_pattern GLOB bug in `src/database/files.rs:376-390`
- **Issue**: Platform-specific normalization converted forward slashes to backslashes on Windows, breaking GLOB matching
- **Root Cause**: Violated RELATIVE_PATHS_CONTRACT.md - database stores Unix-style paths with forward slashes
- **Solution**: Removed normalization entirely - user patterns work as-is with workspace-relative storage
- **Impact**: Enabled text search on memory files for Windows users
- **Tests**: Added regression test `test_fts_file_pattern_forward_slash_glob_matching` with 5 test cases

**Deferred to Phase 2:**
- [ ] plan tool (mutable memories with update operations)
- [ ] File watching for live reindexing of plan updates

### Success Metrics
- Checkpoint save: <50ms (includes git context + file write)
- Chronological recall: <5ms (SQL view query)
- Semantic recall: <100ms (existing fast_search performance)
- Zero impact on existing tool performance
- Human-readable JSON files (can edit with text editor)
- Git-friendly (no merge conflicts on concurrent work)

### Why Immutable First?

**Simplicity:**
- Append-only semantics (no update logic needed)
- No concurrency concerns (never modify existing files)
- Easy to reason about (write once, never change)

**Foundation:**
- Gets core storage/indexing working
- Validates flexible schema approach
- Establishes SQL view patterns
- Tests git integration

**Phase 2 builds on this:**
- Same storage structure (`.memories/`) ✅
- Same indexing pipeline (tree-sitter → SQLite → HNSW) ✅
- Just adds: update operations + mutable subdirectory

---

## Phase 1.5: Mutable Plans (Bridge to Phase 2)

**Goals:**
- Add mutable "plan" memories that can be updated
- Keep same storage/indexing infrastructure
- Enable task tracking and status updates

**Key Differences from Immutable:**

| Aspect | Immutable (checkpoint) | Mutable (plan) |
|--------|----------------------|----------------|
| Storage | `memories/YYYY-MM-DD/timestamp_id.json` | `memories/plans/plan_{id}.json` |
| Filename | Includes timestamp (unique) | Stable ID (updateable) |
| Operations | Write once | Write + Update |
| Git merges | Perfect (separate files) | Good (plans usually single-author) |

**Implementation:**
```rust
// New tool: plan
pub async fn plan_tool(action: PlanAction) -> Result<String> {
    match action {
        PlanAction::Create { title, content } => {
            // Create new plan file
            let plan = Memory {
                id: format!("plan_{}", generate_id()),
                timestamp: now(),
                memory_type: "plan".into(),
                // ... plan-specific fields
            };
            write_plan_file(&plan)?;
        }
        PlanAction::Update { id, updates } => {
            // Read existing plan
            let mut plan = read_plan_file(&id)?;
            // Apply updates (mark tasks complete, change status, etc.)
            apply_updates(&mut plan, updates)?;
            // Atomic write (temp + rename)
            write_plan_file(&plan)?;
        }
    }
}
```

**Deliverables:** ✅ **COMPLETE (v1.5.1 - 2025-11-10)**
- [x] plan tool with 6 actions (save, get, list, activate, update, complete) ✅
- [x] Plan-specific update logic (status changes, content updates) ✅
- [x] Atomic file updates with temp + rename pattern ✅
- [x] SQL views for plan searchability ✅
- [x] One active plan enforcement ✅
- [x] Stable filenames (plan_slug.json) ✅
- [x] 22 unit tests + 8 integration tests (30 total) ✅
- [x] 3 additional serialization tests for case sensitivity fix ✅

**v1.5.1 Release Notes:**
- Fixed plan tool JSON Schema case sensitivity bug (explicit per-variant serde rename)
- Fixed query preprocessor phrase handling (preserve quoted phrases)
- Enhanced ignore patterns (Gradle, Dart, Next.js, Nuxt, CMake)
- Fixed 57GB RAM usage from .NET build artifact indexing
- Fixed FTS5 query syntax error (implicit AND)
- Test pass rate: 1652/1661 (99.5%)
- Git pre-commit hook for automatic memory file staging

---

## Phase 2: Memory Embeddings Optimization

### Goals ✅ **COMPLETE (v1.6.1 - 2025-11-11)**
- Optimize embedding generation for .memories/ files
- Implement custom RAG pipeline for memory content
- Reduce database size and improve search quality
- Fix critical bugs in memory search ranking and parsing

### Implementation

#### 2.1 Custom RAG Pipeline for Memories

**Problem Identified:**
- Standard code embeddings were suboptimal for memory files
- Every JSON field (id, timestamp, tags, description, etc.) got separate embeddings
- Result: 5-10 embeddings per memory file, most were noise

**Solution:**
```rust
// Custom embedding pipeline for .memories/ files
fn build_memory_embedding_text(&self, symbol: &Symbol) -> String {
    // Only embed "description" symbols - skip id, timestamp, tags, etc.
    if symbol.name != "description" {
        return String::new(); // Empty = skip embedding
    }

    // Extract type and description from JSON
    let type_value = extract_json_string_value(&symbol.code_context, "type")
        .unwrap_or_else(|| "checkpoint".to_string());
    let description = extract_json_string_value(&symbol.code_context, "description")
        .unwrap_or_else(|| symbol.name.clone());

    // Focused embedding: "{type}: {description}"
    format!("{}: {}", type_value, description)
}
```

**Results:**
- 88.7% reduction: 355 symbols → 40 embeddings per workspace
- 1 focused embedding per memory file (vs 5-10 scattered)
- 80% database savings
- Clearer semantic search (one concept per embedding)

#### 2.2 Critical Bug Fixes

**Bug #1: Search Ranking Penalty**
- **Issue**: Memory descriptions got 0.8x penalty (Variable kind), ranked 3x lower than expected
- **Fix**: Special case in `get_symbol_kind_boost()` for `.memories/` JSON description symbols
- **Result**: 2.0x boost (same as functions), memories rank 2.5x higher

**Bug #2: Escaped Quotes**
- **Issue**: Original `find('"')` implementation truncated descriptions with quotes
  - Input: `"Fixed \"auth\" bug"` → Output: `Fixed \` ❌
- **Fix**: Use `serde_json::Deserializer` for robust JSON parsing
- **Result**: Handles escaped quotes (`\"`), backslashes (`\\`), unicode (`\u0041`)

#### 2.3 Test Coverage

7 comprehensive tests:
1. `test_memory_embedding_text_checkpoint` - Checkpoint format
2. `test_memory_embedding_text_decision` - Decision format
3. `test_memory_embedding_skips_non_description_symbols` - Filtering
4. `test_memory_embedding_excludes_mutable_plans` - Plan exclusion
5. `test_memory_embedding_handles_missing_type_field` - Graceful degradation
6. `test_standard_code_symbols_unchanged` - No regression
7. `test_memory_embedding_handles_escaped_quotes` - JSON edge cases

Plus 1 semantic scoring test:
- `test_memory_description_symbol_gets_boost` - Validates 2.0x boost

### Deliverables ✅ **ALL COMPLETE**
- [x] Audit embedding pipeline and identify bottlenecks
- [x] Implement custom RAG pipeline for .memories/ files
- [x] Filter empty embedding text in batch processing
- [x] Fix search ranking penalty for memory descriptions
- [x] Replace string parsing with serde_json streaming deserializer
- [x] Comprehensive test coverage (7 tests)
- [x] Documentation and release (v1.6.1)

### Success Metrics ✅ **ALL ACHIEVED**
- Embedding reduction: 88.7% (355 → 40)
- Database savings: 80% for memory files
- Search ranking: 2.5x improvement
- JSON parsing: Production-ready (handles all edge cases)
- Zero performance regression on code embeddings

---

## Phase 3: Cross-Workspace Intelligence

### Status: ⏸️ **DEFERRED** (Not a current priority)

**Decision Rationale:**
- Reference workspaces already provide multi-project search capabilities
- Most developers actively work in 1-3 projects, not 10+
- Cross-workspace adds significant complexity for uncertain ROI
- **Focus on polish** > new features right now

**What Already Works:**
- Julie's reference workspace system lets you search other projects
- `workspace` parameter in search tools filters by specific workspace
- Memory system works great within a single project (where most work happens)

### Original Goals (For Future Reference)
- Enable searching across all projects from any workspace
- Create unified view of developer's knowledge
- Support cross-project patterns and learnings
- Maintain workspace isolation when needed

### Implementation Details (Archived - For Future Reference)

<details>
<summary>Click to expand original implementation plan</summary>

#### 3.1 Workspace Registry

Create `~/.julie/workspace_registry.json`:

```json
{
  "version": "2.0",
  "workspaces": {
    "julie_95d84a94": {
      "path": "c:\\source\\julie",
      "name": "julie",
      "last_seen": "2025-01-10T10:30:00Z",
      "last_indexed": "2025-01-10T09:15:00Z",
      "stats": {
        "symbol_count": 10976,
        "memory_count": 47,
        "file_count": 771,
        "has_memories": true
      }
    },
    "tusk_abc12345": {
      "path": "c:\\source\\tusk",
      "name": "tusk",
      "last_seen": "2025-01-09T15:45:00Z",
      "last_indexed": "2025-01-09T14:30:00Z",
      "stats": {
        "symbol_count": 3200,
        "memory_count": 112,
        "file_count": 89,
        "has_memories": true
      }
    }
  }
}
```

#### 2.2 Auto-Registration

On Julie startup, register workspace:

```rust
impl JulieWorkspace {
    pub fn register_with_global(&self) -> Result<()> {
        let registry_path = home_dir().join(".julie/workspace_registry.json");
        let mut registry = WorkspaceRegistry::load_or_create(registry_path)?;

        registry.update_workspace(WorkspaceEntry {
            id: self.workspace_id.clone(),
            path: self.root.clone(),
            name: self.name.clone(),
            last_seen: Utc::now(),
            last_indexed: self.last_indexed,
            stats: WorkspaceStats {
                symbol_count: self.get_symbol_count()?,
                memory_count: self.get_memory_count()?,
                file_count: self.get_file_count()?,
                has_memories: self.memories_dir.exists(),
            }
        });

        registry.save()?;
        Ok(())
    }
}
```

#### 2.3 Cross-Workspace Tools

Add `--all-workspaces` flag to existing tools:

```rust
// Single workspace (default)
julie fast_search "auth implementation"

// All registered workspaces
julie fast_search "auth implementation" --all-workspaces

// Specific workspaces
julie fast_search "auth implementation" --workspaces julie,tusk

// Cross-workspace recall
julie recall --all-workspaces --since "2025-01-01"
```

#### 2.4 Query Orchestration

```rust
pub struct CrossWorkspaceSearch {
    registry: WorkspaceRegistry,
    max_parallel: usize,
}

impl CrossWorkspaceSearch {
    pub async fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchResult> {
        // Get target workspaces
        let workspaces = match options.workspaces {
            WorkspaceTarget::Current => vec![self.current_workspace()],
            WorkspaceTarget::All => self.registry.all_workspaces(),
            WorkspaceTarget::Specific(ids) => self.registry.get_workspaces(ids),
        };

        // Create parallel queries
        let futures = workspaces.iter().map(|ws| {
            self.search_workspace(ws, query.clone())
        });

        // Execute with concurrency limit
        let results = stream::iter(futures)
            .buffer_unordered(self.max_parallel)
            .collect::<Vec<_>>()
            .await;

        // Merge and rank results
        self.merge_results(results, options.limit)
    }

    async fn search_workspace(&self, ws: &WorkspaceEntry, query: String) -> Result<Vec<SearchResult>> {
        let db_path = ws.get_db_path();

        // Run in blocking task (SQLite is synchronous)
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            let results = execute_search(&conn, &query)?;
            Ok(results.into_iter().map(|r| {
                SearchResult {
                    workspace_id: ws.id.clone(),
                    workspace_name: ws.name.clone(),
                    ..r
                }
            }).collect())
        }).await?
    }
}
```

#### 2.5 Unified Standup

Generate standup across all workspaces:

```rust
julie standup --all-workspaces --days 7
```

Output:
```markdown
## Developer Standup - Last 7 Days

### Julie (Code Intelligence)
- Added memory system architecture
- Implemented checkpoint/recall tools
- Fixed HNSW index rebuild performance

### Tusk (Memory System)
- Migrated from SQLite to JSONL storage
- Added git context capture
- Improved session detection

### Goldfish (Previous Iteration)
- Archived in favor of Julie integration
```

### Deliverables
- [ ] Global workspace registry implementation
- [ ] Auto-registration on startup
- [ ] Cross-workspace query orchestration
- [ ] --all-workspaces flag for tools
- [ ] Result merging and ranking
- [ ] Performance optimizations for parallel queries

### Success Metrics (If Implemented)
- Registry update: <10ms
- Cross-workspace search: <500ms (parallel execution)
- Memory overhead: <1MB for registry
- Support 100+ registered workspaces

</details>

**May revisit if strong user demand emerges. For now, reference workspaces provide sufficient multi-project capability.**

---

## Phase 4: Skills System

### Status: ✅ **COMPLETE (Modified Approach)**

**What Changed:**
- ❌ **Not deprecating Sherpa** - It solves a different problem (systematic workflows vs code intelligence)
- ✅ **Skills implemented in both Julie AND Sherpa** - Complementary, not replacement
- ✅ **Behavioral adoption approach** - Drive agent usage patterns organically through tool descriptions and examples

### Actual Implementation (2025-11-11)

**Architecture Decision:**
- **Julie**: Code+memory intelligence backend (what to search, what to remember)
- **Sherpa**: Workflow orchestration (how to develop systematically, TDD, debugging patterns)
- **Skills**: Bridge between the two (combine intelligence with process)

**Skills Implemented:**
1. **Julie Skills** - Leverage code+memory intelligence
   - Example: "explore-codebase" skill uses fast_search, get_symbols, trace_call_path
   - Example: "safe-refactor" skill uses fast_refs, rename_symbol with validation

2. **Sherpa Skills** - Workflow automation
   - Example: "rust-tdd-implementer" follows TDD methodology systematically
   - Example: "sqlite-rust-expert" provides specialized database guidance

### Original Goals (For Context)
- ~~Deprecate Sherpa as separate command orchestration tool~~ ❌ Decided against this
- ~~Leverage Claude Code's native skills system~~ ✅ Done, but augmented with Julie/Sherpa skills
- Create intelligent workflows that combine code + memory ✅ **ACHIEVED**
- Enable complex multi-step operations ✅ **ACHIEVED**

### Behavioral Adoption > Hooks

**Key Lesson Learned:**
Hooks can add complexity and synchronization challenges. Instead, we drive agent behavior through:

1. **Tool Descriptions** - Clear, detailed descriptions guide agent usage
   - Example: Julie's fast_search description explains when to use text vs semantic
   - Example: Sherpa's guide tool description explains the systematic workflow

2. **Skills as Templates** - Reusable workflow patterns
   - Skills show agents *how* to combine tools effectively
   - No hooks needed - skills are invoked explicitly when needed

3. **Examples in Documentation** - Show correct usage patterns
   - Agent instructions include workflow examples
   - "Before X, always do Y" patterns in tool descriptions

**Why This Works Better:**
- ✅ No synchronization issues between hooks
- ✅ Agents can reason about when to use tools
- ✅ Easier to maintain and evolve
- ✅ More transparent to users

**Custom Slash Commands Implemented:**
- `/checkpoint` - Save development memory
- `/recall` - Query past memories and decisions

These provide convenient shortcuts without hook complexity.

### Original Implementation Plan (Archived - For Reference)

<details>
<summary>Click to expand original skills implementation plan</summary>

#### 4.1 Skill Architecture

Skills are markdown files in `.claude/skills/` that define complex workflows:

```markdown
# skill: tdd
# description: Test-driven development workflow with memory integration

## Workflow
1. Run tests to see failures
2. Search codebase for similar test patterns
3. Recall previous solutions to similar test failures
4. Implement minimal solution
5. Checkpoint successful implementation
6. Refactor if needed
```

#### 3.2 Core Skills Library

**tdd.md** - Test-driven development
```markdown
1. julie fast_search "test" --type test
2. Run test suite, capture failures
3. julie recall --semantic "{error_message}"
4. julie trace_call_path {failing_function}
5. Implement fix
6. julie checkpoint "Fixed {test_name}: {solution}"
```

**debug.md** - Intelligent debugging
```markdown
1. julie fast_search "{error_pattern}"
2. julie recall --semantic "{error_message}" --all-workspaces
3. julie trace_call_path {stack_trace_function}
4. julie get_symbols {suspicious_file}
5. Identify root cause
6. julie checkpoint "Bug: {cause}, Solution: {fix}"
```

**refactor.md** - Safe refactoring
```markdown
1. julie checkpoint "Before refactoring {component}"
2. julie fast_refs {symbol_to_refactor}
3. julie rename_symbol {old_name} {new_name}
4. Run tests
5. julie checkpoint "Refactored {component}: {changes}"
```

**architecture.md** - Architectural decisions
```markdown
1. julie fast_search "similar patterns" --all-workspaces
2. julie recall --type decision --tags architecture
3. Document decision
4. julie checkpoint --type decision "Chose {option} because {reasons}"
```

#### 3.3 Hook Integration

Hooks can trigger skills automatically:

```typescript
// .claude/hooks/pre-commit.ts
export default {
  async execute() {
    // Trigger checkpoint before commit
    await julie.checkpoint(`Pre-commit: ${getStagedFiles()}`);

    // Run tests via TDD skill
    await executeSkill('tdd');
  }
}
```

#### 3.4 Skill Context

Skills have access to full Julie context:

```typescript
interface SkillContext {
  // Code intelligence
  searchCode: (query: string) => Promise<CodeResults>;
  findReferences: (symbol: string) => Promise<References>;
  traceCallPath: (symbol: string) => Promise<CallPath>;

  // Memory intelligence
  checkpoint: (description: string) => Promise<void>;
  recall: (query: RecallQuery) => Promise<Memories>;

  // Cross-workspace
  searchAllWorkspaces: (query: string) => Promise<UnifiedResults>;

  // Git context
  getCurrentBranch: () => Promise<string>;
  getDiff: () => Promise<string>;
}
```

#### 3.5 Migration from Sherpa

Map Sherpa commands to skills:

| Sherpa Command | Julie Skill | Enhanced With |
|---------------|-------------|---------------|
| sherpa test | tdd.md | Memory of previous test fixes |
| sherpa debug | debug.md | Cross-workspace error patterns |
| sherpa refactor | refactor.md | Impact analysis via fast_refs |
| sherpa review | review.md | Historical code decisions |

### Deliverables
- [ ] Skills template system
- [ ] Core skills library (10-15 skills)
- [ ] Skill execution engine
- [ ] Hook integration for skills
- [ ] Migration guide from Sherpa
- [ ] Skill development documentation

### Success Metrics (If Implemented Per Original Plan)
- Skill execution: <100ms overhead
- Complex workflows: <1s total execution
- 90% of Sherpa use cases covered
- Zero additional dependencies

</details>

**Actual Result:**
- Skills implemented in Julie AND Sherpa (complementary architecture)
- Behavioral adoption approach working well
- No hooks needed - tool descriptions + skills + examples drive usage

---

## Benefits (Achieved with Modified Approach)

### Simplification ✅ **ACHIEVED**
- **Before**: Julie + Tusk + Goldfish + Sherpa = 4 separate tools
- **After**: Julie (code+memory) + Sherpa (workflows) = 2 focused tools
  - ✅ Goldfish replaced by Julie's memory system
  - ✅ Tusk capabilities now in Julie (checkpoint/recall/plan)
  - ✅ Sherpa remains for workflow orchestration (different concern)
  - ✅ Skills bridge both tools effectively

### Intelligence Amplification ✅ **ACHIEVED**
- ✅ Code search that understands history (checkpoint/recall integrated)
- ✅ Debugging that learns from past fixes (semantic search on memories)
- ✅ Refactoring that preserves decision context (memory checkpoints)
- ✅ Architecture that builds on previous patterns (decision memories)

### Team Collaboration ✅ **ACHIEVED**
- ✅ Git-tracked memories = shared knowledge (.memories/ directory)
- ✅ Architectural decisions in code repository (checkpoint tool)
- ✅ Onboarding via historical context (recall tool searches history)
- ✅ Collective learning from bugs/fixes (memory system captures learnings)

### Developer Experience ✅ **ACHIEVED**
- ✅ Focused tools: Julie for intelligence, Sherpa for workflow
- ✅ Consistent patterns: Skills bridge both tools
- ✅ Progressive disclosure: Behavioral adoption > hook complexity
- ✅ Natural language queries work across code + memory

---

## Migration Path (Status: Partially Complete)

### From Tusk ⏸️ **Tool Available, Migration Optional**
**Status:** Julie's checkpoint/recall tools work today. Tusk users can switch anytime.

```bash
# Manual migration (if desired)
# 1. Export Tusk memories as JSON
# 2. Place in .memories/ directory with proper format

# New workflow
checkpoint → julie checkpoint (or /checkpoint slash command)
recall → julie recall (or /recall slash command)
```

### From Goldfish ✅ **COMPLETE**
**Status:** Goldfish deprecated. Julie's memory system replaced it.

```
# Migration already done for active users
.goldfish/ → .memories/ (git-tracked, project-level)
```

### From Sherpa ❌ **NOT MIGRATING**
**Status:** Sherpa remains as separate tool for workflow orchestration.

**Rationale:**
- Sherpa solves different problem (systematic process vs code intelligence)
- Skills bridge Julie and Sherpa effectively
- Both tools benefit from being focused on their core concerns

---

## Implementation Timeline (Actual vs Planned)

### Phase 1: Memory System ✅ **COMPLETE (2025-11-10)**
- ✅ Storage, tools, SQL views
- ✅ Tree-sitter integration (no changes needed)
- ✅ Testing, documentation (26 tests passing)
- ✅ Windows path bug fix (critical)

### Phase 1.5: Mutable Plans ✅ **COMPLETE (2025-11-10)**
- ✅ plan() tool with 6 actions
- ✅ Atomic file updates
- ✅ SQL views and searchability
- ✅ 30 tests passing

### Phase 2: Memory Embeddings ✅ **COMPLETE (2025-11-11)**
- ✅ Custom RAG pipeline for .memories/
- ✅ 88.7% embedding reduction
- ✅ Critical bug fixes (ranking + escaped quotes)
- ✅ 7 comprehensive tests

### Phase 3: Cross-Workspace ⏸️ **DEFERRED**
- Not implemented - reference workspaces sufficient
- May revisit based on user demand

### Phase 4: Skills ✅ **COMPLETE (Modified)**
- ✅ Skills in Julie AND Sherpa
- ✅ Behavioral adoption approach
- ✅ Custom slash commands (/checkpoint, /recall)
- ❌ No hooks (deliberate decision)

---

## Success Criteria (Status: Largely Achieved)

### Quantitative ✅ **ACHIEVED**
- ✅ Memory operations: <50ms latency (checkpoint/recall)
- ⏸️ Cross-workspace search: N/A (deferred)
- ✅ Skill execution: Works well with both Julie and Sherpa
- ✅ Storage: Extremely efficient with 88.7% embedding reduction
- ✅ Development tasks use Julie for code+memory intelligence

### Qualitative ✅ **ACHIEVED**
- ✅ Developers using Julie's memory system (Goldfish deprecated)
- ✅ Team knowledge captured naturally (.memories/ git-tracked)
- ✅ Reduced context loss (checkpoint/recall/plan tools)
- ✅ Faster onboarding via historical context (semantic search on memories)
- ✅ Improved debugging via pattern recognition (memory search)

---

## Future Enhancements (Post-Launch)

### Intelligence Features
- Auto-suggest memories based on activity
- Pattern detection across workspaces
- Predictive search based on history
- Team knowledge graphs

### Integration Expansions
- VS Code extension with inline memories
- Web UI for browsing team knowledge
- CI/CD integration for decision tracking
- Metrics dashboard for code + memory

### Advanced Capabilities
- Natural language programming via memories
- Auto-generate documentation from decisions
- Cross-team knowledge sharing
- AI-assisted architecture reviews

---

## Conclusion (2025-11-11 Update)

Julie has successfully evolved into a comprehensive code+memory intelligence system through pragmatic, incremental development. Rather than consolidating everything into one tool, we achieved the core vision through strategic architecture decisions:

**What We Achieved:**
- ✅ **Memory System Complete**: Replaced Goldfish with superior git-tracked, project-level memories
- ✅ **Optimized Performance**: 88.7% embedding reduction, production-ready JSON parsing
- ✅ **Focused Architecture**: Julie handles intelligence (code+memory), Sherpa handles workflow
- ✅ **Skills as Bridges**: Complementary tools working together, not forced consolidation
- ✅ **Behavioral Adoption**: Tool descriptions + skills + examples > hook complexity

**What Changed from Original Plan:**
- ⏸️ **Cross-workspace deferred**: Reference workspaces already provide this capability
- ❌ **Sherpa not replaced**: Solves different problem (systematic process vs intelligence)
- ✅ **Better outcome**: Two focused tools > one monolithic system

**The Result:**
Julie understands not just *what* code does, but *why* it exists and *how* you've worked with it over time. Combined with Sherpa's systematic workflow guidance and bridging skills, developers have a powerful, maintainable toolset for intelligent development.

**This isn't just an upgrade** - it's a validation that:
- Focus beats feature bloat
- Complementary tools can be better than monolithic systems
- Behavioral adoption beats complex hook architectures
- Pragmatic decisions deliver better outcomes than rigid plans

The vision was realized, just not exactly as originally planned. And that's okay - we shipped something better.

---

## Bonus Phase: Auto-Generated .julieignore (v1.7.3+)

### Status: 🚀 **IN PROGRESS** (2025-11-12)

**Problem Identified:**
- Legacy apps mix vendor and custom code in same directories (`Scripts/` has jquery AND PatientCase)
- Current 100KB limit catches big vendor files, but smaller libraries (67-84KB) still get indexed
- Result: 15K+ vendor symbols pollute search results with bootstrap, jquery, angular noise

**Solution:** Auto-generate `.julieignore` during first workspace scan to exclude vendor code automatically.

### Goals
- Detect vendor patterns during initial file discovery (libs/, plugin/, *.min.js)
- Auto-generate `.julieignore` with detected patterns before indexing
- Make the file self-documenting (explain what/why/how to modify)
- Integrate diagnostics into health check (show what's excluded)
- Enable agent-assisted debugging when search doesn't find files

### Implementation

#### 1. Enhanced Discovery Phase

Modify `discover_indexable_files()` to analyze patterns before indexing:

```rust
// src/tools/workspace/discovery.rs

pub(crate) fn discover_indexable_files(&self, workspace_path: &Path) -> Result<Vec<PathBuf>> {
    let julieignore_path = workspace_path.join(".julieignore");

    // If .julieignore doesn't exist, auto-generate it
    let custom_ignores = if julieignore_path.exists() {
        self.load_julieignore(workspace_path)?
    } else {
        info!("🤖 No .julieignore found - scanning for vendor patterns...");

        // Step 1: Collect ALL files first
        let mut all_files = Vec::new();
        self.walk_directory_recursive(
            workspace_path,
            &blacklisted_dirs,
            &blacklisted_exts,
            max_file_size,
            &[], // No filters yet
            &mut all_files,
        )?;

        // Step 2: Analyze for vendor patterns
        let detected_patterns = self.analyze_vendor_patterns(&all_files, workspace_path)?;

        // Step 3: Generate .julieignore file
        if !detected_patterns.is_empty() {
            self.generate_julieignore_file(workspace_path, &detected_patterns)?;
            info!("✅ Generated .julieignore with {} patterns", detected_patterns.len());
            detected_patterns
        } else {
            info!("✨ No vendor patterns detected - project looks clean!");
            Vec::new()
        }
    };

    // Continue with normal discovery...
}
```

#### 2. Vendor Pattern Detection

```rust
fn analyze_vendor_patterns(&self, files: &[PathBuf], workspace_root: &Path) -> Result<Vec<String>> {
    let mut patterns = Vec::new();
    let mut dir_stats: HashMap<PathBuf, DirectoryStats> = HashMap::new();

    // Collect statistics for each directory
    for file in files {
        if let Some(parent) = file.parent() {
            let stats = dir_stats.entry(parent.to_path_buf()).or_default();
            stats.file_count += 1;

            // Check for vendor indicators
            if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                if name.contains(".min.") { stats.minified_count += 1; }
                if name.starts_with("jquery") { stats.jquery_count += 1; }
                if name.starts_with("bootstrap") { stats.bootstrap_count += 1; }
            }
        }
    }

    // Detect vendor directories with high confidence
    for (dir, stats) in dir_stats {
        let dir_name = dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // High confidence: Directory name indicates vendor code
        if matches!(dir_name, "libs" | "lib" | "plugin" | "plugins" | "vendor" | "third-party") {
            if stats.file_count > 5 {
                let pattern = self.dir_to_pattern(&dir, workspace_root);
                info!("📦 Detected vendor directory: {} ({} files)", pattern, stats.file_count);
                patterns.push(pattern);
            }
        }
        // Medium confidence: Lots of vendor-named files
        else if stats.jquery_count > 3 || stats.bootstrap_count > 2 {
            let pattern = self.dir_to_pattern(&dir, workspace_root);
            info!("📦 Detected library directory: {} (jquery/bootstrap files)", pattern);
            patterns.push(pattern);
        }
        // Medium confidence: High concentration of minified files
        else if stats.minified_count > 10 && stats.minified_count > stats.file_count / 2 {
            let pattern = self.dir_to_pattern(&dir, workspace_root);
            info!("📦 Detected minified code directory: {} ({} minified)", pattern, stats.minified_count);
            patterns.push(pattern);
        }
    }

    Ok(patterns)
}

#[derive(Default)]
struct DirectoryStats {
    file_count: usize,
    minified_count: usize,
    jquery_count: usize,
    bootstrap_count: usize,
}
```

#### 3. Self-Documenting .julieignore

```rust
fn generate_julieignore_file(&self, workspace_path: &Path, patterns: &[String]) -> Result<()> {
    let content = format!(
r#"# .julieignore - Julie Code Intelligence Exclusion Patterns
# Auto-generated by Julie on {}
#
# ═══════════════════════════════════════════════════════════════
# What Julie Did Automatically
# ═══════════════════════════════════════════════════════════════
# Julie analyzed your project and detected vendor/third-party code patterns.
# These patterns exclude files from:
# • Symbol extraction (function/class definitions)
# • Semantic search embeddings (AI-powered search)
#
# Files are still searchable as TEXT using fast_search(mode="text"),
# but won't clutter symbol navigation or semantic search results.
#
# ═══════════════════════════════════════════════════════════════
# Why Exclude Vendor Code?
# ═══════════════════════════════════════════════════════════════
# 1. Search Quality: Prevents vendor code from polluting search results
# 2. Performance: Skips symbol extraction for thousands of vendor functions
# 3. Relevance: Semantic search focuses on YOUR code, not libraries
#
# ═══════════════════════════════════════════════════════════════
# How to Modify This File
# ═══════════════════════════════════════════════════════════════
# • Add patterns: Just add new lines with glob patterns (gitignore syntax)
# • Remove patterns: Delete lines or comment out with #
# • Check impact: Use manage_workspace(operation="health")
#
# FALSE POSITIVE? If Julie excluded something important:
# 1. Delete or comment out the pattern below
# 2. Julie will automatically reindex on next file change
#
# DISABLE AUTO-GENERATION: Create this file manually before first run
#
# ═══════════════════════════════════════════════════════════════
# Auto-Detected Vendor Directories
# ═══════════════════════════════════════════════════════════════
{}

# ═══════════════════════════════════════════════════════════════
# Common Patterns (Uncomment if needed in your project)
# ═══════════════════════════════════════════════════════════════
# *.min.js
# *.min.css
# jquery*.js
# bootstrap*.js
# angular*.js

# ═══════════════════════════════════════════════════════════════
# Debugging: If Search Isn't Finding Files
# ═══════════════════════════════════════════════════════════════
# Use manage_workspace(operation="health") to see:
# • How many files are excluded by each pattern
# • Whether patterns are too broad
#
# If a pattern excludes files it shouldn't, comment it out or make
# it more specific (e.g., "**/vendor/lib/**" vs "**/lib/**")
"#,
        chrono::Local::now().format("%Y-%m-%d"),
        patterns.iter()
            .map(|p| format!("{}/**", p))
            .collect::<Vec<_>>()
            .join("\n")
    );

    std::fs::write(workspace_path.join(".julieignore"), content)?;
    Ok(())
}
```

#### 4. Enhanced Health Check

```rust
// src/tools/workspace/commands/health.rs

pub async fn health_check(&self, detailed: bool) -> Result<HealthReport> {
    // ... existing checks ...

    // NEW: .julieignore analysis
    if let Some(ignore_stats) = self.analyze_julieignore(workspace_path)? {
        info!("📋 .julieignore patterns:");
        info!("   Total patterns: {}", ignore_stats.pattern_count);
        info!("   Files excluded: {}", ignore_stats.excluded_count);

        if detailed {
            for (pattern, count) in &ignore_stats.pattern_breakdown {
                info!("   - {}: {} files", pattern, count);
            }
        }

        // Warning if too broad
        if ignore_stats.excluded_count > ignore_stats.total_files / 2 {
            warn!("⚠️  More than 50% of files are excluded");
            warn!("   Review .julieignore - patterns may be too broad");
        }
    }

    Ok(report)
}
```

#### 5. Server Instructions Update

Add to `JULIE_AGENT_INSTRUCTIONS.md`:

```markdown
## 🔍 Auto-Generated .julieignore

Julie automatically generates `.julieignore` on first run to exclude vendor code.

### Debugging "File Not Found" Issues

If search isn't finding expected files:

1. **Check exclusion stats**: `manage_workspace(operation="health")`
2. **Review .julieignore**: Read the file to see patterns
3. **Test with text search**: Try `fast_search(mode="text")`

### Example Debugging Workflow

User: "Why can't I find MyCustomScript.js?"

Agent:
1. Calls manage_workspace(operation="health")
   → Sees "Scripts/plugin/** excludes 89 files"
2. Reads .julieignore
   → Sees pattern excluding plugins directory
3. Explains: "MyCustomScript.js is excluded. Edit .julieignore to adjust."
```

### Benefits

1. **Zero User Friction** - Happens automatically during first scan
2. **Transparent** - Logs explain what was detected and why
3. **Self-Documenting** - File explains itself with comprehensive comments
4. **Debuggable** - Health check shows exclusion stats
5. **Correctable** - Easy to edit if false positives occur
6. **Team-Shareable** - Commit to git like .gitignore

### Example Output

```
🔍 Scanning workspace: /Users/murphy/source/SurgeryScheduling
🤖 No .julieignore found - scanning for vendor patterns...
📦 Detected vendor directory: Scheduling/Scripts/libs (15 files)
📦 Detected vendor directory: Scheduling/Scripts/plugin (89 files)
✅ Generated .julieignore with 2 patterns
📝 Review and commit .julieignore to version control
📊 Discovered 408 files total
⏭️  Excluded 104 files, indexing 304 files
```

### Deliverables

- [ ] Implement pattern detection in `discover_indexable_files()`
- [ ] Add `analyze_vendor_patterns()` with heuristics
- [ ] Generate comprehensive .julieignore template
- [ ] Enhance health check with exclusion diagnostics
- [ ] Update server instructions for debugging workflow
- [ ] Add tests for pattern detection logic
- [ ] Documentation and examples

### Success Metrics

- Pattern detection: <10ms overhead on file scan
- False positive rate: <5% (conservative heuristics)
- Self-serve debugging: Agents can diagnose via health check
- Reduced vendor noise: 80-90% of vendor symbols excluded
- Zero breaking changes: Existing projects unaffected

**Status:** Ready for implementation (design approved 2025-11-12)