# Phase 3: `get_context` Tool

**Date:** 2026-02-24
**Status:** Design Approved
**Depends on:** Phase 1 (OR fallback search), Phase 2 (centrality ranking)

## Context

Inspired by vexp.dev's "context capsule" concept — a single tool call that returns a token-budgeted subgraph of relevant code for a given query. Julie has all the primitives (search, relationships, token estimation, symbol extraction) but currently requires the agent to orchestrate 3-4 separate tool calls to assemble context for a task.

`get_context` composes these primitives into a single server-side operation: search → rank by centrality → expand graph → adaptive token allocation → formatted output.

## Problem Statement

Today's agent workflow for understanding an area of code:

```
fast_search("payment processing")     → round trip 1, agent interprets
deep_dive("process_payment")          → round trip 2, agent interprets
fast_refs("PaymentResult")            → round trip 3, agent interprets
get_symbols("src/payment/mod.rs")     → round trip 4, agent interprets
```

Each round trip costs ~2-5 seconds and requires the agent to decide the next step. The agent may go down wrong paths, miss important connections, or over-fetch irrelevant code.

`get_context` replaces this with one call that leverages server-side knowledge of the code graph to make better decisions about what context to return.

## Design

### Tool Interface

```rust
pub struct GetContextTool {
    /// Search query — concept, task description, or symbol pattern
    pub query: String,

    /// Maximum tokens to return (optional, default: adaptive)
    /// When omitted, the tool auto-selects based on result count:
    ///   1-2 pivots → ~2000 tokens (deep context)
    ///   3-5 pivots → ~3000 tokens (balanced)
    ///   6+  pivots → ~4000 tokens (broad overview)
    pub max_tokens: Option<u32>,

    /// Workspace filter (default: "primary")
    pub workspace: Option<String>,

    /// Language filter (optional)
    pub language: Option<String>,

    /// File pattern filter (optional, glob syntax)
    pub file_pattern: Option<String>,
}
```

### Pipeline

#### Step 1: Search

Run `search_symbols` with the query (now with OR fallback from Phase 1).

- Fetch `limit * 3` candidates (overfetch for reranking)
- Apply language and file_pattern filters

#### Step 2: Centrality-Aware Pivot Selection

Rerank candidates using combined score:

```
combined_score = text_relevance * (1.0 + ln(1 + reference_score) * CENTRALITY_WEIGHT)
```

Select top N as "pivot" symbols. N is determined by the score distribution:
- If top result is 2x+ above second → 1 pivot (clear winner)
- If top 3 are within 30% of each other → 3 pivots (cluster)
- Otherwise → 2 pivots

#### Step 3: Graph Expansion

For each pivot symbol:
1. Fetch incoming relationships (callers, implementors, importers)
2. Fetch outgoing relationships (callees, types used, modules imported)
3. Fetch children (for containers: class methods, trait functions, module exports)

Deduplicate neighbors across pivots. Score neighbors by their own `reference_score` to identify the most important connections.

#### Step 4: Adaptive Token Allocation

The token budget is split between pivots and neighbors:

```
Budget allocation:
  Pivots:    60% of budget → full code bodies
  Neighbors: 30% of budget → signatures + doc comments
  Summary:   10% of budget → graph overview, file map
```

**Adaptive behavior based on result count:**

| Pivots Found | Pivot Treatment | Neighbor Treatment |
|---|---|---|
| 1 | Full code body (up to budget) | Signatures + doc comments + 1-line context |
| 2-3 | Full code body (shared budget) | Signatures only |
| 4-6 | Signature + key lines (heuristic: first/last 5 lines) | Names + file locations |
| 7+ | Signatures only | Names only (list format) |

**Token tracking:** Use `TokenEstimator` to count tokens as content is added. Stop adding content when budget is reached. Prioritize by: pivots first (highest centrality), then neighbors (by centrality), then summary.

#### Step 5: Format Output

```
═══ Context: "payment processing" ═══
Found 2 pivot symbols, 8 neighbors across 4 files

── Pivot: process_payment ──────────────────────
src/payment/processor.rs:42 (function, public)
  Centrality: high (ref_score: 47)

  pub async fn process_payment(order: &Order, method: PaymentMethod) -> Result<Receipt> {
      let validated = validate_payment(&order, &method)?;
      let charge_result = gateway.charge(validated).await?;
      let receipt = Receipt::from_charge(charge_result);
      audit_log::record_payment(&receipt);
      Ok(receipt)
  }

  Callers (5): checkout_handler, retry_payment, batch_processor, ...
  Calls: validate_payment, gateway.charge, Receipt::from_charge, audit_log::record_payment

── Pivot: PaymentMethod ────────────────────────
src/payment/types.rs:15 (enum, public)
  Centrality: medium (ref_score: 23)

  pub enum PaymentMethod {
      CreditCard { number: String, expiry: NaiveDate },
      BankTransfer { routing: String, account: String },
      Wallet { provider: WalletProvider, token: String },
  }

── Neighbors ───────────────────────────────────
  validate_payment    src/payment/validation.rs:10   fn validate_payment(order: &Order, method: &PaymentMethod) -> Result<ValidatedPayment>
  Receipt             src/payment/types.rs:35        pub struct Receipt { id: Uuid, amount: Decimal, timestamp: DateTime<Utc> }
  checkout_handler    src/api/checkout.rs:28         pub async fn checkout_handler(req: CheckoutRequest) -> Response
  gateway.charge      src/payment/gateway.rs:55      pub async fn charge(&self, payment: ValidatedPayment) -> Result<ChargeResult>
  audit_log::record   src/audit/mod.rs:12            pub fn record_payment(receipt: &Receipt)
  ...

── Files ───────────────────────────────────────
  src/payment/processor.rs   (pivot: process_payment)
  src/payment/types.rs       (pivot: PaymentMethod, neighbor: Receipt)
  src/payment/validation.rs  (neighbor: validate_payment)
  src/payment/gateway.rs     (neighbor: gateway.charge)
  src/api/checkout.rs        (neighbor: checkout_handler)
```

