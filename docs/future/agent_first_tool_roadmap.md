# Agent-First Tool Roadmap
**Last Updated:** 2025-10-01
**Status:** Planning Phase
**Source:** Transcript discussions from tool redesign sessions

## Executive Summary

This document captures our complete roadmap for building agent-first tools that solve real AI agent pain points. These tools leverage Julie's core technologies (tree-sitter, Tantivy, HNSW embeddings, DMP) without creating tool explosion.

**Core Principle:** "Agents will use tools that make them look competent" - every retry, syntax error, or zero-result search makes agents seem less capable.

---

## 🎯 Core Agent Pain Points

| Agent Struggle | Current Reality | Julie's Solution |
|----------------|-----------------|------------------|
| **Syntax errors after edits** | 3+ retry attempts to fix extra `}` | AST-based auto-fix (instant) |
| **Reading 500 lines for 5 important ones** | 3000 tokens wasted | Smart read (150 tokens, 95% savings) |
| **"Where's the important code?"** | Manual exploration, guessing | Onboarding mode (instant answer) |
| **String matching failures** | Ambiguous Edit tool retries | Line-based precision (zero ambiguity) |
| **Context window exhaustion** | Forced to restart frequently | Token-optimized tools (5x longer sessions) |
| **Zero search results** | 5 searches, 0 results each time | Improved Tantivy utilization |
| **Manual documentation** | Hours writing CLAUDE.md | Auto-generated agent docs |

---

## 📋 Tool Specifications

### 1. AST-Based Reformat/Fix Tool
**Purpose:** Eliminate agent retry loops from syntax errors

**The Problem:**
Agents frequently make syntax errors when editing code:
- Add one too many curly braces `}`
- Miss semicolons
- Break indentation
- Create unmatched brackets

Current reality: Agent edits → syntax error → 3 retry attempts → finally works

**The Solution:**
```rust
ast_fix file="user.rs" mode="auto"
// Returns: "Fixed: Removed unmatched '}' at line 47"

ast_fix file="user.ts" mode="diagnose"
// Returns: "Error at line 23: Missing semicolon after 'return data'"

ast_fix file="main.py" mode="reformat"
// Returns: "Reformatted: Fixed indentation (4 spaces), added missing colons"

ast_fix file="app.js" mode="validate"
// Returns: "Valid" or specific error locations
```

**Modes:**
- `auto` - Detect and fix common issues automatically
- `diagnose` - Report issues with specific fix suggestions
- `reformat` - Fix formatting/style issues (indentation, trailing commas)
- `validate` - Just check if syntax is valid (boolean + details)

**Why Agents Will Use It:**
- **Deterministic**: AST parsing gives exact error locations
- **Fast recovery**: No more guess-and-check loops
- **Confidence**: Know the fix worked before moving on
- **Learning**: See what was wrong and how it was fixed

**Implementation Approach:**
- Leverage existing tree-sitter parsers for all 26 languages
- Could extend FuzzyReplaceTool with `ast_mode` parameter
- Or create new SmartRefactorTool operation: `reformat`, `validate_syntax`
- Use tree-sitter error nodes to detect issues
- Apply language-specific auto-fix rules

**Success Metrics:**
- 95% of syntax errors auto-fixed
- <5% retry rate (vs current 30%)
- Agents choose this over manual debugging 90%+ of time

---

### 2. Smart Read Tool
**Purpose:** Surgical file reading with 70-90% context savings

**The Problem:**
Current agent workflow:
```
Agent: Read src/services/user.rs
System: [Returns 847 lines, uses 3000 tokens]
Agent: [Only needed lines 234-245, wasted 2850 tokens]
```

**The Solution:**
```rust
smart_read file="user.rs" target="getUserById" mode="minimal"
// Returns: Just the function + types it uses (150 tokens vs 3000)

smart_read file="user.rs" mode="business_logic"
// Returns: Core logic only, no boilerplate (500 tokens vs 3000)

smart_read file="user.rs" line=234 context=10 mode="ast_aware"
// Returns: Complete AST node around line 234 (clean boundaries)

smart_read file="user.rs" symbols=["getUserById", "User"] mode="dependencies"
// Returns: Functions + all their type dependencies
```

