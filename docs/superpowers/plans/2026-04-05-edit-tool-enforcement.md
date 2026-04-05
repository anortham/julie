# Edit Tool Enforcement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make agents use Julie's edit_file/edit_symbol tools by default instead of the Read+Edit antipattern, through three reinforcing enforcement layers.

**Architecture:** Three independent layers (SessionStart instructions, PreToolUse hook, rewritten skill) that each work without the others. Changes span two repos: `julie` (source of truth) and `julie-plugin` (distribution).

**Tech Stack:** Node.js (session-start.cjs), JSON (hooks.json), Markdown (SKILL.md, JULIE_AGENT_INSTRUCTIONS.md)

**Spec:** `docs/superpowers/specs/2026-04-05-edit-tool-enforcement-design.md`

---

### Task 1: Rewrite the Editing Skill (both repos)

The skill is the most self-contained change and doesn't depend on the other layers. The existing skill at `.claude/skills/editing/SKILL.md` has a description that never triggers and a body that's too thin to redirect behavior.

**Files:**
- Modify: `.claude/skills/editing/SKILL.md` (julie repo, source of truth)
- Modify: `~/source/julie-plugin/skills/editing/SKILL.md` (distribution copy)

- [ ] **Step 1: Rewrite the skill in the julie repo**

Replace the entire contents of `.claude/skills/editing/SKILL.md` with:

```markdown
---
name: editing
description: >-
  Use BEFORE making any code or file changes -- whenever you're about to use
  Read+Edit, sed, or any modify-then-write pattern. Routes to Julie's edit_file
  and edit_symbol tools which edit files directly without reading them first.
  Trigger on: fix, change, update, modify, refactor, rename, replace, add,
  remove, move, or any task involving changes to existing files. Even one-line
  changes. Even non-code files.
allowed-tools: mcp__julie__edit_file, mcp__julie__edit_symbol, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__fast_search
---

# Editing Files with Julie

Julie's edit tools modify files without reading them first. This is the default
path for all file modifications.

## Which tool do I use?

- **Creating a new file?** Use the Write tool. This skill doesn't apply.
- **Changing a symbol (function, struct, class, method)?** Use `deep_dive` to understand it, then `edit_symbol` to change it.
- **Changing arbitrary text in a file?** Use `edit_file` with `old_text` and `new_text`.
- **Need to understand the file first?** Use `get_symbols` (structure) or `deep_dive` (full context), then use `edit_symbol`. Not Read.

## Stop and check

If you catch yourself thinking any of these, you're about to waste tokens:

| Thought | What to do instead |
|---------|-------------------|
| "I need to read the file first" | No. `edit_file` uses DMP fuzzy matching on `old_text`. `edit_symbol` finds symbols by name. Neither needs a Read. |
| "It's just a quick change" | Quick changes are `edit_file`'s sweet spot. `edit_file(old_text=..., new_text=..., dry_run=true)` -- done. |
| "I'm not sure of the exact text to match" | Use `get_symbols` or `deep_dive` to see the code, then `edit_symbol` to change it. Still no Read+Edit. |
| "This isn't a code file" | `edit_file` works on ANY text file: YAML, TOML, Markdown, .gitignore, configs, everything. |
| "The edit is too complex for fuzzy matching" | Try it with `dry_run=true` first. DMP handles whitespace differences, minor mismatches. You'll see the diff before applying. |

## Workflow

1. **Always preview first**: `dry_run=true` (the default). Review the diff.
2. **Then apply**: same call with `dry_run=false`.

### edit_symbol (for code symbols)

- `operation: "replace"` -- swap an entire function/struct/class definition
- `operation: "insert_after"` -- add code after a symbol
- `operation: "insert_before"` -- add code before a symbol

### edit_file (for any text)

- `old_text`: text to find (DMP fuzzy matched)
- `new_text`: replacement text
- `occurrence`: `"first"` (default), `"last"`, or `"all"`

## Example: the cost of Read+Edit

Changing a version number in Cargo.toml:

**Read+Edit pattern (5 calls, ~800 tokens):**
1. Edit -- fails ("File has not been read yet")
2. Read Cargo.toml -- waste
3. Grep for the version line -- unnecessary
4. Read again with offset -- more waste
5. Edit -- finally works

**edit_file pattern (2 calls, ~200 tokens):**
1. `edit_file(file_path="Cargo.toml", old_text='version = "6.6.2"', new_text='version = "6.6.3"', dry_run=true)` -- preview
2. Same call with `dry_run=false` -- done

4x fewer tokens. 3 fewer round trips.
```

