# Code Intelligence Audit & Optimization

**Date:** 2026-02-05
**Status:** Approved
**Goal:** Verify Julie's data foundation is rock-solid, then optimize every tool to surpass what LSP-based MCP servers can offer AI agents.

## Context

Julie removed semantic embeddings (ONNX/HNSW) because the complexity and performance cost didn't justify the value. The "budget" freed up is being reinvested into:

1. **Best-in-class code search** — Tantivy with custom CodeTokenizer and per-language idiom configs
2. **Maximum extraction from tree-sitter** — 30 language extractors producing symbols, identifiers, types, and relationships
3. **Agent-unique capabilities** — things no LSP can do: cross-language tracing, business logic discovery, codebase-wide pattern queries, architectural analysis

The competitive landscape: if Claude Code (or similar) adds native LSP support, any MCP server that just wraps LSP features becomes redundant. Julie's moat is the intelligence layer *above* what an LSP provides.

---

## Phase 1: Data Pipeline Audit ("The Foundation")

**Goal:** Verify we're extracting, storing, and making queryable every piece of intelligence our tree-sitter extractors can produce.

**Method:** Automated census + representative deep-dives, traced end-to-end through the pipeline (extract → store → index → query).

### Step 1 — Automated Census

Write a test/script that runs each extractor against its test fixtures and counts output:

| Language | Symbols | Identifiers | Types | Relationships | Pending Rels |
|----------|---------|-------------|-------|---------------|-------------|
| Rust     | ?       | ?           | ?     | ?             | ?           |
| TypeScript | ?     | ?           | ?     | ?             | ?           |
| Python   | ?       | ?           | ?     | ?             | ?           |
| ...      |         |             |       |               |             |

This surfaces which extractors are rich vs. thin. Languages missing certain extractors get flagged but aren't necessarily problems (e.g., type extraction from TOML doesn't make sense).

### Step 2 — Pipeline Trace (4 Representative Languages)

For **Rust, TypeScript, Python, Go** — trace one function end-to-end:

1. What tree-sitter nodes does the parser produce?
2. What does our extractor turn those into (Symbol, Identifier, TypeInfo, Relationship)?
3. Does it all land in SQLite correctly (all columns populated)?
4. Does content make it into Tantivy with proper tokenization?
5. Can we query it back out meaningfully through each relevant tool?

### Step 3 — Gap Analysis

Identify data we *could* extract but aren't. Known suspects:

- **PendingRelationship resolution** — are these actually getting resolved to real relationships, or sitting unresolved?
- **`semantic_group`** — is cross-language linking actually functioning?
- **Decorator/attribute metadata** — `#[derive(Clone)]`, `@deprecated`, `@Test` carry rich semantic info
- **Import graphs** — we have `Import` identifiers but are we building a complete module dependency picture?
- **Pattern metadata** — e.g., is this function async? Is this class abstract? Is this method a getter/setter?

### Estimated Sessions: 1-2

---

## Phase 2: Tool Categorization ("The Strategy")

**Goal:** Evaluate every exposed tool through the lens of "does this give an AI agent something it can't get from an LSP-based MCP server?"

### Current Tools (15)