**Modes:**
- `minimal` - Bare minimum code to understand the target
- `business_logic` - Skip framework/boilerplate, show domain logic
- `dependencies` - Include all used types/imports for completeness
- `ast_aware` - Respect AST boundaries (full functions/classes, not partial)
- `context_lines` - Traditional line-based context (fallback)

**Why This Matters:**
- **Token savings**: 70-90% reduction in most cases
- **Faster understanding**: Agent sees only relevant code
- **Better context management**: Work 5x longer before compaction
- **Precision**: AST boundaries mean clean, complete code blocks

**Implementation Approach:**
- Extend existing `GetSymbolsTool` (currently shows structure only)
- Add `include_body` flag to show actual code
- Add `target` parameter to extract specific symbols
- Use tree-sitter to find symbol boundaries
- Use embeddings to find related code (semantic clustering)

**Tool Interoperability:**
- Results include structured data (file, line, symbol names)
- Can feed directly into SmartRefactorTool or FuzzyReplaceTool
- Supports multi-step workflows (search → read → edit)

---

### 3. Semantic Diff Tool
**Purpose:** Understand behavioral changes, not just text changes

**The Problem:**
Traditional diffs show:
```
- function getData() {
+ async function getData() {
```

What agents need to know:
```
BEHAVIORAL CHANGE: Made synchronous → asynchronous
IMPACT: All 47 callers need await
PUBLIC API: Changed - breaking change
ERROR HANDLING: New async error patterns needed
```

**The Solution:**
Combine DMP (text) + AST (structure) + Embeddings (semantics):

```rust
semantic_diff old="commit_a" new="commit_b" mode="behavioral"
// Returns: "3 behavioral changes:
//  1. Error handling added (try-catch wrapping)
//  2. Return type changed (User → Promise<User>)
//  3. Made async (synchronous → asynchronous)"

semantic_diff file="user.rs" mode="structural"
// Returns: "Moved auth check from line 45 to line 23 (earlier validation)
//          No behavioral change - logic preserved"

semantic_diff before=<code> after=<code> mode="impact"
// Returns: "This changes the public API
//          3 consumers will need updates:
//            - frontend/AuthService.ts:234
//            - api/routes/users.ts:89
//            - tests/integration/auth.test.ts:156"

semantic_diff file="main.py" mode="visual"
// Returns: Color-coded diff agents can parse
//  GREEN (semantic match): Code moved but logic same
//  YELLOW (refactor): Structure changed, behavior preserved
//  RED (breaking): Behavioral change, action needed
```

**Modes:**
- `behavioral` - What actually changed in terms of behavior
- `structural` - How code was reorganized (refactors)
- `impact` - Who will be affected by this change
- `visual` - Color-coded diff with semantic annotations
- `api_changes` - Focus on public API modifications

**Why This Matters:**
- Agents understand **impact before making changes**
- Detect **unintended behavioral changes** in refactors
- Validate that **refactors preserve semantics**
- **Confidence in code review** - know what really changed

**Implementation Approach:**
- FuzzyReplaceTool already has DMP - expose diff generation
- Add AST comparison using tree-sitter
- Use embeddings to detect semantic similarity
- Combine scores: text similarity + AST similarity + semantic similarity
- Present results in agent-friendly format

**Tool Interoperability:**
- Input: Can accept git commits, file paths, or code strings
- Output: Structured JSON with change classifications
- Integration: Works with trace_call_path to find impact radius

---

### 4. Enhanced fast_explore - Onboarding Mode
**Purpose:** Identify the "heart" of any project instantly

**The Problem:**
Agent's first question: "What's important in this codebase?"

Current answer: "Let me read 50 files and guess..."

