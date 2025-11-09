# Julie 2.0: Unified Project Intelligence System

## Executive Summary

Julie evolves from a code intelligence tool into a comprehensive project intelligence system that unifies code search, project memory, and workflow orchestration. This transformation eliminates the need for separate tools (Tusk, Goldfish, Sherpa) by integrating their capabilities directly into Julie.

**Vision**: One MCP server that understands not just *what* your code does, but *why* it exists and *how* you work with it.

**Result**: Replace three separate tools with one unified system that provides:
- Code intelligence (existing Julie capabilities)
- Project memory (checkpoint/recall from Tusk/Goldfish)
- Workflow orchestration (skills replacing Sherpa)

## Architecture Overview

### Core Principles
1. **Simplicity First**: Leverage existing Julie infrastructure, minimal changes
2. **Project-Level Storage**: Memories live with code, git-trackable
3. **Progressive Enhancement**: Each phase builds on the previous
4. **No Breaking Changes**: Existing Julie functionality remains intact

### Storage Architecture

```
# Project Level (with code)
<project>/.julie/
├── memories/                    # NEW: Project memories (JSONL)
│   ├── 2025-01-09-143022.jsonl
│   └── 2025-01-10-093012.jsonl
├── indexes/
│   └── {workspace_id}/
│       ├── db/symbols.db        # Existing + memory views
│       └── vectors/             # HNSW index (code + memories)
└── workspace_registry.json      # Existing

# User Level (cross-project)
~/.julie/
└── workspace_registry.json      # NEW: All registered workspaces
```

### Data Flow

```
Memory Creation:
User → checkpoint tool → JSONL file → File watcher → Tree-sitter → symbols.db → Embeddings → HNSW

Memory Recall:
User → recall tool → SQL view → Chronological results
User → fast_search → FTS5/HNSW → Unified results (code + memories)

Cross-Workspace:
User → search --all-workspaces → Registry → Parallel queries → Merged results
```

---

## Phase 1: Project-Level Memory System

### Goals
- Add memory capabilities to Julie without disrupting existing functionality
- Store memories as JSONL files that are automatically indexed
- Enable both chronological recall and semantic search of memories
- Keep memories git-trackable for team knowledge sharing

### Implementation

#### 1.1 Memory Storage Format

Memories stored in `.julie/memories/` as JSONL files:

```json
{
  "id": "mem_1736422822_abc123",
  "timestamp": 1736422822,
  "type": "checkpoint",
  "description": "Fixed race condition in auth flow by adding mutex",
  "tags": ["bug", "auth", "concurrency"],
  "session_id": "session_xyz",
  "git": {
    "branch": "fix/auth-race",
    "commit": "abc123def",
    "dirty": false,
    "files_changed": ["src/auth.rs", "src/lib.rs"]
  },
  "metadata": {
    "confidence": 85,
    "related_symbols": ["AuthHandler::login", "SessionManager::validate"],
    "error_pattern": "concurrent session validation"
  }
}
```

#### 1.2 New Tools

**checkpoint** - Save a development checkpoint
```rust
julie checkpoint "description" [--tags tag1,tag2] [--type checkpoint|decision|learning]
```

**recall** - Retrieve memories chronologically or by search
```rust
julie recall [--last-hour] [--since date] [--tags tags] [--semantic query]
```

**plan** - Manage active development plans
```rust
julie plan create "implement auth"
julie plan update --status in_progress
julie plan complete
```

#### 1.3 Database Enhancements

Add SQL view for memories in symbols.db:

```sql
CREATE VIEW memories AS
SELECT
  f.path,
  f.content_hash,
  json_extract(f.content, '$.id') as id,
  json_extract(f.content, '$.timestamp') as timestamp,
  json_extract(f.content, '$.description') as description,
  json_extract(f.content, '$.type') as type,
  json_extract(f.content, '$.tags') as tags,
  json_extract(f.content, '$.git.branch') as git_branch,
  json_extract(f.content, '$.git.commit') as git_commit
FROM files f
WHERE f.path LIKE '.julie/memories/%'
  AND f.path LIKE '%.jsonl';

-- Index for chronological queries
CREATE INDEX idx_memories_timestamp ON memories(timestamp);
```

#### 1.4 Tree-Sitter Integration

Update JSON parser configuration to:
- Parse JSONL files (each line as separate JSON)
- Extract memory structure into searchable fields
- Generate embeddings for description and metadata

