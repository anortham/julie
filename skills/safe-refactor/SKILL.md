---
name: safe-refactor
description: Perform safe code refactoring with reference checking and validation. Uses Julie's rename_symbol for workspace-wide renames and fuzzy_replace for precise edits. Activates when user wants to refactor, rename, or safely modify code.
allowed-tools: mcp__julie__rename_symbol, mcp__julie__fuzzy_replace, mcp__julie__fast_refs, mcp__julie__fast_goto, mcp__julie__get_symbols, Edit, Read, Bash
---

# Safe Refactor Skill

## Purpose
Perform **safe, validated code refactoring** using Julie's intelligence to prevent breaking changes. This skill combines **reference checking**, **workspace-wide renames**, and **fuzzy matching edits** to modify code confidently.

## When to Activate
Use when the user:
- **Wants to rename**: "rename this function", "change variable name"
- **Needs to refactor**: "extract this code", "refactor this class"
- **Modifies code safely**: "update this API", "change this interface"
- **Reorganizes code**: "move this to another file", "split this class"

## Julie's Refactoring Tools

### 🔄 Workspace-Wide Renaming

**rename_symbol** - AST-aware symbol renaming
```
Renames symbols across entire workspace
- Checks all references first
- AST-aware (understands code structure)
- Safe multi-file updates
- Preserves code semantics
```

**Use when:** Renaming classes, functions, variables, types

### ✏️ Precise Code Editing

**fuzzy_replace** - Diff-match-patch fuzzy matching
```
Professional text replacement with fuzzy tolerance
- Handles whitespace variations
- Tolerates minor differences
- Character-based (UTF-8 safe)
- Validation before commit
```

**Use when:** Targeted edits, method bodies, imports, specific code sections

### 🔍 Safety Checks

**fast_refs** - Find all references
```
Critical for impact analysis
- See all usage points
- Verify scope of change
- Identify breaking changes
```

**fast_goto** - Find definitions
```
Verify symbol exists before rename
Understand current implementation
```

**get_symbols** - Structure validation
```
Verify file structure before/after edits
Ensure no symbols lost
```

## Orchestration Strategy

### Pattern 1: Safe Symbol Rename
**Goal:** Rename across entire workspace

```
1. fast_refs({ symbol: "oldName" }) → Check all references
2. Review impact (how many files affected?)
3. Ask user confirmation if >10 files
4. rename_symbol({ old_name: "oldName", new_name: "newName" })
5. Verify completion
```

### Pattern 2: Targeted Code Edit
**Goal:** Modify specific code section

```
1. get_symbols to understand structure
2. Read target section
3. fuzzy_replace with old/new strings
4. Verify edit succeeded
5. Run tests if available
```

### Pattern 3: Extract Function
**Goal:** Extract code to new function

```
1. Identify code to extract
2. fast_refs to check variable usage
3. Create new function with fuzzy_replace
4. Replace original code with call
5. Verify both edits
```

### Pattern 4: Refactor with Validation
**Goal:** Large refactoring with safety

```
1. get_symbols(mode="structure") → Baseline
2. Perform edits (rename_symbol + fuzzy_replace)
3. get_symbols again → Verify structure
4. fast_refs on modified symbols → Check usage
5. Run tests → Confirm no breakage
```

## Example Refactoring Sessions

### Example 1: Rename Function Safely

```markdown
User: "Rename getUserData to fetchUserProfile"

Skill activates → Safe rename process

Step 1: Check References
→ fast_refs({ symbol: "getUserData" })

References found (8 locations):
- src/services/user.ts (definition)
- src/api/users.ts (3 calls)
- src/components/Profile.tsx (2 calls)
- src/hooks/useUser.ts (1 call)
- tests/user.test.ts (1 call)

Step 2: Confirm Impact
→ "Found 8 references across 5 files. This is a safe rename.
   Proceeding with workspace-wide rename."

Step 3: Execute Rename
→ rename_symbol({
    old_name: "getUserData",
    new_name: "fetchUserProfile"
  })

Step 4: Verify
→ fast_refs({ symbol: "fetchUserProfile" })
→ Confirm all 8 references updated

Result: "✅ Renamed getUserData → fetchUserProfile across 5 files (8 references)"
```

### Example 2: Refactor with Fuzzy Replace

```markdown
User: "Change this error handling to use our new logger"

Current code:
console.error('Failed to load user:', error);
throw error;

Step 1: Read Context
→ get_symbols to see function structure

Step 2: Prepare Replacement
Old (with fuzzy tolerance):
console.error('Failed to load user:', error);

New:
logger.error('Failed to load user', { error });
throw new UserLoadError('Failed to load user', error);

Step 3: Execute Fuzzy Replace
→ fuzzy_replace({
    file_path: "src/services/user.ts",
    old_string: "console.error('Failed to load user:', error);\nthrow error;",
    new_string: "logger.error('Failed to load user', { error });\nthrow new UserLoadError('Failed to load user', error);",
    similarity_threshold: 0.8
  })

Step 4: Verify
→ Read updated section
→ Confirm change applied correctly

Result: "✅ Updated error handling to use logger.error and UserLoadError"
```