**The Solution:**
```rust
fast_explore mode="onboarding" focus="core"
// Returns prioritized list:
// 1. Core business logic (95% importance): src/services/
// 2. Main API endpoints (85% importance): src/routes/
// 3. Data models (80% importance): src/models/
// 4. Config files (20% importance): config/
// 5. Skip entirely: tests/, vendor/, node_modules/

fast_explore mode="onboarding" focus="entry_points"
// Returns: "Main execution starts at:
//          - Server: main.rs:45 (HTTP server startup)
//          - CLI: cli.rs:23 (command processing)
//          - Background: workers/scheduler.rs:67 (cron jobs)
//          - API routes defined in: routes/ (REST endpoints)"

fast_explore mode="onboarding" focus="heart"
// Returns: "Project heart (top 5 critical files):
//          1. src/auth/AuthService.ts (100% - used by everything)
//          2. src/database/UserRepository.ts (95% - core data access)
//          3. src/api/routes.ts (90% - public interface)
//          4. src/models/User.ts (85% - central domain model)
//          5. src/services/PaymentService.ts (80% - critical business logic)
//
//          Noise to skip (80% of codebase):
//          - tests/ (450 files - test code)
//          - vendor/ (1200 files - dependencies)
//          - config/ (34 files - environment config)"
```

**How It Works:**
1. **AST Analysis** - Identify structural importance:
   - Main functions, exports, public APIs
   - Entry points (main, init, startup)
   - Core classes vs utility classes

2. **Embedding Clustering** - Group related business logic:
   - Find semantic clusters (all auth code, all payment code)
   - Identify core vs peripheral functionality
   - Detect framework code vs domain logic

3. **Path Analysis** - Filter noise:
   - Production code: `src/`, `lib/` (high weight)
   - Tests: `tests/`, `__tests__/`, `*.test.*` (skip)
   - Config: `config/`, `.env` (low weight)
   - Dependencies: `node_modules/`, `vendor/` (skip)

4. **Git History** - Understand criticality:
   - Frequently changed files = actively maintained
   - Rarely changed files = stable foundation or dead code
   - Recent changes = current focus areas

5. **Cross-Reference Analysis** - Find central code:
   - High fan-in (many imports) = core dependency
   - High fan-out (imports many) = orchestrator
   - Isolated (few connections) = peripheral

**Criticality Scoring Formula:**
```
criticality_score =
  (structural_importance * 0.3) +    // AST: is it main, export, public?
  (semantic_centrality * 0.25) +      // Embeddings: core concept?
  (cross_reference_count * 0.25) +    // How many depend on this?
  (git_change_frequency * 0.1) +      // How often modified?
  (path_weight * 0.1)                 // src/ vs test/ vs vendor/
```

**Why Agents Will Use It:**
- **Instant orientation** in unfamiliar codebases
- **No wasted time** reading boilerplate
- **Confidence** in understanding architecture
- **Actionable insights** on where to focus

**Implementation Approach:**
- Extend FastExploreTool with new mode
- Use existing embeddings infrastructure
- Leverage AST parsing for all languages
- Add git history analysis
- Path-based filtering rules

---

### 5. Auto-Generate Agent Documentation
**Purpose:** Perfect CLAUDE.md and AGENTS.md files, zero effort

**The Problem:**
Writing good agent documentation is:
- Time-consuming (hours of work)
- Easy to get wrong (too verbose or too sparse)
- Becomes outdated quickly
- Inconsistent across projects

**The Solution:**
```rust
generate_docs output="CLAUDE.md" style="agent_optimized"
// Analyzes project, generates perfect agent documentation:
// - Project overview (tech stack, architecture)
// - Critical files and their purpose
// - Common workflows (how to add features, fix bugs)
// - Testing approach
// - Deployment process
// - NO FLUFF - only what agents need

generate_docs output="AGENTS.md" style="onboarding" focus="new_agent"
// Creates onboarding doc specifically for new AI agents:
// - "Start here" file paths
// - Core concepts (domain models, business logic)
// - Naming conventions
// - Code patterns to follow
// - Integration points
```

**What Gets Included (Intelligently):**
1. **Tech Stack Detection** - Automatic discovery:
   - Languages (from extractors)
   - Frameworks (from package.json, Cargo.toml, requirements.txt)
   - Databases (from config files, connection strings)
   - Infrastructure (Docker, K8s detection)

2. **Architecture Summary** - From onboarding mode:
   - Entry points
   - Core business logic files (top 10)
   - Data flow (how data moves through system)
   - External dependencies (APIs, services)

3. **Critical Paths** - Automated discovery:
   - Authentication flow
   - Main user workflows
   - Data persistence patterns
   - Error handling approach

4. **Development Workflow** - From git + build config:
   - How to build (`cargo build`, `npm run build`)
   - How to test (`cargo test`, `npm test`)
   - How to run (`cargo run`, `npm start`)
   - Pre-commit checks (if any)

