# Semantic Enhancements to Existing Tools

## Problem

Julie has a stable embedding pipeline (ORT/DirectML on Windows, fastembed elsewhere) that
produces per-symbol vectors. These vectors power hybrid search in `fast_search` (definitions)
and `get_context`, plus a "similar symbols" section in `deep_dive` at full depth.

But embeddings are underutilized. The cross-language bridging capability — finding that
`IUser` (TypeScript), `UserDto` (C#), `ApplicationUser` (domain), and `Users` (SQL) are
all "about the same thing" — only surfaces when someone explicitly requests `deep_dive`
at full depth. That's a narrow window for a powerful capability.

## Design Principle

**Enhance existing tools, don't add new ones.** Julie has 7 tools. Each one gets used.
Adding a tool that doesn't get reached for regularly is worse than no tool — it bloats
the tool descriptions and wastes tokens on every MCP handshake. Semantic awareness should
surface organically in tools agents already call.

**Every token must earn its place.** Semantic results should improve quality, not add bloat.
If the similar symbols section doesn't add signal, it shouldn't appear.

## Changes

### 1. `deep_dive` — show similar symbols at "context" depth

**Current behavior:** `build_similar` only runs at `depth == "full"`.

**New behavior:** Run `build_similar` at `depth == "context"` too. The similar section
adds ~50 tokens (5 entries, one line each) on top of context depth's ~600 token output.

**Rationale:** Context depth is the "I'm about to modify this symbol" call — the moment
when seeing cross-language relatives is most useful. An agent investigating `UserDto`
before changing it would automatically see `IUser` and `ApplicationUser` as related.

**Why NOT at overview depth:** Overview is ~200 tokens for quick orientation. Adding ~50
tokens of similar symbols would bloat it by 25% for a feature that's not relevant during
quick lookups.

**Distance threshold:** Add a minimum similarity score of 0.5 (score = 1.0 - distance)
to `build_similar`. Currently it returns whatever KNN gives back, including garbage
matches. Since we're now showing similar symbols more frequently (context + full instead
of just full), filtering low-quality matches matters more. This threshold applies to
both context and full depth.

**Implementation:**
- `src/tools/deep_dive/data.rs:205-210` — change gate from `depth == "full"` to
  `depth == "full" || depth == "context"`
- `src/tools/deep_dive/data.rs` `build_similar()` — add distance threshold filter
  after KNN results, before returning. Filter out entries with score < 0.5.
- Update comment in `src/tools/deep_dive/formatting.rs:43` — currently says
  "full depth only", update to "context and full depth"
- Update test `test_similar_symbols_not_at_context_depth` — it currently asserts
  context depth has NO similar symbols; update to assert it DOES
- Add test: verify overview depth still excludes similar symbols
- Add test: verify distance threshold filters low-quality matches
- Performance cost: one `get_embedding` + one `knn_search` (~1-2ms total), negligible

### 2. `fast_refs` — semantic fallback on zero references

**Current behavior:** When no references found, returns:
```
No references found for "X"
💡 Check spelling, or try fast_search(query="X", search_target="definitions") to verify the symbol exists
```

**New behavior:** When no references found AND the symbol has a stored embedding,
run KNN search and show semantically similar symbols. The existing recovery hint
is replaced by actionable cross-language discovery.

**When NOT to show semantic results:**
- When exact references exist (don't dilute precise results with approximate matches)
- When no embedding exists for the symbol (graceful degradation, no error)
- When all similar symbols have score < 0.5 (don't show garbage matches)
- Reference workspace queries — skip semantic fallback entirely (reference workspaces
  have separate DB files and may not have embeddings; keep it simple for now)

**Output format:**
```
No references found for "IUser"

Related symbols (semantic):
  UserDto                   0.82  src/api/models.cs:45 (class, public)
  ApplicationUser           0.78  src/domain/user.cs:12 (class, public)
  Users                     0.71  db/tables.sql:8 (table)

💡 These are semantically similar, not exact references
```

**Reuse strategy:** Extract the KNN lookup logic into a shared function in
`src/search/similarity.rs` (new file) that both `deep_dive/data.rs::build_similar`
and the `fast_refs` fallback can call. The shared function takes `(&SymbolDatabase,
&Symbol or symbol_name) -> Vec<SimilarEntry>` and handles embedding lookup, KNN,
self-filtering, and distance thresholding.

**Implementation:**
- Create `src/search/similarity.rs` — shared `find_similar_symbols()` function
- Refactor `deep_dive/data.rs::build_similar` to delegate to the shared function
- `src/tools/navigation/fast_refs.rs` — in the `call_tool` method, detect zero-ref
  result, call shared similarity function, append to output
- The `fast_refs` handler already has access to `JulieServerHandler` which provides
  `get_workspace()` → `db` and `embedding_provider`. Thread these into the zero-ref path.
- `src/tools/navigation/formatting.rs` — add `format_semantic_fallback()` function
- Cap at 5 results, distance threshold 0.5

### 3. `get_context` — validate cross-language quality (investigation, not code)

`get_context` already uses `hybrid_search` with RRF merge. A query like
`"user entity across the stack"` should leverage embeddings to find cross-language
matches. This needs validation on a real polyglot workspace, not new code.

**Action:** Run `get_context` queries against a polyglot reference workspace and assess
whether the hybrid search surfaces cross-language results. If not, investigate whether
the issue is in embedding quality, RRF weighting, or the `is_nl_like_query` gate.

## Non-goals

- No new MCP tools (agents don't reach for dedicated similarity tools)
- No changes to `is_nl_like_query` — it correctly distinguishes symbol lookups from NL
- No changes to the embedding pipeline itself (what gets embedded, how)
- No similar symbols at `deep_dive` overview depth (token budget doesn't justify it)
- No semantic fallback for reference workspace queries in `fast_refs` (simplicity first)

## Testing

### deep_dive
- Update `test_similar_symbols_not_at_context_depth` → assert context depth DOES include similar
- Add test: overview depth still excludes similar symbols
- Add test: distance threshold filters entries with score < 0.5

### fast_refs
- Zero refs with embeddings → semantic fallback shown (above threshold)
- Zero refs with embeddings but all below threshold → no semantic section
- Zero refs without embeddings → graceful degradation (no crash, shows recovery hint)
- Refs exist → no semantic section (don't dilute)
- Reference workspace zero refs → no semantic fallback (skip for now)

### Integration
- Validate on julie's own codebase: `deep_dive(symbol="EmbeddingProvider", depth="context")`
  should show related provider/embedding types

## Files (expected)

- `src/search/similarity.rs` — **new**: shared `find_similar_symbols()` function
- `src/search/mod.rs` — add `pub mod similarity;`
- `src/tools/deep_dive/data.rs` — lower the `build_similar` gate, delegate to shared fn
- `src/tools/deep_dive/formatting.rs` — update comment only
- `src/tools/navigation/fast_refs.rs` — add semantic fallback path
- `src/tools/navigation/formatting.rs` — add `format_semantic_fallback()` function
- `src/tests/tools/deep_dive_tests.rs` — update/add tests
- `src/tests/tools/fast_refs_tests.rs` or similar — add tests
