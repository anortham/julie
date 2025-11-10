---
name: explore-codebase
description: Autonomously explore unfamiliar codebases using Julie's code intelligence. Use semantic search, symbol navigation, and call path tracing to understand architecture without reading entire files. Activates when user asks to understand, explore, or learn about a codebase.
allowed-tools: mcp__julie__fast_search, mcp__julie__get_symbols, mcp__julie__fast_goto, mcp__julie__fast_refs, mcp__julie__trace_call_path, mcp__julie__find_logic, mcp__julie__fast_explore
---

# Explore Codebase Skill

## Purpose
Understand unfamiliar codebases **efficiently** using Julie's code intelligence without reading entire files. This skill uses **semantic search**, **symbol navigation**, and **execution flow tracing** to build a mental model of the code.

## When to Activate
Use when the user:
- **Wants to understand code**: "how does authentication work?", "explain the architecture"
- **Needs to find something**: "where is error handling?", "find the database layer"
- **Explores new codebase**: "I'm new to this project", "help me understand this code"
- **Investigates functionality**: "how does X feature work?", "trace this execution flow"

## Julie's Code Intelligence Tools

### 🔍 Search & Discovery

**fast_search** - Semantic + text search
```
Mode: "semantic" - Understands intent ("find authentication logic")
Mode: "lines" - Fast text search ("find all imports")
Mode: "symbols" - Symbol-only search ("class UserService")
```

**find_logic** - Business logic discovery
```
Filters out framework noise, finds actual business logic
```

**fast_explore** - Architectural exploration
```
Understand overall codebase structure and patterns
```

### 📖 Symbol Understanding

**get_symbols** - File structure overview (70-90% token savings!)
```
Mode: "structure" - High-level overview (classes, functions, imports)
Mode: "full" - Complete symbol details with relationships
Mode: "definitions" - Just the symbols (minimal)
```

**Key benefit:** See file structure WITHOUT reading entire file!

### 🧭 Navigation

**fast_goto** - Jump to definitions
```
Find where symbols are defined across the codebase
```

**fast_refs** - Find all references
```
See everywhere a symbol is used
```

### 🔗 Execution Flow Tracing

**trace_call_path** - Cross-language call graphs
```
Direction: "upstream" - What calls this? (callers)
Direction: "downstream" - What does this call? (callees)
Direction: "both" - Full call graph
```

**Unique feature:** Traces across language boundaries!

## Orchestration Strategy

### Pattern 1: Top-Down Exploration
**Goal:** Understand overall architecture

```
1. fast_explore() → Get codebase overview
2. get_symbols(mode="structure") on key files
3. trace_call_path(direction="downstream") on entry points
4. Identify patterns and layers
```

### Pattern 2: Feature Investigation
**Goal:** Understand specific feature

```
1. fast_search(query="feature name", mode="semantic")
2. get_symbols on relevant files
3. trace_call_path to understand execution flow
4. fast_refs to see all usage points
```

### Pattern 3: Bug Investigation
**Goal:** Find where something is broken

```
1. fast_search for error messages or symptoms
2. fast_goto to find definitions
3. trace_call_path(direction="upstream") to find callers
4. Analyze execution flow for root cause
```

### Pattern 4: Dependency Discovery
**Goal:** Understand what uses what

```
1. fast_refs on key symbols
2. trace_call_path(direction="both") for full graph
3. Map dependencies and relationships
```

## Example Exploration Session

