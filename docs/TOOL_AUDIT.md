# Julie Tool Audit (2025-11-11)

## Purpose

Following the completion of memory embeddings optimization (v1.6.1), we're conducting a systematic audit of all exposed MCP tools to ensure each tool:
1. Leverages Julie's semantic search capabilities (the crown jewel)
2. Uses optimal search modes (text/semantic/hybrid)
3. Takes advantage of memory system integration where relevant
4. Has optimal default parameters and filters
5. Provides clear tool descriptions for behavioral adoption
6. Avoids redundancy with other tools
7. Outputs results in optimal format for agent consumption

## Context: What Changed

**Recent Improvements (v1.6.0 - v1.6.1):**
- ✅ 88.7% embedding reduction for .memories/ files
- ✅ Custom RAG pipeline with focused "{type}: {description}" format
- ✅ Critical bug fixes: search ranking (2.0x boost) + escaped quotes (serde_json)
- ✅ Production-ready semantic search with HNSW
- ✅ Optimized CASCADE architecture (SQLite FTS5 → HNSW Semantic)

**Hypothesis:** With mature semantic search, some tools may benefit from:
- Switching from text-only to semantic or hybrid search
- Adding semantic search options where missing
- Adjusting default parameters based on new performance characteristics
- Better integration with memory system for context-aware results

---

## Tool Inventory & Audit Checklist

### 🔍 Search & Discovery
- [ ] **fast_search** - Primary search tool (text/semantic/hybrid modes) - **PRIORITY 1**
- [ ] **fast_explore** - Workspace exploration/file discovery

### 🧭 Navigation
- [ ] **fast_goto** - Jump to symbol definitions - **PRIORITY 2**
- [ ] **fast_refs** - Find all symbol references - **PRIORITY 2**
- [ ] **trace_call_path** - Trace execution paths across languages - **PRIORITY 3**

### 📦 Symbols & Code Structure
- [ ] **get_symbols** - Extract symbol structure from files - **PRIORITY 2**

### 🔨 Refactoring & Editing
- [ ] **fuzzy_replace** - Pattern-based replacements with fuzzy matching
- [ ] **edit_lines** - Line-level surgical edits
- [ ] **rename_symbol** - Workspace-wide symbol renaming
- [ ] **edit_symbol** - Symbol-aware editing (replace body, insert relative, extract)

### 💾 Memory System
- [ ] **checkpoint** - Save immutable memories
- [ ] **recall** - Query past memories
- [ ] **plan** - Mutable plans (save/get/list/activate/update/complete)

### 🗂️ Workspace Management
- [ ] **manage_workspace** - Index, add, remove, health check, stats

### 🎯 Business Logic Discovery
- [ ] **find_logic** - Filter framework boilerplate, find domain logic - **PRIORITY 3**

---

## Audit Framework

For each tool, we evaluate:

### 1. Current State Analysis
- **Purpose**: What does this tool do?
- **Search Strategy**: text/semantic/hybrid/none?
- **Embedding Usage**: How does it leverage semantic search?
- **Memory Integration**: Does it consider project memory context?
- **Parameter Defaults**: Are limits/filters optimal?

### 2. Optimization Questions
1. **Search Mode**: Is it using the optimal search strategy given the task?
2. **Semantic Potential**: Could semantic search improve results?
3. **Memory Context**: Should it integrate with memory system?
4. **Parameters**: Are defaults optimal for typical use cases?
5. **Tool Description**: Does it guide agents to use it correctly?
6. **Redundancy**: Does it overlap with other tools? Can we consolidate?
7. **Output Format**: Is the output optimal for agent consumption?

### 3. Recommendations
- **Keep As-Is**: No changes needed
- **Optimize**: Specific improvements identified
- **Investigate**: Needs deeper analysis
- **Deprecate**: Consider removing if redundant

---

## Tool Audits (In Priority Order)

---

## 1. fast_search ⭐ **CROWN JEWEL** - PRIORITY 1

### Current State
**Purpose:** Primary search interface with three modes:
- `text` - Fast FTS5 search with BM25 ranking (<5ms)
- `semantic` - HNSW similarity search (<50ms)
- `hybrid` - Combines both for balanced results

