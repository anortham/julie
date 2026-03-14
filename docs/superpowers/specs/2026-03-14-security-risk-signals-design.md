# Security Risk Signals — Structural Security Analysis

## Goal

Give AI agents per-symbol **security risk** scores in `deep_dive` and `get_context` output. An agent investigating `process_request()` should immediately see: "Security Risk: HIGH (0.85) — calls execute, raw_sql; public; accepts string params" without any extra tool calls.

## Scope

This spec covers structural security risk signals — static, index-time analysis of symbol properties, parameter types, and direct callee relationships. It does NOT cover runtime analysis, taint tracking, or data flow across multiple statements.

Security risk is **separate from change risk** (implemented in Phase 2). They answer different questions:
- **Change risk:** "How dangerous is it to modify this code?" (centrality + test coverage)
- **Security risk:** "Does this code have structural security concerns?" (exposure + sinks + input handling)

A function can be low change risk (private, well-tested) but high security risk (calls `exec` with string params).

## Prerequisites (Complete)

- `reference_score` (graph centrality) computed for all symbols
- `test_coverage` metadata (Layer C) — used for "untested" signal
- `change_risk` metadata (Layer D) — coexists, not replaced
- `relationships` table with Calls edges
- `identifiers` table with Call kind entries
- `signature` field on symbols with parameter/return type info

---

## Security Signals

Five structural signals, each normalized to 0.0–1.0:

| Signal | Weight | Detection method | What it means |
|--------|--------|-----------------|---------------|
| **Exposure** | 0.25 | `visibility` + `kind` | Public callable = highest exposure |
| **Input handling** | 0.25 | Regex on `signature` for untrusted-data parameter types | Accepts potentially untrusted data |
| **Sink calls** | 0.30 | One-hop: identifiers (Call kind) + relationships (Calls) matching sink patterns | Directly calls dangerous functions |
| **Blast radius** | 0.10 | `reference_score` (P95 log sigmoid, same normalization as change risk) | How many things depend on this |
| **Untested** | 0.10 | `test_coverage` metadata | No test safety net for security-critical code |

### Exposure signal (0.25)

Reuses the same `visibility_score()` from `change_risk.rs` but uses a security-specific kind weight (containers and data are less relevant for security than for change risk):

```
exposure = visibility_score * security_kind_weight
```

Where:
- `visibility_score`: public = 1.0, protected = 0.5, private = 0.2, NULL = 0.5 (reuse from change_risk)
- `security_kind_weight`: callable (Function/Method/Constructor/etc.) = 1.0, container = 0.3, data = 0.1
- Import/Export excluded from scoring entirely (returns None)

The lower container/data weights reflect that security risk is primarily about callable code that handles input and calls sinks, not data structures.

### Input handling signal (0.25)

Regex on the symbol's `signature` string to detect parameter types suggesting untrusted input. Language-agnostic patterns:

```
// Web request types
Request, HttpRequest, HttpServletRequest, ActionContext,
req:, request:, ctx:

// Query/form/body parameter types
Query, Form, Body, Params, FormData, MultipartFile,
QueryString, RouteParams

// Raw string/byte types (in parameter position)
// Only match when in parameter list, not return type
&str, String, string, str, bytes, []byte, InputStream,
ByteArray, Vec<u8>, &[u8]
```

**Score:** 1.0 if any pattern matches in the parameter portion, 0.0 otherwise. Binary signal.