5. **Agent-Specific Guidance**:
   - "When adding features, start at X"
   - "When fixing bugs, check Y first"
   - "Never modify Z files directly"
   - "Always run W after changes"

**What Gets EXCLUDED (Intelligently):**
- Generic boilerplate ("This is a software project...")
- Obvious info ("Git is used for version control...")
- Framework docs (agents can look those up)
- Excessive detail (line-by-line code explanations)
- Marketing fluff

**Output Format:**
```markdown
# Project: Julie Code Intelligence

## Tech Stack
- Language: Rust (native performance)
- Search: Tantivy (sub-10ms queries)
- Embeddings: ONNX/FastEmbed (semantic search)
- Parsing: tree-sitter (26 languages)

## Architecture
**Entry Point:** src/main.rs:59
**Core Logic:** src/search/, src/extractors/
**Critical Files:**
1. src/search/engine/mod.rs - Main search engine
2. src/extractors/typescript.rs - TypeScript extractor
3. src/tools/search.rs - Search tool MCP interface

## Key Workflows

### Adding a New Language
1. Add tree-sitter parser to Cargo.toml
2. Create extractor in src/extractors/{lang}.rs
3. Register in src/extractors/mod.rs
4. Add tests in src/tests/{lang}_tests.rs

### Debugging Search Issues
1. Check Tantivy index: .julie/indexes/{workspace}/tantivy/
2. Enable debug logging: RUST_LOG=debug
3. Inspect CASCADE fallback: SQLite → Tantivy → HNSW

## Testing
- Unit tests: `cargo test --lib`
- Integration: `cargo test --test '*'`
- Coverage: `cargo tarpaulin`

## Agent Guidelines
- ✅ ALWAYS search before coding (fast_search)
- ✅ USE get_symbols before reading full files
- ✅ CHECK references before modifying (fast_refs)
- ❌ NEVER skip TDD (write failing test first)
```

**Why This Matters:**
- **Zero manual work** - generated from code analysis
- **Always up-to-date** - regenerate after major changes
- **Agent-optimized** - only essential information
- **Consistent quality** - every project gets great docs

**Implementation Approach:**
- Use onboarding mode to find critical files
- Parse build configs for tech stack
- Analyze git history for workflows
- Use templates with intelligent fill-in
- Markdown output, structured for agents

---

### 6. Search Improvements - Fix Zero Results
**Purpose:** Great search that saves tokens, not wastes them

**The Problem:**
"I've seen way too many times the agent search with julie and get zero results back and I know it isn't right. I want us to have great search so we save tokens by only needing to return 3-5 results with high confidence, but we aren't saving anything by making an agent search 5 times and getting zero results every time."

**Current Issues:**
- Tantivy not being fully utilized
- Search falling back to SQLite too often
- Zero results when there should be matches
- No context lines in results (agents need surrounding code)
- Results not easily feedable to other tools

**The Solution:**

#### A. Return Context with Results
```rust
// Current: Just symbol location
{
  "file": "user.rs",
  "line": 234,
  "name": "getUserById"
}

// Enhanced: Include context
{
  "file": "user.rs",
  "line": 234,
  "name": "getUserById",
  "context": {
    "before": [
      "232: // Validate permissions",
      "233: if !auth.can_read(user_id) {"
    ],
    "match": "234: pub async fn getUserById(&self, id: &str) -> Result<User> {",
    "after": [
      "235:     let user = self.repo.find(id).await?;",
      "236:     Ok(user.to_dto())"
    ]
  },
  "confidence": 0.95
}
```

#### B. Better Tantivy Utilization
- **Stop falling back to SQLite so quickly**
- Tune Tantivy query parsing for code patterns
- Better tokenization for camelCase, snake_case
- Fuzzy matching with configurable edit distance
- Boost exact matches, penalize partial matches

#### C. Multi-Word Query Logic
```rust
// Current: "user authentication" → 0 results
// Why: Treated as phrase, no documents have exact phrase

// Fixed: "user authentication" → 30 results
// How: AND-first (both words), then OR (either word)
// Ranking: Both words > Word 1 > Word 2
```