- [ ] **Step 2: Copy the skill to julie-plugin**

```bash
cp /Users/murphy/source/julie/.claude/skills/editing/SKILL.md /Users/murphy/source/julie-plugin/skills/editing/SKILL.md
```

- [ ] **Step 3: Verify both files are identical**

```bash
diff /Users/murphy/source/julie/.claude/skills/editing/SKILL.md /Users/murphy/source/julie-plugin/skills/editing/SKILL.md
```

Expected: no output (files are identical).

- [ ] **Step 4: Commit in julie repo**

```bash
cd /Users/murphy/source/julie
git add .claude/skills/editing/SKILL.md
git commit -m "feat(plugin): rewrite editing skill for better triggering and enforcement

Skill was never being invoked because description matched tool names
instead of user intent words. New description triggers on fix/change/
update/modify/refactor/etc. Body adds decision tree, rationalization
prevention table, and concrete before/after token cost example."
```

- [ ] **Step 5: Commit in julie-plugin repo**

```bash
cd /Users/murphy/source/julie-plugin
git add skills/editing/SKILL.md
git commit -m "feat: update editing skill with enforcement patterns

Synced from julie repo. Better trigger description, decision tree,
rationalization prevention table, concrete examples."
```

---

### Task 2: Upgrade SessionStart Instructions

Modify the guidance string in `session-start.cjs` to promote editing to its own section and add the rationalization prevention block. This also requires updating `JULIE_AGENT_INSTRUCTIONS.md` for consistency.

**Files:**
- Modify: `~/source/julie-plugin/hooks/session-start.cjs`
- Modify: `/Users/murphy/source/julie/JULIE_AGENT_INSTRUCTIONS.md`

- [ ] **Step 1: Update the guidance string in session-start.cjs**

In `~/source/julie-plugin/hooks/session-start.cjs`, replace the entire `guidance` template string (lines 6-22) with:

```javascript
const guidance = `You have Julie, a code intelligence MCP server. Follow these rules:

1. **Search before coding**: Always fast_search before writing new code.
   - For exact symbols: fast_search(query="SymbolName", search_target="definitions")
   - For concepts: fast_search(query="error handling retry logic", search_target="definitions") uses semantic search.
2. **Structure before reading**: Always get_symbols before Read (70-90% token savings).
3. **References before changes**: Always fast_refs before modifying any symbol.
4. **Deep dive for understanding**: Use deep_dive when you need to understand a symbol's full context (callers, callees, types) before modifying it.
5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.

**Editing workflow**: edit_file and edit_symbol are the DEFAULT for all file modifications. They edit without reading the file first.
- Code symbols: deep_dive > edit_symbol (dry_run=true first)
- Any text: edit_file(old_text=..., new_text=..., dry_run=true)
- Read + Edit is the FALLBACK, not the default. Use only when Julie tools genuinely cannot handle the edit.

**Edit antipatterns -- if you catch yourself doing these, STOP:**
- Reading a file just to edit it -> use edit_file directly
- Using Read to find exact text for Edit -> use get_symbols or deep_dive, then edit_symbol
- "It's just a quick change" -> quick changes are edit_file's sweet spot
- Falling back to Read + Edit "because it's easier" -> it's 3-5x more tokens. It's not easier.

Do not use grep/find when Julie tools are available.
Do not read files without get_symbols first.
Do not chain multiple tools when deep_dive does it in one call.`;
```

Key changes from the current version:
- Rule 6 ("Edit without reading") removed from the numbered list
- Editing promoted to its own section with bold heading
- Rationalization prevention block added (4 antipatterns)
- "Do not use Read + Edit..." line at the bottom removed (redundant with the new section)

- [ ] **Step 2: Update JULIE_AGENT_INSTRUCTIONS.md rule #6**

In `/Users/murphy/source/julie/JULIE_AGENT_INSTRUCTIONS.md`, replace rule 6 and the Workflow/closing lines to match the new framing. Replace the entire file with:

```markdown
# Julie - Code Intelligence Server

## Rules

1. **Search before coding**: Always `fast_search` before writing new code.
2. **Structure before reading**: Always `get_symbols` before Read (70-90% token savings).
3. **References before changes**: Always `fast_refs` before modifying any symbol.
4. **Deep dive before modifying**: Use `deep_dive` before changing a symbol. One call replaces chaining fast_search + get_symbols + fast_refs + Read.
5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.

## Tools

- `fast_search`: Find code by text. `search_target="definitions"` promotes exact symbol matches.
- `get_symbols`: File structure without reading full content. Use `target` + `mode="minimal"` to extract one symbol.
- `deep_dive`: Investigate a symbol: definition, callers, callees, children, types. Always use before modifying.
- `fast_refs`: All references to a symbol. Required before any change. Use `reference_kind` to filter.
- `get_context`: Token-budgeted area orientation (pivots + neighbors). Use at start of task.
- `rename_symbol`: Workspace-wide rename. Always preview with `dry_run=true` first.
- `manage_workspace`: Index, add/remove references, health check. First action in new workspace: `operation="index"`.
- `edit_file`: Edit a file without reading it first. DMP fuzzy matching for old_text. Always `dry_run=true` first.
- `edit_symbol`: Edit a symbol by name. Operations: replace, insert_after, insert_before. Always `dry_run=true` first.
- `query_metrics`: Code health (security/change risk, test coverage), session stats, trend history.

## Editing Workflow

`edit_file` and `edit_symbol` are the DEFAULT for all file modifications. They edit without reading the file first.
- Code symbols: `deep_dive` > `edit_symbol` (`dry_run=true` first)
- Any text: `edit_file(old_text=..., new_text=..., dry_run=true)`
- Read + Edit is the FALLBACK, not the default. Use only when Julie tools genuinely cannot handle the edit.

## Other Workflows

- **New task**: get_context > deep_dive key symbols > fast_refs > implement
- **Bug fix**: fast_search > deep_dive > write failing test > fix
- **Refactor**: fast_refs > deep_dive > rename_symbol (dry_run first)

Do not use grep/find when Julie tools are available. Do not read files without get_symbols first. Do not chain multiple tools when deep_dive does it in one call.
```

Key changes:
- Rule 6 removed from numbered list; editing now has its own `## Editing Workflow` section
- "Editing" workflow line moved from the general workflow section into the new dedicated section
- "Do not use Read + Edit..." closing line removed (covered by the new section)
- General workflows remain under `## Other Workflows`

- [ ] **Step 3: Verify session-start.cjs runs without errors**

```bash
cd /Users/murphy/source/julie-plugin
CLAUDE_PLUGIN_ROOT="$(pwd)" node hooks/session-start.cjs
```

Expected: JSON output with `hookSpecificOutput.additionalContext` containing the updated guidance string. No errors.

- [ ] **Step 4: Commit in julie-plugin repo**

```bash
cd /Users/murphy/source/julie-plugin
git add hooks/session-start.cjs
git commit -m "feat: upgrade SessionStart with editing enforcement

Promote editing to dedicated section with rationalization prevention
block. Edit tools framed as default, Read+Edit as fallback."
```

- [ ] **Step 5: Commit in julie repo**

```bash
cd /Users/murphy/source/julie
git add JULIE_AGENT_INSTRUCTIONS.md
git commit -m "feat: promote editing workflow to dedicated section

Rule 6 replaced with Editing Workflow section. edit_file/edit_symbol
framed as default, Read+Edit as fallback. Consistent with plugin
SessionStart injection."
```

---

### Task 3: Upgrade the PreToolUse Hook

Replace the verbose tip echo with a concise one-liner in both repos' hooks.json files.

**Files:**
- Modify: `~/source/julie-plugin/hooks/hooks.json`
- Modify: `/Users/murphy/source/julie/.claude/hooks/hooks.json`

- [ ] **Step 1: Update julie-plugin hooks.json**

