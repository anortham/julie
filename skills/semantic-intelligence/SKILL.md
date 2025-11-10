---
name: semantic-intelligence
description: Use Julie's semantic search capabilities for conceptual code understanding. Activates when searching for concepts, cross-language patterns, business logic, or exploring unfamiliar code. Combines text and semantic search for optimal results.
allowed-tools: mcp__julie__fast_search, mcp__julie__find_logic, mcp__julie__trace_call_path, mcp__julie__fast_goto, mcp__julie__get_symbols
---

# Semantic Intelligence Skill

## Purpose
Leverage Julie's **semantic understanding** to find code by concept, not just keywords. Goes beyond text matching to understand what code does.

## When to Activate
- Searching for concepts ("authentication logic")
- Finding cross-language patterns
- Discovering business logic
- Exploring unfamiliar codebases
- When text search returns nothing
- Understanding execution flows

## Semantic vs Text Search

**Text Search** (exact/wildcard matching):
```
fast_search({ query: "console.error", mode: "text" })
→ Fast (<10ms)
→ Exact matches only
→ Misses variations ("logger.error", "print_error")
```

**Semantic Search** (conceptual understanding):
```
fast_search({ query: "error logging", mode: "semantic" })
→ Slower (~100ms)
→ Finds concepts
→ Discovers: console.error, logger.error, logging.error, errorHandler
→ Cross-language: Python logging, Rust tracing, Go log
```

**Hybrid Search** (best of both):
```
fast_search({ query: "authentication", mode: "hybrid" })
→ Runs text + semantic in parallel
→ Fuses results intelligently
→ Boosts symbols in BOTH searches
→ Optimal: ~150ms
```

---

## When to Use Each Mode

### Use Text Mode When:
- ✅ Searching for specific API names
- ✅ Finding exact strings
- ✅ Speed critical (<10ms)
- ✅ You know exact symbol name

```
Examples:
- "getUserData" → find specific function
- "console.log" → find exact API usage
- "import React" → exact import statement
- "TODO: fix" → exact comment
```

### Use Semantic Mode When:
- ✅ Searching for concepts
- ✅ Cross-language patterns
- ✅ Don't know exact names
- ✅ Understanding what code does

```
Examples:
- "authentication logic" → find ALL auth-related code
- "error handling" → discover error patterns
- "database connections" → find DB code (MySQL, Postgres, etc.)
- "payment processing" → business logic discovery
```

### Use Hybrid Mode When:
- ✅ Not sure which mode is best
- ✅ Want comprehensive results
- ✅ Concept + exact matches both useful
- ✅ Willing to wait ~150ms

```
Examples:
- "user authentication" → concept + exact matches
- "API endpoints" → finds routes, handlers, controllers
- "validation logic" → semantic concept + exact validators
```

---

## Search Strategy Decision Tree

```
Know exact symbol name?
  YES → fast_goto("SymbolName")
        → <5ms, jumps to definition

Know exact API/string?
  YES → fast_search({ mode: "text" })
        → <10ms, exact matches

Searching for concept/behavior?
  YES → fast_search({ mode: "semantic" })
        → ~100ms, conceptual understanding

Not sure / want comprehensive?
  YES → fast_search({ mode: "hybrid" })
        → ~150ms, text + semantic fused

Looking for business logic specifically?
  YES → find_logic({ domain: "..." })
        → Filters framework noise, semantic tier included
```

---

## Cross-Language Semantic Matching

**Julie's Superpower:** Finds similar code across languages

```
Example: Finding "user validation" across codebase

fast_search({
  query: "user input validation",
  mode: "semantic",
  limit: 20
})

Results discovered:
- TypeScript: validateUser(input: UserInput)
- Python: def validate_user_input(data: dict)
- Rust: fn validate_user(user: &User) -> Result
- Go: func ValidateUserInput(input *UserInput) error
- Java: public boolean validateUser(User user)

→ Same CONCEPT, different languages
→ Naming variants automatically understood
→ Semantic embeddings capture meaning
```

**Why this works:** Embeddings encode *what code does*, not just *what it's called*

---

## Business Logic Discovery

**Problem:** Framework code dominates search results

**Solution:** `find_logic` filters noise, finds actual business logic

```
find_logic({
  domain: "payment",
  max_results: 20,
  min_business_score: 0.3
})

Filtering strategy:
1. Text patterns (business domain keywords)
2. Symbol scoring (complexity, public visibility)
3. Path scoring (src/ over node_modules/)
4. **Semantic tier** (conceptual similarity to domain)

→ Returns business logic, not framework boilerplate
→ Grouped by layer (controllers, services, models)
```

**Common domains:**
- "payment" → payment processing logic
- "auth" → authentication/authorization
- "user" → user management
- "order" → order processing
- "notification" → notification systems

---

## Semantic Search on Memories

**Memories are searchable just like code:**

```
# Find similar past decisions
fast_search({
  query: "database choice rationale",
  mode: "semantic",
  file_pattern: ".memories/**/*.json"
})

→ Discovers decision memories about DB selection
→ Finds learnings about database migrations
→ Cross-references related checkpoints

# Find bug fixes related to concept
fast_search({
  query: "race condition fixes",
  mode: "semantic",
  file_pattern: ".memories/**/*.json"
})

→ Returns past race condition bugs
→ Shows solutions that worked
→ Prevents repeating same debugging
```

