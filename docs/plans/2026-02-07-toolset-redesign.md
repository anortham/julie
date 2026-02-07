# Toolset Redesign — From 14 to 9 Tools

**Date:** 2026-02-07
**Status:** Complete

## Motivation

Julie recently completed a major architecture shift: removing semantic search/embeddings in favor of Tantivy full-text search with code-aware tokenization. With the search foundation rebuilt, it's time to re-evaluate the tool layer above it.

### Design Principles

1. **Julie's moat is intelligence, not editing.** Hosts (Claude Code, Cursor, Windsurf) ship their own editing primitives and are adding LSP functionality. We should get out of their way and double down on what they can't do.
2. **Smaller toolset = better adoption.** Agents internalize fewer tools faster. Every tool we expose competes for attention in agent instructions.
3. **Don't build tools just because we can.** We have 30+ treesitter extractors and rich indexed data. That's a foundation, not a mandate to expose every query as a tool.
4. **Tools for data access, skills for strategy.** A tool should exist when it needs direct access to indexed data that an agent can't efficiently assemble from existing tools. Exploration strategies and multi-step workflows belong in skills.

## Changes Overview

| Action | Count | Tools |
|--------|-------|-------|
| Remove | 6 | `fast_goto`, `edit_lines`, `edit_symbol`, `fuzzy_replace`, `trace_call_path`, `fast_explore` |
| Enhance | 1 | `fast_search` (absorbs `fast_goto` intelligence) |
| Add | 1 | `deep_dive` (new tool) |
| Keep | 7 | `get_symbols`, `fast_refs`, `rename_symbol`, `manage_workspace`, `checkpoint`, `recall`, `plan` |
| **Total** | **9** | Down from 14 |

---

## Removals

### `fast_goto` — Absorbed into `fast_search`

**Reason:** 80%+ overlap with `fast_search(search_target="definitions")`. The only differentiator is precision for exact symbol matches, which is better handled as smart behavior inside `fast_search` rather than a separate tool.

### `edit_lines` — Host tools cover this

**Reason:** Claude Code's native `Edit` tool, Cursor's editing, and similar host tools all handle line-based editing. The dry-run preview was the only differentiator, and it wasn't enough to justify a separate tool.

### `edit_symbol` — Host tools cover this

**Reason:** `replace_body` was the only frequently-used operation. `insert_relative` was occasional, `extract_to_file` was barely touched. Not enough unique value over host editing tools.

### `fuzzy_replace` — Host tools cover this

**Reason:** Single-file mode competes directly with host Edit tools. Multi-file mode (`file_pattern`) was unique but rarely used, and `rename_symbol` covers the primary cross-file refactoring use case.

### `trace_call_path` — Becomes a skill

**Reason:** Great concept, noisy in practice. Cross-language tracing generates false positives even after generic name filtering. An agent orchestrating `fast_refs` intelligently (via a skill) can actually produce better results because the agent's reasoning filters noise more effectively than heuristic code. Single-language call tracing is achievable by chaining `fast_refs` calls.

### `fast_explore` — Modes become skills

**Reason:** Three very different tools crammed into one with a `mode` parameter. `similar` mode is dead (embeddings removed). `logic`, `dependencies`, and `types` modes are exploration strategies better served as skills that teach agents how to orchestrate the core tools.

---

## Enhancement: `fast_search` — Exact Symbol Match Promotion

### Current Behavior

`fast_search(query="SearchIndex", search_target="definitions")` returns a flat list of matching definitions ranked by Tantivy score. No distinction between exact matches and partial matches.

### Enhanced Behavior

When `search_target="definitions"` and a result is an **exact name match** for the query, promote it to the top with richer definition-quality info:

```
Definition found: SearchIndex
  src/search/index.rs:108 (struct, public)
  pub struct SearchIndex { ... }

Other matches:
  src/search/tokenizer.rs:45  ... SearchIndex::new() ...
  src/tests/search_tests.rs:12  ... SearchIndex ...
```

### Implementation

After running the Tantivy search, check if any result has `name == query` (exact match). If so:

1. Promote it to the top of results
2. Fetch full symbol details from the symbols table (kind, visibility, full signature)
3. Format with a "Definition found:" header
4. Remaining results appear below as "Other matches:"

If no exact match exists, behavior is unchanged. Zero overhead when it doesn't apply.

### Parameters

No new parameters needed. The intelligence is automatic.

---

## New Tool: `deep_dive`

### Purpose

Given a symbol, return everything an agent needs to understand it in a single call. Replaces the common 3-4 tool chain of `fast_search` → `get_symbols` → `fast_refs` → `Read`.

### Parameters

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `symbol` | String | required | Symbol name to investigate (supports qualified names like `Processor::process`) |
| `depth` | enum | `"overview"` | `"overview"`, `"context"`, or `"full"` |
| `context_file` | String? | null | Disambiguate when multiple symbols share a name |
| `workspace` | String? | `"primary"` | Workspace filter |

### Depth Levels

**`overview`** (default) — *"Give me the lay of the land"* — ~200 tokens
- Symbol signature, kind, visibility, location
- Incoming references: names + locations (capped at 10)
- Outgoing references: names + locations (capped at 10)
- Type names used in signature (names only, no definitions)
- **No implementation body.** Keeps it tight.

**`context`** — *"I'm about to work on this"* — ~600 tokens
- Everything in overview, plus:
- Implementation body of the target symbol
- Incoming/outgoing reference signatures (not just names)
- Type signatures for parameter/return types (one-line summaries)