#### D. Structured Results for Tool Interop
```rust
// Results include everything next tool needs:
{
  "results": [
    {
      "file": "user.rs",
      "line": 234,
      "symbol": {
        "name": "getUserById",
        "kind": "function",
        "signature": "pub async fn getUserById(&self, id: &str) -> Result<User>"
      },
      "context": { ... },
      "confidence": 0.95,
      // THIS IS KEY: Structured data for tool chaining
      "actions": {
        "smart_read": {
          "file": "user.rs",
          "target": "getUserById",
          "mode": "minimal"
        },
        "smart_refactor": {
          "file": "user.rs",
          "line": 234,
          "operation": "rename_symbol"
        }
      }
    }
  ],
  "total_found": 47,
  "returned": 5,
  "query_analysis": {
    "original": "get user data",
    "tokens": ["get", "user", "data"],
    "strategy": "AND-first (all 3 words), fallback OR (any word)"
  }
}
```

#### E. Confidence Scoring
Return 3-5 **high confidence** results, not 50 mediocre ones:
- Exact match: 1.0
- Fuzzy match (1 char diff): 0.9
- Partial match: 0.7
- Semantic match: 0.6
- **Filter**: Only show results > 0.6 confidence

#### F. Search Query Suggestions
When getting zero results:
```rust
{
  "results": [],
  "suggestions": [
    "Did you mean 'getUserData' instead of 'getUserDat'?",
    "Try searching for 'user' or 'data' separately",
    "Enable semantic mode for conceptual matches: mode='semantic'"
  ],
  "debug": {
    "tantivy_attempted": true,
    "tantivy_error": null,
    "sqlite_fallback": true,
    "semantic_available": true
  }
}
```

**Implementation Priorities:**
1. **Add context lines** to all search results (immediate win)
2. **Fix Tantivy utilization** (stop premature SQLite fallback)
3. **Multi-word AND/OR logic** (handle "user authentication" correctly)
4. **Structured output** for tool interoperability
5. **Confidence filtering** (quality over quantity)
6. **Search suggestions** (guide agents to success)

---

## 🔗 Tool Interoperability - The Multi-Step Workflow

**Core Principle:** "Results from fast_search should be feedable to other tools"

### Example: Rename Symbol Workflow
```rust
// Step 1: Search
fast_search query="getUserById" mode="text"
→ Returns structured results with file, line, context

// Step 2: Verify Impact (feed results directly)
fast_refs symbol="getUserById"
→ Uses search results, shows all 47 usages

// Step 3: Refactor (feed results directly)
smart_refactor operation="rename_symbol" params='{
  "old_name": "getUserById",
  "new_name": "fetchUser",
  "update_comments": true
}'
→ Uses fast_refs results, renames all occurrences

// Step 4: Validate
ast_fix file="user.rs" mode="validate"
→ Confirms syntax still valid
```

### Structured Data Format
All tools return JSON with:
```json
{
  "status": "success",
  "data": { ... },
  "structured_content": {
    // Machine-readable data for next tool
  },
  "actions": {
    // Suggested next tools with pre-filled params
  }
}
```

### Tool Chain Support
- Search tools provide locations
- Navigation tools provide definitions
- Edit tools consume locations
- Validation tools verify results
- **No manual data transformation needed**

---

## 📊 Success Metrics

| Metric | Current | Target | How We Measure |
|--------|---------|--------|----------------|
| **Syntax error recovery** | Manual (5-10 min) | Auto-fix 95% | Track ast_fix usage |
| **Token usage per task** | 10,000 tokens avg | 3,000 tokens avg | Compare Read vs smart_read |
| **Search retry rate** | 30-40% | <5% | Track searches per successful result |
| **Zero-result searches** | 15-20% | <2% | Count empty result sets |
| **Agent tool adoption** | 60% | 90% | Julie tools vs built-in tools |
| **Context exhaustion** | 20% of sessions | <5% | Track compaction triggers |
| **Documentation time** | 2-4 hours | 5 minutes | Time to create CLAUDE.md |
| **Onboarding time** | 30+ min exploration | <2 min with onboarding | Time to first productive edit |

---

## 🚀 Implementation Phases

### Phase 1: Critical Fixes (Week 1)
**Goal:** Fix existing problems, quick wins

1. **Add context lines to fast_search** (2 days)
   - Update FastSearchTool to include context
   - Test with agents, verify token savings

