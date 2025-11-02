---
name: symbols
description: Show file structure and symbols without reading entire file (70-90% token savings)
---

# Symbols Command

Display file structure and symbols efficiently without reading the entire file.

## Task

1. **Parse file path:**
   Extract file path from command arguments.
   Support both absolute and relative paths.

2. **Call get_symbols:**
   ```
   get_symbols({
     file_path: "[file]",
     mode: "structure"  // default mode
   })
   ```

3. **Present structure clearly:**

### Structure Mode (Default)
```markdown
📄 src/services/user-service.ts

**Imports:**
- UserRepository from '../repositories'
- logger from '../utils/logger'
- { validateEmail } from '../utils/validation'

**Classes:**

class UserService
  ├─ constructor(repo: UserRepository)
  ├─ findById(id: string): Promise<User>
  ├─ findByEmail(email: string): Promise<User | null>
  ├─ create(data: CreateUserDTO): Promise<User>
  ├─ update(id: string, data: UpdateUserDTO): Promise<User>
  ├─ delete(id: string): Promise<void>
  ├─ authenticate(email: string, password: string): Promise<AuthResult>
  ├─ changePassword(id: string, oldPass: string, newPass: string): Promise<void>
  └─ resetPassword(email: string): Promise<void>

**Exports:**
- UserService (default)
- UserServiceError (named)

**Interfaces:**
- CreateUserDTO
- UpdateUserDTO
- AuthResult

---
📊 File stats: 8 methods, 3 interfaces, 285 lines
💡 Use /search to find implementations or related code
```

### Full Mode (Detailed)
```markdown
📄 src/services/user-service.ts (full details)

**Imports:**
- UserRepository from '../repositories/user-repository'
  Type: class import
- logger from '../utils/logger'
  Type: default import
- { validateEmail } from '../utils/validation'
  Type: named import, function

**Classes:**

class UserService
  Location: line 15
  Exported: yes (default)

  Methods:
  - findById(id: string): Promise<User>
    Line: 23
    Access: public
    Parameters: id (string)
    Returns: Promise<User>
    Calls: repo.findById, logger.debug

  - findByEmail(email: string): Promise<User | null>
    Line: 31
    Access: public
    Parameters: email (string)
    Returns: Promise<User | null>
    Calls: validateEmail, repo.findByEmail

  [... more details ...]

**Relationships:**
- Imports from: UserRepository, logger, validation utils
- Called by: 12 locations (use fast_refs to see)
- Calls: 8 repository methods, 3 utility functions

---
💡 Next: Use fast_refs to see where this is used
      Use trace_call_path to see execution flow
```

### Definitions Mode (Minimal)
```markdown
📄 src/services/user-service.ts (definitions)

Symbols:
- class UserService (line 15)
- interface CreateUserDTO (line 45)
- interface UpdateUserDTO (line 52)
- interface AuthResult (line 59)
- class UserServiceError (line 67)

Total: 5 symbols
```

## Mode Selection

Users can specify mode:
```
/symbols src/file.ts              # structure (default)
/symbols src/file.ts --full       # full details
/symbols src/file.ts --definitions # just symbol list
```

**Mode meanings:**
- **structure** (default): Overview with imports, exports, classes, methods
- **full**: Complete details including relationships and call information
- **definitions**: Minimal list of symbols only

## File Path Handling

Accept various formats:
```
/symbols src/services/user.ts      # relative path
/symbols /abs/path/to/file.ts      # absolute path
/symbols user-service.ts           # filename (search for it first)
```

If just filename given, search for it first:
```
→ fast_search({ query: "user-service.ts", mode: "lines" })
→ If single match, use that file
→ If multiple matches, ask user to clarify
```

## Token Savings Highlight

Always mention the efficiency:
```markdown
💾 Token Efficiency:
Reading full file: ~3,500 tokens
This symbol view: ~400 tokens
Savings: ~89% (3,100 tokens saved!)

You can read the full file if needed, but this structure
gives you a great overview first.
```

## No Symbols Found

If file has no extractable symbols:
```markdown
📄 src/data/config.json

⚠️  No symbols found

This file may be:
- Configuration/data file (JSON, YAML, etc.)
- Empty or comments only
- Unsupported language
- Binary file

Supported languages: TypeScript, JavaScript, Python, Java,
C#, PHP, Ruby, Swift, Kotlin, C, C++, Go, Rust, Lua,
GDScript, Vue, Razor, SQL, HTML, CSS, Regex, Bash,
PowerShell, Zig, Dart (25 total)
```

## Integration with Search

After showing symbols, suggest searches:
```markdown
💡 Related commands:
- Find usage: fast_refs({ symbol: "UserService" })
- Find definition: fast_goto({ symbol: "findById" })
- Trace calls: trace_call_path({ symbol: "authenticate" })
- Search similar: /search user service pattern
```

## Key Behaviors

- Default to "structure" mode (best balance)
- Show clear hierarchy for classes/methods
- Include file statistics
- Highlight token savings
- Suggest follow-up actions
- Handle missing files gracefully

## Error Handling

- **File not found**: Suggest searching for it
- **Unsupported language**: List supported languages
- **No symbols**: Explain why (config file, binary, etc.)
- **Permission error**: Suggest checking file permissions