**`full`** — *"I need the complete picture"* — ~1500 tokens
- Everything in context, plus:
- Caller/callee implementation bodies
- Type definition bodies
- Related test locations

### Kind-Aware Behavior

The tool detects the symbol's kind and adjusts what "incoming" and "outgoing" references mean:

#### Function / Method
```
src/payment/processor.rs:42 (fn, public)
  pub fn process_payment(order: &Order, method: PaymentMethod) -> Result<Receipt>

Callers (3):
  src/api/checkout.rs:88       checkout_handler()
  src/api/admin.rs:142         manual_charge()
  src/workers/billing.rs:31    process_pending()

Callees (2):
  src/payment/validate.rs:15   validate_payment()
  src/payment/gateway.rs:55    charge_gateway()

Types:
  Order          src/models/order.rs:12       struct
  PaymentMethod  src/models/payment.rs:5      enum
  Receipt        src/models/payment.rs:45     struct
```

#### Trait / Interface
```
src/payment/mod.rs:8 (trait, public)
  pub trait PaymentGateway

Required Methods (2):
  fn authorize(&self, amount: Money) -> Result<AuthToken>
  fn capture(&self, token: AuthToken) -> Result<Receipt>

Implementations (3):
  src/payment/stripe.rs:15      StripeGateway
  src/payment/paypal.rs:12      PayPalGateway
  src/payment/mock.rs:8         MockGateway
```

#### Struct / Class
```
src/models/order.rs:12 (struct, public)
  pub struct Order

Fields (4):
  id: OrderId
  items: Vec<LineItem>
  total: Money
  status: OrderStatus

Methods (3):
  fn new(items: Vec<LineItem>) -> Self          :28
  fn add_item(&mut self, item: LineItem)        :35
  fn calculate_total(&self) -> Money            :42

Implements:
  Display, Serialize, Deserialize

Used By (5):
  src/payment/processor.rs:42   process_payment()
  src/api/checkout.rs:60        create_order()
  ...
```

#### Module / Namespace
```
src/payment/mod.rs (module)

Public Exports (4):
  trait  PaymentGateway          :8
  fn     process_payment()       :42
  struct Receipt                 :55
  enum   PaymentMethod           :70

Dependencies (3):
  src/models/order.rs            Order, LineItem
  src/models/money.rs            Money
  src/errors/mod.rs              PaymentError
```

### Reference Caps

| Depth | Callers/Callees Cap | Types Cap |
|-------|-------------------|-----------|
| overview | 10 | 10 |
| context | 15 | 15 |
| full | uncapped | uncapped |

When capped, show total count: `Callers (10 of 23):`

These caps are initial values — tune after dogfooding.

### Implementation Strategy

The tool combines data from existing indexed sources:

1. **Symbol lookup:** Query symbols table for exact name match (with `context_file` disambiguation)
2. **Relationship query:** Query relationships table for incoming (callers, implementors) and outgoing (callees, types used) relationships
3. **Identifier fallback:** Query identifiers table for references not captured in relationships
4. **Kind detection:** Check `symbol.kind` to determine output shape
5. **Depth gating:** Only fetch bodies/additional details for `context` and `full` depths
6. **Formatting:** Lean text format matching existing tool output style

All data already exists in SQLite. No new indexing required.

---

## Final Toolset (9 tools)

### Code Intelligence (4 tools)
| Tool | Purpose |
|------|---------|
| `fast_search` | Content search + definition search with exact-match promotion |
| `get_symbols` | File structure overview with progressive detail |
| `fast_refs` | Find all references to a symbol |
| `deep_dive` | Progressive-depth, kind-aware symbol context |

### Refactoring (1 tool)
| Tool | Purpose |
|------|---------|
| `rename_symbol` | AST-aware cross-file rename across 30 languages |

### Workspace (1 tool)
| Tool | Purpose |
|------|---------|
| `manage_workspace` | Indexing, health checks, workspace registry |

### Memory (3 tools)
| Tool | Purpose |
|------|---------|
| `checkpoint` | Save development memory |
| `recall` | Retrieve development memories |
| `plan` | Manage working plans |

---

## Post-Toolset Work: Skills

After the toolset changes ship, convert removed tool functionality into skills:

- **Call path tracing skill** — teaches agents to trace execution flow using `fast_refs` chains
- **Architecture exploration skill** — teaches agents to understand module structure using `get_symbols` + `fast_search`
- **Dependency analysis skill** — teaches agents to analyze transitive dependencies using `fast_refs` + `deep_dive`
- **Type hierarchy exploration skill** — teaches agents to explore type hierarchies using `deep_dive` on traits/interfaces

Skills don't add to tool count and leverage agent reasoning for noise filtering.

---

## Implementation Phases

### Phase 1: Cut (Low risk, immediate simplification)
- Remove `edit_lines`, `edit_symbol`, `fuzzy_replace`, `fast_explore`, `trace_call_path`, `fast_goto`
- Remove from MCP tool registration, delete tool modules
- Update agent instructions
- **Tests:** Verify remaining tools still pass, remove tests for deleted tools

### Phase 2: Enhance `fast_search` (Low risk, targeted change)
- Add exact-match detection in definition search
- Add promoted definition formatting
- **Tests:** Test exact match promotion, test no-match-unchanged behavior

### Phase 3: Build `deep_dive` (New feature, medium complexity)
- Implement core symbol lookup + relationship aggregation
- Implement kind-aware output formatting
- Implement depth-gated detail levels
- Implement reference capping
- **Tests:** Test each kind at each depth level, test disambiguation, test capping
