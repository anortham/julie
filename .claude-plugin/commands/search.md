---
name: search
description: Quick semantic search across the codebase with intelligent mode selection
---

# Search Command

Perform intelligent code search with automatic mode selection.

## Task

1. **Parse search query:**
   Extract query from command arguments.

2. **Analyze query intent:**
   Determine best search mode:
   - **Concept/behavior** → semantic mode
   - **Exact string/API** → lines mode
   - **Symbol name** → symbols mode

3. **Execute search:**
   ```
   fast_search({
     query: "[user's query]",
     mode: "[determined mode]",
     limit: 10
   })
   ```

4. **Present results clearly:**

### For Semantic Results
```markdown
🔍 Search: "[query]" (semantic)

📄 src/auth/jwt.ts (score: 0.95)
   JWT token generation and validation
   class JwtService { ... }

📄 src/auth/middleware.ts (score: 0.91)
   Authentication middleware for Express
   export function authenticate() { ... }

📄 src/guards/auth.guard.ts (score: 0.87)
   NestJS authentication guard
   @Injectable() class AuthGuard { ... }

---
Found 8 results. Showing top 3.
Use get_symbols to see file structure.
```

### For Text Results
```markdown
🔍 Search: "console.error" (exact match)

📄 src/services/user.ts:42
   console.error('Failed to load user', error);

📄 src/api/payments.ts:78
   console.error('Payment failed', error);

📄 src/utils/logger.ts:12
   console.error(message, ...args);

---
Found 15 matches across 8 files.
```

### For Symbol Results
```markdown
🔍 Search: "UserService" (symbols)

📄 src/services/user-service.ts:15
   class UserService
   Methods: findById, findByEmail, create, update, delete

📄 src/services/__tests__/user-service.test.ts:8
   const mockUserService = ...

---
Use /symbols src/services/user-service.ts to see full structure.
```

## Query Examples

### Semantic Queries (Concepts)
```
/search authentication logic
/search error handling
/search database connections
/search payment processing
/search user validation
```

### Text Queries (Exact Strings)
```
/search "console.error"
/search "import React"
/search "TODO: fix"
/search "throw new Error"
/search "localhost:3000"
```

### Symbol Queries (Definitions)
```
/search UserService
/search PaymentProcessor class
/search fetchData function
/search AuthToken interface
```

## Optional Flags

Support these flags:
- `--semantic` - Force semantic mode
- `--text` - Force text/lines mode
- `--symbols` - Force symbols mode
- `--limit N` - Return N results (default: 10)

Examples:
```
/search --semantic payment processing
/search --text "console.log"
/search --symbols UserService
/search --limit 20 authentication
```

## No Results Handling

If no results found:
```markdown
🔍 No results for "[query]"

Suggestions:
- Try broader terms (e.g., "auth" instead of "authenticate")
- Check spelling
- Try different search mode:
  /search --text "[query]"  (for exact matches)
  /search --semantic "[query]"  (for concepts)
- Verify workspace is indexed: /index
```

## Follow-up Suggestions

After showing results:
```markdown
💡 Next steps:
- See file structure: /symbols [file]
- Navigate to definition: Use fast_goto
- Find references: Use fast_refs
- Trace execution: Use trace_call_path
```

## Key Behaviors

- Auto-detect search mode intelligently
- Present top results with context
- Include file paths and line numbers (for text search)
- Show relevance scores (for semantic search)
- Suggest follow-up actions
- Offer mode override flags

## Integration with Skills

The smart-search skill provides the intelligence for mode selection.
This command is just a user-friendly interface to fast_search.