**Implementation note:** To avoid false positives on return types (e.g., `fn get_user() -> String`), split the signature at the return type delimiter before matching. Heuristic: find the last `->` (Rust), `:` after `)` (TypeScript/Python), or `returns` keyword, and only match patterns in the text BEFORE that delimiter. If no delimiter is found, match the full signature (many languages don't have explicit return type syntax in signatures). This is imperfect but catches the majority of cases.

### Sink calls signal (0.30)

One-hop detection: query identifiers (Call kind) and relationships (Calls kind) whose target name matches a known dangerous sink pattern.

**Category A — Command/code execution:**
```
exec, eval, system, popen, spawn, fork, shell_exec,
subprocess, Process.Start, Runtime.exec, os.system,
child_process, ShellExecute, CreateProcess
```

**Category B — Database/query operations:**
```
execute, raw_sql, exec_query, executeQuery, executeUpdate,
cursor.execute, rawQuery, RunSQL, db.Exec, db.Query
```

Note: `prepare`, `query`, `sql`, and `raw` are intentionally excluded — they're too common in safe abstractions (query builders, ORMs, test helpers) and produce excessive false positives.

**Matching strategy:** Split the identifier/symbol name by `::` and `.` separators, then **case-insensitive exact-match the final segment** against the sink list. Case-insensitive matching means `db.Exec` matches `exec` and `Process.Start` is redundant (covered by final segment `start` — but `start` is too generic, so keep compound patterns for precision). Examples:
- `db.execute` → final segment `execute` → **matches**
- `cursor.execute` → final segment `execute` → **matches**
- `execution_context` → final segment `execution_context` → **no match**
- `SymbolDatabase::new` → final segment `new` → **no match**
- `os.system` → final segment `system` → **matches**

For multi-segment sink patterns like `Process.Start`, match the last N segments: `Process.Start` matches when the last two segments are `Process` and `Start`.

**Detection algorithm — batch approach:**

Pre-load all relevant data once, then match in-memory (avoids O(N) per-symbol queries):

1. **Pre-load identifiers:** Query all identifiers where `kind = 'call'`, grouped by `containing_symbol_id` into a `HashMap<String, Vec<String>>` (symbol_id → list of callee names). This requires a new query method `get_call_identifiers_grouped()` on `SymbolDatabase` (see file structure).

2. **Pre-load relationship callees:** Query all relationships where `kind = 'calls'`, JOIN to symbols table to get the callee name, grouped by `from_symbol_id` into a `HashMap<String, Vec<String>>`.

```sql
SELECT r.from_symbol_id, s_callee.name
FROM relationships r
JOIN symbols s_callee ON r.to_symbol_id = s_callee.id
WHERE r.kind = 'calls'
```

3. **Per-symbol matching:** For each symbol being scored, look up its callee names from both HashMaps, apply the final-segment matching strategy against the sink pattern list.

4. Deduplicate matched sink names.
5. Score: 0.0 if no sinks found, 0.7 if one sink, 1.0 if multiple sinks.
6. Store detected sink names (capped at 5) for display.

### Blast radius signal (0.10)

Reuse the same P95 log sigmoid normalization from change risk:

```
blast_radius = min(1.0, ln(1.0 + reference_score) / ln(1.0 + P95))
```

P95 computed once at the start of `compute_security_risk()`. Can reuse the same P95 value if change risk already computed it in the same pipeline run, but computing independently is simpler and the cost is one SQL query.

### Untested signal (0.10)

Binary: 1.0 if no `test_coverage` key in metadata (untested), 0.0 if tested.

Unlike change risk which uses a graduated scale (thorough → stub), security risk treats "any test coverage" as the threshold. The reasoning: for security-critical code, even thin tests that exercise the code path are meaningfully better than zero tests.

---

## Scoring

**Formula:**
```
security_risk = 0.25 * exposure + 0.25 * input_handling + 0.30 * sink_calls + 0.10 * blast_radius + 0.10 * untested
```

**Tier labels:**

| Score | Label |
|-------|-------|
| >= 0.7 | HIGH |
| >= 0.4 | MEDIUM |
| < 0.4 | LOW |

**Scoring gate:** Only symbols where at least one of these is true get scored:
- `exposure >= 0.5` (public/protected callable)
- `input_handling > 0` (accepts untrusted-looking params)
- `sink_calls > 0` (calls a dangerous function)

Symbols that don't trigger any gate get no `security_risk` key in metadata (absence = no security concerns detected).

---

## Storage

In the symbol's existing `metadata` JSON column:

```json
{
  "security_risk": {
    "score": 0.85,
    "label": "HIGH",
    "signals": {
      "exposure": 1.0,
      "input_handling": 1.0,
      "sink_calls": ["execute", "raw_sql"],
      "blast_radius": 0.60,
      "untested": true
    }
  }
}
```

`sink_calls` stores detected sink names (capped at 5) rather than a numeric score, giving the agent actionable context.

No schema migrations. No new tables.

---

## Tool Integration

### `deep_dive` — security risk section

Shown after the change risk section, only when `security_risk` key exists in metadata. Skipped for test symbols.

```
Security Risk: HIGH (0.85) — calls execute, raw_sql; public; accepts string params
  exposure: public function
  input handling: signature contains String, Request params
  sink calls: execute, raw_sql
  blast radius: 0.60 (8 callers)
  untested: yes
```

**Implementation:** New `format_security_risk_info()` in `src/tools/deep_dive/formatting.rs`. Wire into the kind-specific formatter call sites only (after `format_change_risk_info` in `format_callable`, `format_class_or_struct`, etc.) — NOT in `format_header`, to avoid double-rendering. Self-skips when no `security_risk` key in metadata.

### `get_context` — security label on pivots

Append security label after the existing risk label, only when present:

```
PIVOT process_request src/handler.rs:42 kind=function centrality=high risk=MEDIUM security=HIGH
```

**Implementation:** Add `pub security_label: Option<String>` to `PivotEntry`. Extract from `batch.full_symbols` metadata in pipeline.rs. Append to **both** compact format (line ~216) and readable format (line ~144) — both formatting paths must be updated.

In `SignatureOnly` mode, `full_symbols` is empty — security labels will be absent, which is acceptable.

### What does NOT change

- `fast_search` — no security labels in search results
- `fast_refs` — references are factual, not risk-assessed
- `rename_symbol` — no security labels
- `change_risk` — independent, untouched

---

## Pipeline Order

Extended from current indexing pipeline:

```
Extract & Store
  -> Resolve Relationships
  -> compute_reference_scores()          [existing]
  -> compute_test_quality_metrics()      [existing]
  -> compute_test_coverage()             [existing - Layer C]
  -> compute_change_risk_scores()        [existing - Layer D]
  -> compute_security_risk()             [NEW]
```

Runs last because it reads `test_coverage` (untested signal) and `reference_score` (blast radius).

Hook point: `src/tools/workspace/indexing/processor.rs`, after `compute_change_risk_scores()`.

---

## File Structure

| File | Change | Est. lines |
|------|--------|-----------|
| `src/analysis/mod.rs` | Add `pub mod security_risk;` + re-export | ~2 |
| `src/analysis/security_risk.rs` | **NEW** — signals, patterns, `compute_security_risk()` | ~300 |
| `src/database/identifiers.rs` | Add `get_call_identifiers_grouped()` query method | ~20 |
| `src/tools/workspace/indexing/processor.rs` | Hook after `compute_change_risk_scores()` | ~4 |
| `src/tools/deep_dive/formatting.rs` | Add `format_security_risk_info()`, wire into call sites | ~50 |
| `src/tools/get_context/formatting.rs` | Add `security_label` to `PivotEntry`, append to both formats | ~10 |
| `src/tools/get_context/pipeline.rs` | Extract `security_label` from metadata | ~8 |
| `src/tests/analysis/mod.rs` | Add `pub mod security_risk_tests;` | ~1 |
| `src/tests/analysis/security_risk_tests.rs` | **NEW** — signal detection + scoring tests | ~300 |
| `src/tests/tools/get_context_formatting_tests.rs` | Add `security_label: None` to `make_pivot` helper | ~1 |

No schema migrations. No new tables. All storage in existing `metadata` JSON column.

### What does NOT change

- Extractors — no modifications needed
- Database schema — no migrations
- `test_detection.rs`, `test_quality.rs`, `test_coverage.rs`, `change_risk.rs` — untouched
- `fast_search` — no security labels
- `fast_refs` — no security labels

---

## Testing Strategy

### Unit tests (security_risk_tests.rs)

**Signal detection:**
- Test exposure scoring for each visibility + kind combination
- Test input handling regex against signatures from multiple languages (Rust, Python, Java, TypeScript, Go, C#)
- Test sink detection via identifiers (Call kind matching exec/query patterns)
- Test sink detection via relationships (Calls to symbols named execute/query)
- Test blast radius uses same P95 normalization as change risk
- Test untested signal: binary (no test_coverage = 1.0, any coverage = 0.0)

**Scoring:**
- Test formula produces expected scores for known signal combinations
- Test tier boundaries (0.4, 0.7)
- Test scoring gate: symbols with no signals get no `security_risk` key
- Test that test symbols are excluded

**Sink patterns:**
- Test each Category A pattern (exec, eval, system, etc.)
- Test each Category B pattern (execute, query, raw_sql, etc.)
- Test that non-sink names don't match (e.g., "execution_context", "query_builder" should NOT match as sinks — pattern must match function call names, not substrings)

### Integration tests

- Run `compute_security_risk()` on Julie fixture → verify symbols that call `db.conn.execute` get security risk scores
- Call `deep_dive` on a function with security signals → verify section appears
- Call `get_context` → verify pivot lines include `security=` label when applicable

### Dogfood validation

- Build release, restart Claude Code
- `deep_dive(symbol="delete_symbols_for_file")` → should show security risk (calls execute, public)
- `get_context(query="database operations")` → pivots should show security labels
- Verify labels make intuitive sense for Julie's own codebase