**Power move:** Search codebase + memories together for complete understanding

---

## Execution Flow Tracing (Cross-Language)

**Unique capability:** Trace calls across language boundaries

```
trace_call_path({
  symbol: "processPayment",
  direction: "downstream",  // what does this call?
  max_depth: 3
})

Execution flow discovered:
TypeScript: processPayment()
  → Rust: payment_processor::process() ← CROSSES LANGUAGE
    → SQL: stored_proc_charge() ← CROSSES AGAIN

→ Semantic matching finds cross-language connections
→ No other tool does this
→ ~200ms for multi-level traces
```

**Directions:**
- `upstream` → Who calls this? (callers)
- `downstream` → What does this call? (callees)
- `both` → Full call graph

---

## Hybrid Search Intelligence

**How hybrid mode works:**

```
1. Run text and semantic searches IN PARALLEL
2. Collect results from both
3. Score fusion algorithm:
   - Symbols in BOTH searches → boosted score
   - Text-only → normal score
   - Semantic-only → normal score
4. Sort by fused score
5. Return top results

Performance: ~150ms (parallelized)
```

**When hybrid shines:**
```
Query: "API error handling"

Text search finds:
- handleAPIError()
- api_error_handler.ts
- APIErrorHandler class

Semantic search finds:
- tryRequest() // catches errors
- errorBoundary() // React error handling
- logFailedRequest() // implicit error handling

Hybrid fusion:
- handleAPIError() ← BOTH searches (boosted!)
- tryRequest() ← semantic understanding
- api_error_handler.ts ← text match
→ Comprehensive results, best ranked first
```

---

## Semantic Reference Discovery

**Find semantically similar code:**

```
# After finding a symbol with fast_goto
fast_refs({
  symbol: "UserService",
  include_definition: true
})

→ Returns ALL references (text-based)

# Semantic alternative (conceptual similarity)
fast_search({
  query: "UserService class implementation",
  mode: "semantic",
  search_target: "definitions"
})

→ Finds UserService + similar patterns:
  - AccountService (similar structure)
  - ProfileService (similar purpose)
  - CustomerService (semantic similarity)
```

**Use case:** "Find all classes similar to this pattern"

---

## Workflow Patterns

### Pattern 1: Unfamiliar Codebase

```
Step 1: Broad semantic search
  fast_search({ query: "main entry point", mode: "semantic" })

Step 2: Find business logic
  find_logic({ domain: "core", max_results: 30 })

Step 3: Trace execution
  trace_call_path({ symbol: "main", direction: "downstream", max_depth: 2 })

Step 4: Get structure
  get_symbols({ file_path: "main.ts", max_depth: 2 })

→ Understand codebase in <5 minutes
```

### Pattern 2: "How is X implemented?"

```
Step 1: Semantic search
  fast_search({ query: "authentication implementation", mode: "semantic" })

Step 2: Find exact symbols
  fast_goto("authenticate")

Step 3: See usage
  fast_refs({ symbol: "authenticate" })

Step 4: Trace flow
  trace_call_path({ symbol: "authenticate", direction: "downstream" })

→ Complete understanding of feature
```

### Pattern 3: Cross-Language Investigation

```
Step 1: Semantic search (all languages)
  fast_search({ query: "user validation logic", mode: "semantic" })

Step 2: Review results
  → TypeScript, Python, Rust implementations

Step 3: Compare patterns
  → See how each language handles validation
  → Identify best practices

Step 4: Trace connections
  trace_call_path({ symbol: "validateUser", direction: "both" })
  → Understand cross-service calls
```

---

## Key Behaviors

### ✅ DO
- Use semantic search for concepts and behaviors
- Use text search for exact API/symbol names
- Use hybrid when uncertain (comprehensive)
- Search memories semantically (find past learnings)
- Use find_logic to discover business code
- Trace execution flows cross-language
- Combine multiple search modes for completeness

### ❌ DON'T
- Use semantic for exact symbol names (use fast_goto)
- Use text search for concepts (misses variations)
- Ignore hybrid mode (often best choice)
- Forget to search memories (knowledge base!)
- Skip cross-language tracing (unique capability)
- Search without strategy (use decision tree)

---

## Success Criteria

This skill succeeds when:
- ✅ Concepts found across languages
- ✅ Business logic discovered efficiently
- ✅ Execution flows traced completely
- ✅ Memories searched semantically
- ✅ Right search mode used for query type
- ✅ Cross-language patterns identified
- ✅ Unfamiliar code understood quickly

---

## Performance

- **Text search**: <10ms
- **Semantic search**: ~100ms
- **Hybrid search**: ~150ms
- **find_logic**: ~200ms (includes semantic tier)
- **trace_call_path**: ~200ms (cross-language)

---

**Remember:** Semantic understanding is Julie's superpower. Text finds what you ask for. Semantic finds what you mean.

**The Rule:** Searching for WHAT (exact) → text. Searching for WHY (concept) → semantic. Not sure → hybrid.
