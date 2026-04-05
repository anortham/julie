# Edit Tool Enforcement Design

**Date:** 2026-04-05
**Status:** Approved
**Problem:** Agents use Read+Edit (3-5 tool calls, ~800+ tokens) instead of Julie's edit_file/edit_symbol (2 calls, ~200 tokens), ignoring existing instructions.

## Problem Analysis

Julie provides `edit_file` and `edit_symbol` tools that edit files without reading them first. Despite instructions in JULIE_AGENT_INSTRUCTIONS.md, SessionStart injection, a PreToolUse hook on Edit, and an editing skill, agents consistently fall back to the Read+Edit pattern.

Root causes identified:
- **SessionStart guidance is buried**: editing is rule #6 in a list, one line, easy to forget mid-task
- **PreToolUse hook is toothless**: echoes a "tip" that gets ignored
- **No hook on Read**: the antipattern starts with Read, but nothing fires there
- **Editing skill never triggers**: the description ("Use when editing code files to save tokens") doesn't match the model's thought patterns at decision time
- **All enforcement is "please"**: nothing creates real friction at the moment of violation

Research into Serena (tool exclusion, per-context tool descriptions, mode-based guidance) and Superpowers (rationalization prevention tables, hard gates, iron laws, SessionStart injection) informed this design.

## Design Constraints

- **Primary audience**: Claude Code, OpenCode, VS Code/GH Copilot (all support Claude plugins)
- **No hard blocks**: tool names may differ across harnesses; Julie's tools may genuinely fail in rare edge cases (~1% of edits)
- **Token-conscious**: a tool promising token savings can't burn tokens on verbose per-call hook messages
- **Layered independence**: each enforcement layer must work without the others, so partial harness support still provides partial benefit
- **Fallback path stays open**: agents must be able to fall back to native tools if Julie's tools are unavailable

## Architecture: Three Independent Layers

| Layer | Mechanism | Token cost | Where it lives |
|-------|-----------|-----------|----------------|
| 1. Instructions | Upgraded SessionStart injection | ~100 tokens, once per session segment | `session-start.cjs` (julie-plugin) |
| 2. Hook | PreToolUse on Edit | ~15 tokens per Edit call | `hooks.json` (julie-plugin) |
| 3. Skill | Rewritten editing skill | ~300 tokens when triggered | `skills/editing/SKILL.md` (both repos) |

The layers compound: instructions set expectations before work starts, the hook creates friction at the moment of violation, and the skill provides detailed guidance during editing tasks. If a harness doesn't support hooks, layers 1 and 3 carry the load. If the skill doesn't trigger, layers 1 and 2 redirect.

## Layer 1: Upgraded SessionStart Instructions

### What changes

In `session-start.cjs`, the `guidance` string gets two modifications:

1. **Promote editing to a visible section** with its own heading, not buried as rule #6
2. **Add a rationalization prevention block** after the editing workflow line

### Rationalization block content

```
**Edit antipatterns -- if you catch yourself doing these, STOP:**
- Reading a file just to edit it -> use edit_file(old_text=..., new_text=...) directly
- Using Read to find exact text for Edit -> use get_symbols or deep_dive, then edit_symbol
- "It's just a quick change" -> quick changes are edit_file's sweet spot
- Falling back to Read + Edit "because it's easier" -> it's 3-5x more tokens. It's not easier.
```

### What stays the same

- Hook fires on `startup|clear|compact` (already covers compaction, which is when models lose behavioral context)
- Total injection stays concise (~35-40 lines, up from ~30)
- No `<EXTREMELY-IMPORTANT>` tags or ALL CAPS; the rationalization table does the enforcement work naturally

## Layer 2: PreToolUse Hook on Edit

### What changes

Replace the current echo tip with a concise one-liner:

```
Use edit_file or edit_symbol instead -- they don't require reading the file first.
```

13 words. The hook's job is a brief nudge, not a tutorial. Detailed guidance lives in the SessionStart instructions and the skill.

### No Read hook

A Read hook was considered and rejected. Read fires 20-50 times per session; even a 10-word message adds 500+ tokens of nag. The token cost of guessing at intent (reading-to-edit vs reading-to-understand) on every call undermines Julie's value proposition. The SessionStart instructions and skill carry this weight instead.

### Implementation