#### 1.5 Behavioral Adoption

Julie will proactively suggest memory creation:
- After completing bug fixes
- When making architectural decisions
- During complex debugging sessions
- At natural transition points

### Deliverables
- [ ] Memory tool implementations (checkpoint, recall, plan)
- [ ] JSONL file writer with atomic operations
- [ ] SQL views and indexes for memory queries
- [ ] Tree-sitter JSONL parsing updates
- [ ] Integration tests for memory operations
- [ ] Documentation and examples

### Success Metrics
- Checkpoint save: <50ms
- Chronological recall: <5ms
- Semantic recall: <100ms (existing search performance)
- Zero impact on existing tool performance

---

## Phase 2: Cross-Workspace Intelligence

### Goals
- Enable searching across all projects from any workspace
- Create unified view of developer's knowledge
- Support cross-project patterns and learnings
- Maintain workspace isolation when needed

### Implementation

#### 2.1 Workspace Registry

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

### Success Metrics
- Registry update: <10ms
- Cross-workspace search: <500ms (parallel execution)
- Memory overhead: <1MB for registry
- Support 100+ registered workspaces

---

## Phase 3: Replace Sherpa with Skills

### Goals
- Deprecate Sherpa as separate command orchestration tool
- Leverage Claude Code's native skills system
- Create intelligent workflows that combine code + memory
- Enable complex multi-step operations

### Implementation

#### 3.1 Skill Architecture

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

### Success Metrics
- Skill execution: <100ms overhead
- Complex workflows: <1s total execution
- 90% of Sherpa use cases covered
- Zero additional dependencies

---

## Benefits

### Simplification
- **Before**: Julie + Tusk + Goldfish + Sherpa = 4 MCP servers
- **After**: Julie 2.0 = 1 MCP server with everything

### Intelligence Amplification
- Code search that understands history
- Debugging that learns from past fixes
- Refactoring that preserves decision context
- Architecture that builds on previous patterns

### Team Collaboration
- Git-tracked memories = shared knowledge
- Architectural decisions in code repository
- Onboarding via historical context
- Collective learning from bugs/fixes

### Developer Experience
- One tool to learn and master
- Consistent commands and patterns
- Progressive disclosure of complexity
- Natural language queries across everything

---

## Migration Path

### From Tusk
```bash
# One-time migration
julie migrate --from-tusk ~/.tusk/journal.db

# Behavioral adoption
checkpoint → julie checkpoint
recall → julie recall
standup → julie recall --format standup
```

### From Goldfish
```bash
# Import memories
julie import --from-goldfish ~/.goldfish/*/checkpoints/

# Update .gitignore
.goldfish/ → .julie/memories/
```

### From Sherpa
```bash
# Convert commands to skills
sherpa.yaml → .claude/skills/*.md

# Update workflows
sherpa command → execute skill
```

---

## Implementation Timeline

### Phase 1: Memory System (Weeks 1-3)
- Week 1: Storage, tools, SQL views
- Week 2: Tree-sitter, embeddings, search integration
- Week 3: Testing, documentation, behavioral adoption

### Phase 2: Cross-Workspace (Weeks 4-5)
- Week 4: Registry, auto-registration, query orchestration
- Week 5: Tool flags, result merging, performance optimization

### Phase 3: Skills (Weeks 6-7)
- Week 6: Skill system, core library, execution engine
- Week 7: Hook integration, migration tools, documentation

### Total: 7 weeks to Julie 2.0

---

## Success Criteria

### Quantitative
- Memory operations: <100ms latency
- Cross-workspace search: <500ms for 10 workspaces
- Skill execution: <1s for complex workflows
- Storage: <100MB per 10K memories
- 90% of development tasks use only Julie

### Qualitative
- Developers prefer Julie over separate tools
- Team knowledge captured naturally
- Reduced context loss between sessions
- Faster onboarding via historical context
- Improved debugging via pattern recognition

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

## Conclusion

Julie 2.0 transforms a code intelligence tool into a comprehensive project intelligence system. By integrating memory capabilities and workflow orchestration directly into Julie, we create a unified platform that understands not just code structure, but the entire context of software development - the what, why, and how of building software.

The phased approach ensures we can deliver value incrementally while maintaining stability. Each phase builds on the previous, creating a powerful system that remains simple to use and understand.

This is not just an upgrade to Julie - it's the realization of the vision for truly intelligent development assistance.