**Search Strategy:** User-selectable via `search_method` parameter

**Embedding Usage:**
- Semantic mode uses optimized embeddings
- Recently optimized: removed code_context, added memory-specific pipeline
- HNSW index rebuilt with cleaner signals

**Memory Integration:**
- Can search .memories/ files via file_pattern
- Memory descriptions get 2.0x boost in semantic mode (recent fix)

**Parameters:**
```rust
query: String           // Required
search_method: String   // "text" | "semantic" | "hybrid"
limit: u32             // Default: varies by mode
search_target: String  // "content" | "definitions"
file_pattern: Option<String>
language: Option<String>
workspace: Option<String>
context_lines: Option<u32>  // For content mode
```

### Audit Questions

#### 1. Is it using optimal search strategy?
- ✅ **User controls mode** - Flexible, allows agent to choose
- 🤔 **Question:** Should we change default mode recommendations in tool description?
- 🤔 **Question:** Is hybrid mode being used effectively? Do agents know when to use it?

#### 2. Could semantic search improve results?
- ✅ **Already available** - Semantic mode works well
- 🤔 **Question:** Are agents using semantic mode enough? Or defaulting to text?
- 🤔 **Question:** Should we recommend semantic for certain query patterns?

#### 3. Memory integration?
- ✅ **Works via file_pattern** - Can target `.memories/`
- ✅ **Recent fix:** 2.0x boost for memory descriptions
- 🤔 **Question:** Should we add a `search_memories: bool` convenience parameter?

#### 4. Parameters optimal?
- ✅ **Comprehensive** - Good coverage of use cases
- 🤔 **Question:** Default limits - are they right for each mode?
  - Text: Fast, could handle higher default
  - Semantic: Slower, current defaults probably fine
- 🤔 **Question:** Should `context_lines` have different defaults by mode?

#### 5. Tool description guiding agents?
- 📝 **TODO:** Review tool description in handler.rs
- 📝 **TODO:** Check if it explains when to use text vs semantic vs hybrid
- 📝 **TODO:** Verify examples show best practices

#### 6. Redundancy?
- ✅ **No redundancy** - This is the primary search interface
- ✅ **Other tools delegate to this** - Good architecture

#### 7. Output format?
- ✅ **Symbol-based** - Consistent with other tools
- ✅ **Includes context** - code_context field for results
- 🤔 **Question:** Does semantic mode need different output format?

### Recommendations

**🔍 INVESTIGATE:**
1. Review actual agent usage patterns - text vs semantic vs hybrid
2. Check tool description for clarity on mode selection
3. Consider if semantic should be recommended for:
   - Conceptual queries ("how does auth work?")
   - Cross-file pattern discovery
   - Memory searches

**✅ KEEP AS-IS:**
- Three-mode architecture is solid
- Parameters are comprehensive
- Recent optimizations (embeddings, memory boost) working well

**🚧 POTENTIAL IMPROVEMENTS:**
- [ ] Add usage guidance in tool description for mode selection
- [ ] Consider `search_memories: bool` convenience parameter
- [ ] Review default limits per mode
- [ ] Add examples of when to use each mode

**Status:** 🟡 NEEDS REVIEW - Tool is solid, but need to verify agent usage patterns and tool description

---

## 2. get_symbols - PRIORITY 2

### Current State
**Purpose:** Extract symbol structure from files without reading full content

**Search Strategy:** N/A (direct file operation)

**Embedding Usage:** N/A

**Memory Integration:** N/A

**Parameters:**
```rust
file_path: String
max_depth: u32        // Symbol nesting depth
limit: Option<u32>    // Max symbols to return
mode: Option<String>  // "structure" | "minimal" | "full"
target: Option<String> // Filter to specific symbol
workspace: Option<String>
```

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 3. fast_goto - PRIORITY 2

### Current State
**Purpose:** Jump directly to symbol definitions with fuzzy matching

**Search Strategy:** Symbol name lookup with semantic fallback

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** N/A

**Parameters:**
```rust
symbol: String
context_file: Option<String>
line_number: Option<u32>
workspace: Option<String>
```

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 4. fast_refs - PRIORITY 2

### Current State
**Purpose:** Find all references/usages of a symbol across workspace