The hook remains a simple command (no external script needed for one line). In `hooks.json`:

```json
{
  "matcher": "Edit",
  "hooks": [{
    "type": "command",
    "command": "echo \"Use edit_file or edit_symbol instead -- they don't require reading the file first.\""
  }]
}
```

## Layer 3: Rewritten Editing Skill

### Description (triggers the skill)

Old (never triggers):
```
Use when editing code files to save tokens. Guides usage of edit_file and edit_symbol tools which don't require reading files first.
```

New (intercepts intent to modify):
```
Use BEFORE making any code or file changes -- whenever you're about to use Read+Edit, sed, or any modify-then-write pattern. Routes to Julie's edit_file and edit_symbol tools which edit files directly without reading them first. Trigger on: fix, change, update, modify, refactor, rename, replace, add, remove, move, or any task involving changes to existing files. Even one-line changes. Even non-code files.
```

Key shifts:
- Triggers on **user intent words** (fix, change, update) not tool names
- Says **BEFORE**, framing it as a pre-step to editing
- Names the antipattern (Read+Edit) so the model recognizes the moment
- Closes escape hatches: "even one-line changes, even non-code files"

### Body structure (~40-50 lines)

**Section 1: Decision tree** (top of skill, immediate routing)
- Creating a new file? -> Write tool, not this skill
- Modifying by symbol name? -> deep_dive -> edit_symbol
- Modifying by text match? -> edit_file
- Need structure first? -> get_symbols or deep_dive, THEN edit_symbol

**Section 2: Rationalization prevention table**

| Thought | Reality |
|---------|---------|
| "I need to read the file first" | edit_file uses fuzzy matching. edit_symbol finds by name. |
| "It's just a quick change" | Quick changes are edit_file's sweet spot. |
| "I'm not sure of the exact text" | get_symbols/deep_dive first, then edit_symbol. Still no Read+Edit. |
| "This isn't a code file" | edit_file works on ANY text file. |

**Section 3: Workflow details**
- edit_symbol operations: replace, insert_after, insert_before
- edit_file: old_text/new_text, DMP fuzzy matching, occurrence="all"
- ALWAYS dry_run=true first, review diff, then dry_run=false

**Section 4: Concrete before/after example**
```
The Cargo.toml version bump:
BAD:  Read -> grep -> Read again -> Edit (5 calls, ~800 tokens)
GOOD: edit_file(old_text='version = "6.6.2"', new_text='version = "6.6.3"', dry_run=true) -> apply (2 calls, ~200 tokens)
```

## Distribution and Sync

| Change | Source (julie) | Distribution (julie-plugin) |
|--------|---------------|---------------------------|
| Editing skill | `.claude/skills/editing/SKILL.md` | `skills/editing/SKILL.md` |
| Edit hook | `.claude/hooks/hooks.json` | `hooks/hooks.json` |
| SessionStart guidance | N/A | `hooks/session-start.cjs` |
| Agent instructions | `JULIE_AGENT_INSTRUCTIONS.md` | Injected via `session-start.cjs` |

Notes:
- Skill updated in both repos; julie is source of truth, manual copy to julie-plugin for immediate availability
- hooks.json is intentionally different between repos (dev-only vs distributed)
- session-start.cjs lives only in julie-plugin; guidance derives from JULIE_AGENT_INSTRUCTIONS.md but is reformatted for injection
- JULIE_AGENT_INSTRUCTIONS.md rule #6 gets reworded to match the rationalization framing (edit_file/edit_symbol as default, Read+Edit as fallback)
- Harnesses without plugin support still get MCP tool descriptions (unchanged) and JULIE_AGENT_INSTRUCTIONS.md if they read it

## Success Criteria

- Agents use edit_file/edit_symbol as the default editing path
- Read+Edit pattern becomes the exception (Julie tools unavailable, DMP matching failure)
- The editing skill actually triggers when agents are about to modify files
- No measurable increase in per-session token overhead from enforcement mechanisms

## Out of Scope

- Enforcement for other antipatterns (grep vs fast_search, Read vs get_symbols); apply these techniques later if edit enforcement proves successful
- Hard-blocking hooks that prevent Edit/Read from executing
- Harness-specific adaptations for Cursor, Windsurf, Gemini CLI (they benefit from MCP tool descriptions and JULIE_AGENT_INSTRUCTIONS.md only)