### Example 3: Extract to New Function

```markdown
User: "Extract this validation logic to a separate function"

Current code in saveUser():
if (!email || !email.includes('@')) {
  throw new Error('Invalid email');
}
if (!password || password.length < 8) {
  throw new Error('Password too short');
}

Step 1: Check Variable Usage
→ fast_refs on 'email', 'password'
→ Confirm local to function

Step 2: Create New Function
→ fuzzy_replace to add:
function validateUserInput(email: string, password: string) {
  if (!email || !email.includes('@')) {
    throw new Error('Invalid email');
  }
  if (!password || password.length < 8) {
    throw new Error('Password too short');
  }
}

Step 3: Replace Original Code
→ fuzzy_replace validation block with:
validateUserInput(email, password);

Step 4: Verify
→ get_symbols → See new function added
→ Read both locations → Verify correctness

Result: "✅ Extracted validation logic to validateUserInput()"
```

## Safety Checklist

Before any refactoring:

### ✅ Pre-Refactor Checks
- [ ] Check references with `fast_refs`
- [ ] Understand current structure with `get_symbols`
- [ ] Identify impact scope (how many files?)
- [ ] Confirm no active PRs on affected files
- [ ] Consider backward compatibility needs

### ✅ During Refactor
- [ ] Use appropriate tool (rename_symbol vs fuzzy_replace)
- [ ] Provide clear old/new strings for fuzzy_replace
- [ ] Use reasonable similarity threshold (0.75-0.85)
- [ ] Verify each step before proceeding

### ✅ Post-Refactor Validation
- [ ] Verify edits applied correctly
- [ ] Check symbol structure unchanged (unless intended)
- [ ] Confirm all references updated
- [ ] Run tests if available
- [ ] Review git diff for unexpected changes

## Tool Selection Guide

### Use rename_symbol when:
- Renaming classes, functions, methods, variables
- Need workspace-wide consistency
- AST-aware understanding required
- Multi-file impact expected

### Use fuzzy_replace when:
- Modifying method bodies
- Updating imports/exports
- Changing specific code sections
- Tolerance for whitespace variations needed

### Use Edit tool when:
- Simple, exact string replacement
- Single file, small change
- No fuzzy matching needed
- Direct line-based editing

## Error Handling

### fuzzy_replace Validation Errors
```
Error: "Similarity 0.65 below threshold 0.75"
→ Check old_string matches current code
→ Adjust threshold or fix old_string
→ Verify no unexpected changes in file
```

### rename_symbol Failures
```
Error: "Symbol not found: oldName"
→ Verify symbol exists with fast_goto
→ Check spelling and casing
→ Ensure symbol is in scope
```

### Reference Conflicts
```
Warning: "Found 47 references across 15 files"
→ Confirm with user before proceeding
→ Consider splitting into smaller renames
→ Verify scope is correct
```

## Integration with Other Skills

### With Sherpa Workflows
```
[Refactor Workflow - Phase 1: Tests First]
→ Ensure test coverage exists
→ safe-refactor skill: Perform refactoring
[Phase 2: Refactor Code]
→ Use rename_symbol and fuzzy_replace
[Phase 3: Verify]
→ Run tests, check references
```

### With Goldfish Memory
```
[Before refactoring]
→ Goldfish: checkpoint({ description: "Pre-refactor state" })
[Perform refactoring]
→ safe-refactor skill: Execute changes
[After refactoring]
→ Goldfish: checkpoint({ description: "Refactored X to Y, 8 files updated" })
```

## Key Behaviors

### ✅ DO
- Always check references before renaming
- Use fuzzy_replace for tolerance to whitespace
- Verify edits succeeded
- Run tests after significant changes
- Provide clear descriptions of changes
- Ask for confirmation on large-scope changes

### ❌ DON'T
- Rename without checking references first
- Use exact string matching when fuzzy is better
- Skip post-refactor verification
- Ignore test failures after refactoring
- Make changes without understanding impact
- Proceed with risky renames without user consent

## Success Criteria

This skill succeeds when:
- Code refactored without breaking changes
- All references updated consistently
- Tests still pass (if applicable)
- Changes are semantically correct
- User confident in the safety of edits

## Performance

- **rename_symbol**: ~200-500ms (depending on workspace size)
- **fuzzy_replace**: ~50-100ms per edit
- **fast_refs**: ~50ms for reference checking
- **get_symbols**: ~100ms for structure validation

Total refactoring session: ~1-2 seconds + test run time

---

**Remember:** Safe refactoring is about checking references, understanding impact, and verifying changes. Use Julie's tools to make confident edits without breaking your codebase!