In `~/source/julie-plugin/hooks/hooks.json`, replace the Edit hook command. The full file should be:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "echo \"Use edit_file or edit_symbol instead -- they don't require reading the file first.\""
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "startup|clear|compact",
        "hooks": [
          {
            "type": "command",
            "command": "node \"${CLAUDE_PLUGIN_ROOT}/hooks/session-start.cjs\"",
            "async": false
          }
        ]
      }
    ]
  }
}
```

Changes: only the `command` string in the Edit PreToolUse hook. SessionStart hook unchanged.

- [ ] **Step 2: Update julie dev hooks.json**

In `/Users/murphy/source/julie/.claude/hooks/hooks.json`, replace the entire file with:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit",
        "command": "echo \"Use edit_file or edit_symbol instead -- they don't require reading the file first.\""
      }
    ]
  }
}
```

Note: the julie dev hooks.json uses the flat format (no `hooks` array wrapper under each matcher), matching its current structure. It does NOT have the SessionStart hook (that's plugin-only).

- [ ] **Step 3: Commit in julie-plugin repo**

```bash
cd /Users/murphy/source/julie-plugin
git add hooks/hooks.json
git commit -m "fix(hooks): replace verbose Edit tip with concise nudge

Old: 22-word tip with 'Consider using them instead'
New: 13-word direct statement framing Edit as the wrong tool"
```

- [ ] **Step 4: Commit in julie repo**

```bash
cd /Users/murphy/source/julie
git add .claude/hooks/hooks.json
git commit -m "fix(hooks): replace verbose Edit tip with concise nudge

Consistent with plugin hooks.json update."
```

---

### Task 4: Verify and Test

Manually verify the changes work together. No automated tests for instruction/hook/skill content, but we can validate that the files are syntactically correct and the SessionStart hook executes.

**Files:** None (verification only)

- [ ] **Step 1: Validate julie-plugin hooks.json is valid JSON**

```bash
node -e "JSON.parse(require('fs').readFileSync('/Users/murphy/source/julie-plugin/hooks/hooks.json', 'utf8')); console.log('Valid JSON')"
```

Expected: `Valid JSON`

- [ ] **Step 2: Validate julie hooks.json is valid JSON**

```bash
node -e "JSON.parse(require('fs').readFileSync('/Users/murphy/source/julie/.claude/hooks/hooks.json', 'utf8')); console.log('Valid JSON')"
```

Expected: `Valid JSON`

- [ ] **Step 3: Run session-start.cjs and verify output structure**

```bash
cd /Users/murphy/source/julie-plugin
CLAUDE_PLUGIN_ROOT="$(pwd)" node hooks/session-start.cjs 2>&1 | node -e "
const data = JSON.parse(require('fs').readFileSync('/dev/stdin', 'utf8'));
const ctx = data.hookSpecificOutput?.additionalContext || '';
const checks = [
  ['Has editing workflow section', ctx.includes('Editing workflow')],
  ['Has antipatterns block', ctx.includes('Edit antipatterns')],
  ['Has edit_file as default', ctx.includes('DEFAULT for all file modifications')],
  ['Has Read+Edit as fallback', ctx.includes('FALLBACK')],
  ['No old rule 6', !ctx.includes('6. **Edit without reading**')],
];
checks.forEach(([name, pass]) => console.log(pass ? 'PASS' : 'FAIL', name));
"
```

Expected: all PASS.

- [ ] **Step 4: Verify skill YAML frontmatter parses**

```bash
node -e "
const fs = require('fs');
const content = fs.readFileSync('/Users/murphy/source/julie/.claude/skills/editing/SKILL.md', 'utf8');
const match = content.match(/^---\n([\s\S]*?)\n---/);
if (!match) { console.log('FAIL: no frontmatter'); process.exit(1); }
const lines = match[1].split('\n');
const hasName = lines.some(l => l.startsWith('name:'));
const hasDesc = lines.some(l => l.startsWith('description:'));
const hasTools = lines.some(l => l.startsWith('allowed-tools:'));
console.log(hasName ? 'PASS' : 'FAIL', 'has name');
console.log(hasDesc ? 'PASS' : 'FAIL', 'has description');
console.log(hasTools ? 'PASS' : 'FAIL', 'has allowed-tools');
"
```

Expected: all PASS.

- [ ] **Step 5: Spot-check skill description contains trigger words**

```bash
grep -c "fix, change, update, modify, refactor, rename, replace, add" /Users/murphy/source/julie/.claude/skills/editing/SKILL.md
```

Expected: `1` (the description line).