### Relationship to Existing Tools

| Tool | Purpose | Starting Point | When to Use |
|---|---|---|---|
| `fast_search` | Find symbols by text | Query string | "Where is X defined?" |
| `deep_dive` | Understand one symbol | Symbol name | "Tell me about X before I modify it" |
| `fast_refs` | Impact analysis | Symbol name | "Who uses X?" |
| `get_context` | Understand an area | Concept/task | "I need to work on payment processing" |

`get_context` is the "start of task" tool. The others are used during the task for specific operations.

## Implementation Steps

### Step 1: Tool scaffolding
1. Create `src/tools/get_context/mod.rs` with tool struct and parameter parsing
2. Create `src/tools/get_context/pipeline.rs` for the search → rank → expand → format pipeline
3. Create `src/tools/get_context/allocation.rs` for adaptive token budgeting
4. Register tool in MCP handler
5. Write basic integration test: tool accepts query, returns non-empty output

### Step 2: Search + pivot selection
1. Write failing test: given indexed codebase, query returns ranked pivots with centrality influence
2. Implement search phase (reuse `search_symbols` from search module)
3. Implement pivot selection logic (score distribution analysis)
4. Pass test

### Step 3: Graph expansion
1. Write failing test: pivots expand to include callers, callees, children
2. Implement graph expansion (reuse relationship queries from database module)
3. Implement neighbor deduplication and ranking
4. Pass test

### Step 4: Adaptive token allocation
1. Write failing test: single pivot gets deep treatment, many pivots get broad treatment
2. Implement budget allocation engine using TokenEstimator
3. Implement progressive content filling (pivots → neighbors → summary)
4. Write test: output stays within max_tokens when specified
5. Pass tests

### Step 5: Output formatting
1. Write failing test: output contains expected sections (pivots, neighbors, files)
2. Implement formatter with sections as shown in design
3. Include centrality hint (high/medium/low based on score percentile)
4. Pass test

### Step 6: Integration testing
1. Build debug binary
2. Test against Julie's own codebase with real queries
3. Test queries: "search ranking", "file watcher", "workspace management"
4. Verify output quality — are the right symbols surfaced? Is the context useful?
5. Compare: how many round trips does the agent save?

### Step 7: Agent instructions
1. Update `JULIE_AGENT_INSTRUCTIONS` to include `get_context` tool
2. Add usage guidance: use at start of task for orientation, switch to specific tools for modification
3. Add to MCP tool descriptions

## Success Criteria

- [ ] Single tool call returns relevant context for concept-level queries
- [ ] Adaptive allocation: few results → deep, many results → broad
- [ ] Token budget respected when max_tokens is specified
- [ ] Centrality influences pivot selection (well-connected symbols preferred)
- [ ] OR fallback ensures queries return results even for natural language input
- [ ] Output format is immediately useful to agents (code bodies, signatures, file map)
- [ ] No overlap with deep_dive's purpose (get_context is area-level, deep_dive is symbol-level)
- [ ] File organization follows project standards (≤500 lines per file, tests in src/tests/)

## Risk Assessment

**Medium risk.** This is a new tool — the same category as `fast_explore` which was removed for being underused. Mitigations:

1. **Built on proven infrastructure** — uses existing search, relationships, token estimation. No new indexes or storage.
2. **Addresses a real agent workflow** — the 3-4 tool chain is a measured pain point, not a hypothetical.
3. **Centrality-aware ranking** — this is the key differentiator from `fast_explore`. The old tool didn't have graph intelligence to rank results. `get_context` does.
4. **Adaptive allocation** — the old tool had static output. Adaptive allocation means the output shape matches the query, which should improve agent satisfaction.
5. **Easy to remove** — if it doesn't get used, it's a self-contained module with no tentacles into other tools.

**Biggest risk:** The agent (Claude) might still prefer manual orchestration of fast_search → deep_dive because it maintains more control. Mitigation: make `get_context` output rich enough that the agent rarely needs follow-up calls. If we find agents still calling deep_dive immediately after get_context, that's a signal to improve get_context's output, not to keep both.