2. **Fix Tantivy zero-results** (2 days)
   - Debug CASCADE fallback logic
   - Improve multi-word query handling
   - Add fuzzy matching tuning

3. **Structured output for interoperability** (1 day)
   - Define standard JSON format
   - Update all search tools
   - Add "actions" hints for next tools

### Phase 2: Smart Reading & Syntax (Week 2)
**Goal:** Major token savings + error elimination

4. **Smart Read Tool** (3 days)
   - Extend GetSymbolsTool with code extraction
   - Implement minimal/business_logic/dependencies modes
   - Test context savings (target: 70-90%)

5. **AST-Based Fix Tool** (3 days)
   - Implement auto/diagnose/reformat modes
   - Use tree-sitter error detection
   - Build auto-fix rules for common issues

### Phase 3: Intelligence & Generation (Week 3)
**Goal:** Onboarding + documentation automation

6. **Onboarding Mode for fast_explore** (4 days)
   - Implement criticality scoring
   - Add path analysis and filtering
   - Test on diverse projects

7. **Auto-Generate Docs** (2 days)
   - Create doc generation logic
   - Use onboarding mode for analysis
   - Template system for CLAUDE.md/AGENTS.md

### Phase 4: Advanced Features (Week 4)
**Goal:** Semantic diff + polish

8. **Semantic Diff Tool** (3 days)
   - Combine DMP + AST + embeddings
   - Implement behavioral/structural/impact modes
   - Agent-friendly output format

9. **Testing & Refinement** (3 days)
   - Real-world agent testing
   - Performance optimization
   - Documentation and examples

---

## 🎯 Design Principles

1. **No Tool Explosion**
   - Extend existing tools where possible
   - Combine related functionality
   - Use mode parameters, not separate tools

2. **Leverage What We Built**
   - Tree-sitter: AST analysis, syntax validation
   - Tantivy: Fast search, proper utilization
   - HNSW Embeddings: Semantic understanding
   - DMP: Text diffing, fuzzy matching

3. **Agent Psychology**
   - Deterministic results (agents trust consistency)
   - Clear error messages (guide to success)
   - Confidence scores (transparency)
   - Structured data (easy chaining)

4. **Token Optimization**
   - Progressive disclosure (summary → details)
   - Context-aware filtering (show what matters)
   - Smart defaults (minimal mode default)
   - Explicit verbosity control (user choice)

5. **Tool Interoperability**
   - Structured JSON output
   - Standard format across tools
   - "Next action" suggestions
   - Zero manual data transformation

---

## 💡 Key Insights from Discussions

### On Tool Design
> "would an agent use this tool? would an agent benefit from this tool? or would an agent just ignore this?"

Every tool must pass this test. If agents won't use it, don't build it.

### On Search Quality
> "I want us to have great search so we save tokens by only needing to return 3-5 results with high confidence, but we aren't saving anything by making an agent search 5 times and getting zero results every time."

Quality over quantity. 5 perfect results > 50 mediocre results > 0 results.

### On Documentation
> "an onboarding tool that can generate perfect CLAUDE.md or AGENTS.md files would be awesome too. Optimized for ai agents, no fluff, just everything important for an agent to have instant understanding of a project."

Auto-generation > manual writing. Always up-to-date, always agent-optimized.

### On Interoperability
> "the results from fast_search should be feedable to other tools too. We don't want tool explosion, but we do want to make sure we're leveraging the things we spent so much time and money to build."

Structured data enables tool chains. One search result can feed rename, refactor, navigate, diff.

---

## 📝 Next Steps

1. **Review this document** with team
2. **Prioritize tools** based on impact vs effort
3. **Start with Phase 1** (critical fixes)
4. **Test with real agents** after each phase
5. **Iterate based on feedback** (measure success metrics)

---

## 🔗 Related Documents

- [julie_tool_ideas.md](./julie_tool_ideas.md) - Original tool brainstorming
- [julie_next_steps.md](./julie_next_steps.md) - Implementation roadmap
- [CLAUDE.md](../../CLAUDE.md) - Current project documentation
- [TODO.md](../../TODO.md) - Current development priorities

---

**Remember:** These tools exist to make agents successful. If an agent struggles, we failed. If an agent thrives, we succeeded.