```markdown
User: "How does authentication work in this codebase?"

Skill activates → Systematic exploration

Step 1: Semantic Search
→ fast_search({ query: "authentication logic", mode: "semantic" })

Results:
- src/middleware/auth.ts (score: 0.95)
- src/services/user-service.ts (score: 0.89)
- src/utils/jwt.ts (score: 0.87)

Step 2: Symbol Structure (Token-Efficient)
→ get_symbols({ file: "src/middleware/auth.ts", mode: "structure" })

Structure:
- class AuthMiddleware
  - authenticate(): middleware function
  - validateToken(): token validation
  - extractUser(): user extraction
- imports: jwt, UserService

Step 3: Trace Execution Flow
→ trace_call_path({
    symbol: "authenticate",
    direction: "downstream"
  })

Execution flow:
authenticate()
  → validateToken()
    → jwt.verify()
  → extractUser()
    → UserService.findById()

Step 4: Find Usage Points
→ fast_refs({ symbol: "authenticate" })

Used in:
- src/routes/api.ts (10 locations)
- src/routes/admin.ts (5 locations)
- src/app.ts (1 location - middleware registration)

Analysis: "Authentication uses JWT middleware that validates tokens
via the UserService. The authenticate function is registered globally
in app.ts and protects 15 routes across api and admin routers."
```

## Token Efficiency Strategy

**Traditional approach:**
```
Read entire file (500 lines) → 12,000 tokens
Analyze → Extract relevant parts
```

**Julie approach:**
```
get_symbols(mode="structure") → 800 tokens (93% savings!)
See structure → Navigate precisely
Only read specific symbols if needed
```

### When to Use What

**get_symbols** (PREFERRED):
- Understanding file structure
- Seeing available symbols
- Quick orientation
- Before deep dive

**Full file read** (SPARINGLY):
- After identifying specific target
- When understanding implementation details
- After narrowing down with symbols

## Cross-Language Navigation

**Julie's Unique Capability:** Trace calls across language boundaries

```typescript
// TypeScript
import { processPayment } from './payment-service';

processPayment(data);  // → What does this call?
```

```
→ trace_call_path({ symbol: "processPayment", direction: "downstream" })

Execution flow:
TypeScript processPayment()
  → Rust payment_processor::process()  ← CROSSES LANGUAGE BOUNDARY
    → SQL stored_procedure_charge()    ← CROSSES AGAIN
```

**No other tool does this!**

## Orchestration Examples

### Example 1: New to Codebase
```
User: "I'm new to this project, help me understand it"

1. fast_explore() → Architecture overview
2. get_symbols(file="src/main.ts", mode="structure") → Entry point
3. trace_call_path on main() → See initialization flow
4. Present: "This is a [type] application with [layers].
   Main entry point initializes [components] and starts [server]."
```

### Example 2: Find Feature
```
User: "Where is the email sending code?"

1. fast_search({ query: "send email", mode: "semantic" })
2. get_symbols on top results
3. trace_call_path to see what triggers it
4. Present: "Email sending is in [file] via [class/function],
   called from [locations]."
```

### Example 3: Understand Error
```
User: "Why am I getting 'Invalid token' errors?"

1. fast_search({ query: "Invalid token" })
2. fast_goto on token validation
3. trace_call_path(direction="upstream") → Who calls this?
4. Analyze: "Token validation happens in [middleware],
   called by [routes]. Error occurs when [condition]."
```

## Key Behaviors

### ✅ DO
- Start with semantic search for relevant code
- Use get_symbols before reading files (massive token savings)
- Trace execution paths to understand flow
- Navigate with fast_goto and fast_refs
- Build mental model incrementally
- Explain findings clearly to user

### ❌ DON'T
- Read entire files without checking symbols first
- Do random grep searches (use semantic search!)
- Ignore call path tracing (understanding flow is critical)
- Overwhelm user with too much detail
- Skip symbol structure overview

## Success Criteria

This skill succeeds when:
- User understands codebase architecture quickly
- Minimal tokens used (via get_symbols)
- Clear execution flow explained
- Relevant code located efficiently
- User can navigate codebase independently afterward

## Performance

- **get_symbols**: ~100ms (vs seconds to read/parse full file)
- **fast_search**: <10ms text, <100ms semantic
- **trace_call_path**: <200ms (including cross-language)
- **fast_refs**: <50ms

Total exploration session: ~500ms + reading time for specific targets

---

**Remember:** Julie's superpower is understanding code structure WITHOUT reading entire files. Use get_symbols liberally, search semantically, and trace execution flows for rapid comprehension!
