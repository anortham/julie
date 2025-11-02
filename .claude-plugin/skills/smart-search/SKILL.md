---
name: smart-search
description: Intelligently choose between semantic and text search based on query intent. Automatically selects the best search mode (semantic for concepts, text for exact terms, symbols for definitions) and provides relevant results. Use when user wants to find code.
allowed-tools: mcp__julie__fast_search, mcp__julie__get_symbols, Read
---

# Smart Search Skill

## Purpose
**Automatically select the best search strategy** based on query intent. This skill understands when to use semantic search (concepts), text search (exact terms), or symbol search (definitions) and presents results effectively.

## When to Activate
Use when the user wants to find code:
- **General search**: "find the authentication code"
- **Concept search**: "where is error handling?"
- **Exact term search**: "find all console.error calls"
- **Symbol search**: "find UserService class"
- **Exploratory**: "show me the database code"

## Search Mode Selection Intelligence

### Semantic Search (Conceptual Understanding)
**Use when query describes WHAT, not HOW:**

```
Triggers:
- "authentication logic" (concept)
- "error handling" (behavior)
- "database connections" (functionality)
- "user management" (domain)
- "payment processing" (business logic)

fast_search({ query: "...", mode: "semantic" })
```

**Best for:**
- Understanding intent ("find auth code" → finds JWT, OAuth, sessions)
- Cross-language concepts
- Architecture exploration
- Business logic discovery

### Text Search (Exact Terms)
**Use when query specifies EXACT strings:**

```
Triggers:
- "console.error" (specific API)
- "import React" (exact syntax)
- "TODO: fix" (exact comment)
- "throw new Error" (specific pattern)
- "localhost:3000" (literal string)

fast_search({ query: "...", mode: "lines" })
```

**Best for:**
- Finding specific API usage
- Literal string matches
- Fast, precise lookups
- Code pattern matching

### Symbol Search (Definitions Only)
**Use when query asks for SPECIFIC symbols:**

```
Triggers:
- "UserService class" (class definition)
- "getUserData function" (function definition)
- "AuthToken interface" (type definition)
- "class PaymentProcessor" (explicit class)

fast_search({ query: "...", mode: "symbols" })
```

**Best for:**
- Finding definitions
- Locating specific symbols
- Type/interface lookup
- Class/function discovery

## Query Analysis Decision Tree

```
User query → Analyze intent

Is it a concept/behavior? (what does it do?)
  YES → Semantic search
  ├─ "authentication", "error handling", "data validation"
  └─ Returns: Conceptually relevant code

Is it an exact string/API? (specific syntax?)
  YES → Text search
  ├─ "console.log", "import", "throw new"
  └─ Returns: Exact matches

Is it a symbol name? (class/function/type?)
  YES → Symbol search
  ├─ "UserService", "fetchData", "AuthToken"
  └─ Returns: Symbol definitions

Is it ambiguous?
  YES → Try semantic first, fallback to text
  └─ Present best results from both
```

## Orchestration Examples

### Example 1: Concept Search (Semantic)

```markdown
User: "Find the authentication logic"

Analysis: Concept query (behavior, not specific code)
→ Mode: Semantic

→ fast_search({ query: "authentication logic", mode: "semantic" })

Results (scored by relevance):
1. src/middleware/auth.ts (0.95) - JWT authentication middleware
2. src/services/auth-service.ts (0.91) - Authentication service
3. src/utils/jwt.ts (0.87) - JWT utilities
4. src/guards/auth.guard.ts (0.84) - Route guards

Present: "Found authentication logic in 4 files.
Main implementation: JWT middleware (auth.ts) with
service layer (auth-service.ts) and utilities (jwt.ts)."
```

### Example 2: Exact String Search (Text)

```markdown
User: "Find all console.error calls"

Analysis: Exact API usage
→ Mode: Text (lines)

→ fast_search({ query: "console.error", mode: "lines" })

Results (15 matches):
- src/services/user.ts:42 - console.error('Failed to load user')
- src/api/payments.ts:78 - console.error('Payment failed', error)
- src/utils/logger.ts:12 - console.error(message, ...args)
- ... (12 more)

Present: "Found 15 console.error calls across 8 files.
Most common in services/ and api/ directories.
Consider using logger.error instead?"
```

### Example 3: Symbol Search (Definition)