**Bucket A — Table-stakes (LSP can do this, keep sharp, don't over-invest):**
- `fast_goto` — go-to-definition
- `fast_refs` — find references
- `rename_symbol` — workspace-wide rename

**Bucket B — Julie's moat (invest heavily, this is the differentiator):**
- `fast_search` — codebase-wide content search with code-aware tokenization and BM25 ranking
- `get_symbols` — file structure + targeted code extraction with token-efficient output (genuinely better than LSP document symbols — provides code bodies, filtered by target, with multiple verbosity modes)
- `trace_call_path` — cross-language execution flow tracing
- `fast_explore` — business logic discovery, dependency analysis, type exploration
- `edit_symbol` — AST-aware editing with fuzzy matching
- `checkpoint` / `recall` / `plan` — persistent development memory

**Bucket C — Evaluate keep/kill/consolidate:**
- `find_logic` — already deprecated, redirect to `fast_explore(mode='logic')`
- `edit_lines` / `fuzzy_replace` — DMP-powered editing has unrealized potential, but agents rarely reach for these in practice. Investigate why during audit.

### Deliverable

A prioritized list of tools for Phase 3 deep-dives, plus decisions on Bucket C tools.

### Estimated Sessions: 1

---

## Phase 3: Tool Deep-Dives ("The Optimization")

**Goal:** One tool per session, full-context audit and optimization.

### Checklist Per Tool

1. **Data utilization** — which SQLite tables and Tantivy indexes does it query? What data sources is it ignoring that could improve results?
2. **Output optimization** — is the format token-efficient? Does it give the agent what it actually needs, or does it include noise?
3. **Semantic remnants** — dead parameters, unused code paths, stale descriptions referencing embeddings/HNSW/ONNX
4. **Code quality** — Rust anti-patterns, early-development cruft (unnecessary clones, stringly-typed logic that should be enums, missing error context, etc.)
5. **Test coverage** — fill gaps discovered during audit, validate findings
6. **Dogfood test** — run against Julie's own codebase, verify real-world quality

### Priority Order

| Priority | Tool | Rationale |
|----------|------|-----------|
| 1 | `fast_search` | Core differentiator, powers discovery |
| 2 | `get_symbols` | Most-used tool, massive token savings |
| 3 | `fast_refs` | Critical for impact analysis before changes |
| 4 | `trace_call_path` | Unique cross-language capability, may be underperforming |
| 5 | `fast_explore` | Broad tool with multiple modes, might be overloaded |
| 6 | `fast_goto` | Simple but essential, probably already solid |
| 7 | Editing tools | Determine keep/kill/consolidate based on evidence |
| 8 | Memory tools | Recently overhauled, likely in good shape |
| 9 | `manage_workspace` | Infrastructure tool, audit for completeness |

### Estimated Sessions: ~9 (one per tool or tool group)

---

## Phase 4: Documentation Cleanup ("The Last Mile")

**Goal:** Update all documentation to reflect current architecture (post-embeddings-removal, post-Tantivy-migration, post-tool-audit).

This phase goes LAST because everything before it changes what needs to be documented.

### Scope

1. Sweep all `docs/*.md` — remove references to embeddings, HNSW, CASCADE 2-tier, ONNX, semantic search
2. Update architecture docs to reflect Tantivy + SQLite as the two pillars
3. Update `CLAUDE.md` — the most important doc (agents read it first)
4. Update tool descriptions in `handler.rs` — literally the first thing an agent sees about each tool
5. Remove entirely obsolete docs rather than trying to salvage them

### Estimated Sessions: 1-2

---

## Cross-Cutting Concerns (Carried Throughout All Phases)

### Tantivy as General-Purpose Resource
Tantivy is more than search — it's a scoring engine, tokenizer, and full-text index we control end-to-end. Opportunities beyond `fast_search`:
- Ranking/scoring for any tool output (e.g., relevance-ranked `fast_refs` results)
- Fuzzy symbol resolution for PendingRelationship matching
- Content-aware filtering in any tool that returns lists
- Doc comment search

### Semantic Search Remnant Cleanup
As we audit each tool, watch for and remove:
- Dead parameters referencing embeddings or semantic search
- Unused code paths that checked for embedding availability
- Stale tool descriptions mentioning semantic capabilities
- Import statements for removed modules

### Rust Code Quality
Watch for early-development patterns that modern models would write differently:
- Unnecessary `.clone()` calls
- Stringly-typed logic that should use enums
- Missing error context (bare `.unwrap()` or `?` without `.context()`)
- Overly complex lifetimes or trait bounds
- Functions doing too many things

### Agent-First Output Optimization
Every tool output should be evaluated for:
- Token efficiency (is there noise that can be removed?)
- Structured vs. prose (agents parse structured data more reliably)
- Actionable information (does the output tell the agent what to do next?)
- TOON format adoption where appropriate

---

## Success Criteria

When this audit is complete:

1. Every extractor's output is verified as comprehensive for its language
2. All extracted data flows correctly through SQLite and Tantivy
3. Every tool leverages the full data foundation available to it
4. Tool output is optimized for AI agent consumption
5. No remnants of the removed semantic search system remain
6. Code quality meets current Rust standards
7. Test coverage validates all findings
8. Documentation accurately reflects current architecture
9. Julie provides capabilities that no LSP-based MCP server can match