**Search Strategy:** [TO BE DETERMINED]

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** N/A

**Parameters:**
```rust
symbol: String
include_definition: bool
limit: u32
workspace: Option<String>
```

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 5. trace_call_path - PRIORITY 3

### Current State
**Purpose:** Trace execution paths across language boundaries (TypeScript → Go → Python → SQL)

**Search Strategy:** [TO BE DETERMINED]

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** N/A

**Parameters:**
```rust
symbol: String
direction: String  // "upstream" | "downstream" | "both"
max_depth: u32
output_format: String  // "json" | "tree"
context_file: Option<String>
workspace: Option<String>
```

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 6. find_logic - PRIORITY 3

### Current State
**Purpose:** Discover core business logic by filtering out framework boilerplate

**Search Strategy:** Business relevance scoring + architectural layer grouping

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** [TO BE DETERMINED]

**Parameters:**
```rust
domain: String  // Business domain keywords
max_results: u32
group_by_layer: bool
min_business_score: f32
```

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 7. fuzzy_replace

### Current State
**Purpose:** Pattern-based replacements with fuzzy matching across files

**Search Strategy:** Fuzzy string matching with threshold

**Embedding Usage:** N/A (text-based)

**Memory Integration:** N/A

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 8. edit_lines

### Current State
**Purpose:** Surgical line-level edits (insert/replace/delete)

**Search Strategy:** N/A (direct line operations)

**Embedding Usage:** N/A

**Memory Integration:** N/A

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 9. rename_symbol

### Current State
**Purpose:** Workspace-wide symbol renaming with semantic awareness

**Search Strategy:** [TO BE DETERMINED]

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** N/A

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 10. edit_symbol

### Current State
**Purpose:** Symbol-aware editing (replace body, insert relative, extract to file)

**Search Strategy:** [TO BE DETERMINED]

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** N/A

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 11. checkpoint

### Current State
**Purpose:** Save immutable development memories

**Search Strategy:** N/A (write operation)

**Embedding Usage:** ✅ **Just optimized** - Memory-specific pipeline

**Memory Integration:** ✅ **IS the memory system**

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET (but recently optimized in v1.6.1)

---

## 12. recall

### Current State
**Purpose:** Query past memories chronologically or semantically

**Search Strategy:** SQL view for chronological, delegates to fast_search for semantic

**Embedding Usage:** ✅ **Via fast_search** when semantic

**Memory Integration:** ✅ **IS the memory system**

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET (but recently optimized in v1.6.1)

---

## 13. plan

### Current State
**Purpose:** Mutable plans (save/get/list/activate/update/complete)

**Search Strategy:** Direct file operations + SQL views

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** ✅ **Part of memory system**

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 14. manage_workspace

### Current State
**Purpose:** Workspace administration (index, add, remove, health, stats, clean, refresh)

**Search Strategy:** N/A (administrative operations)

**Embedding Usage:** Triggers embedding generation during indexing

**Memory Integration:** Indexes .memories/ files

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## 15. fast_explore

### Current State
**Purpose:** Workspace exploration and file discovery

**Search Strategy:** [TO BE DETERMINED]

**Embedding Usage:** [TO BE DETERMINED]

**Memory Integration:** [TO BE DETERMINED]

### Audit Questions

#### 1-7. [TO BE FILLED DURING AUDIT]

### Recommendations
**Status:** ⬜ NOT AUDITED YET

---

## Summary of Findings

### Tools Audited: 0/15

### High-Level Patterns
- [ ] [TO BE FILLED AS AUDIT PROGRESSES]

### Quick Wins Identified
- [ ] [TO BE FILLED AS AUDIT PROGRESSES]

### Long-Term Improvements
- [ ] [TO BE FILLED AS AUDIT PROGRESSES]

---

## Next Steps

1. ✅ Create this audit document
2. ⬜ Start with fast_search (PRIORITY 1)
3. ⬜ Continue through high-priority tools
4. ⬜ Document findings and implement changes
5. ⬜ Create checkpoint when audit complete

---

**Last Updated:** 2025-11-11 (Initial creation)
**Status:** 🟡 In Progress - Starting with fast_search