```markdown
User: "Find the UserService class"

Analysis: Specific symbol (class)
→ Mode: Symbols

→ fast_search({ query: "UserService", mode: "symbols" })

Results:
- src/services/user-service.ts:15 - class UserService
  Type: class
  Exports: yes
  Methods: 8

→ get_symbols({ file: "src/services/user-service.ts", mode: "structure" })

Present: "UserService class is defined in user-service.ts
Methods: findById, findByEmail, create, update, delete,
authenticate, changePassword, resetPassword"
```

### Example 4: Ambiguous Query (Hybrid)

```markdown
User: "Find database stuff"

Analysis: Ambiguous ("stuff" is vague)
→ Try semantic first

→ fast_search({ query: "database operations", mode: "semantic" })

Results:
- src/database/connection.ts (0.93)
- src/models/ (0.89)
- src/repositories/ (0.85)

→ Also try text: fast_search({ query: "database", mode: "lines" })

More results:
- import statements with "database"
- Comments mentioning "database"
- Configuration files

Present: "Database code found in:
- Connection management: database/connection.ts
- Data models: models/ directory
- Repository pattern: repositories/ directory
Also found 23 mentions in configs and comments."
```

## Search Result Presentation

### Clear Relevance Ranking
```markdown
Results (sorted by relevance):

🥇 src/auth/jwt.ts (score: 0.95)
   JWT token generation and validation

🥈 src/auth/middleware.ts (score: 0.91)
   Authentication middleware for Express

🥉 src/guards/auth.guard.ts (score: 0.87)
   NestJS authentication guard
```

### Grouped by Category
```markdown
Authentication found in:

📁 Core Implementation:
- src/auth/jwt.ts - Token handling
- src/auth/middleware.ts - Express middleware

📁 Application Layer:
- src/guards/auth.guard.ts - Route guards
- src/decorators/auth.decorator.ts - Auth decorators

📁 Utilities:
- src/utils/crypto.ts - Encryption utilities
```

### With Code Context
```markdown
src/middleware/auth.ts:

export function authenticate(req, res, next) {
  const token = extractToken(req);
  if (!token) {
    return res.status(401).json({ error: 'No token' });
  }
  // ...
}

This middleware validates JWT tokens and extracts user info.
Used by 15 routes across the API.
```

## Search Optimization Patterns

### Progressive Refinement
```
1. Start broad (semantic)
2. If too many results → narrow with text search
3. If too few results → broaden query
4. Use symbols for precise targeting
```

### Combine with Symbol Structure
```
1. Search finds relevant file
2. get_symbols shows structure
3. User sees overview without reading file
4. Navigate to specific symbol if needed
```

### Follow-up Queries
```
Initial: "Find database code"
→ Semantic search finds repositories/

Follow-up: "Show me the User repository"
→ Symbol search finds UserRepository

Detail: "What methods does it have?"
→ get_symbols shows all methods
```

## Key Behaviors

### ✅ DO
- Analyze query intent before searching
- Use semantic for concepts and behaviors
- Use text for exact strings and API calls
- Use symbols for specific definitions
- Present results with context
- Offer to show file structure with get_symbols
- Suggest refinements if results unclear

### ❌ DON'T
- Default to one search mode for everything
- Return raw results without analysis
- Overwhelm with too many matches
- Skip symbol structure for unfamiliar files
- Ignore user's search language/terminology

## Query Intent Indicators

### Semantic Indicators
- "logic", "handling", "management", "processing"
- "how does...", "where is...", "find code for..."
- Domain terms without specific syntax
- Behavioral descriptions

### Text Indicators
- Exact function names in quotes
- API calls (console.log, fetch, etc.)
- Import statements
- Literal strings
- TODO comments

### Symbol Indicators
- "class X", "function Y", "interface Z"
- CamelCase/PascalCase names
- "definition of...", "where is X defined"
- Type names

## Success Criteria

This skill succeeds when:
- Correct search mode selected automatically
- Relevant results found quickly
- Results presented with useful context
- User doesn't need to retry with different mode
- Follow-up queries feel natural

## Performance

- **Semantic search**: <100ms (GPU accelerated)
- **Text search**: <10ms (SQLite FTS5)
- **Symbol search**: <5ms (indexed)
- **Result ranking**: ~1ms
- **get_symbols**: ~100ms

Total search experience: <200ms including presentation

---

**Remember:** The right search mode makes all the difference. Semantic for concepts, text for exact matches, symbols for definitions. Let Julie's intelligence choose the best approach!
