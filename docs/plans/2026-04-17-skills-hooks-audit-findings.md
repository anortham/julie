# Skills & Hooks Audit Findings

**Date:** 2026-04-17
**Design doc:** [`2026-04-17-skills-hooks-audit-design.md`](./2026-04-17-skills-hooks-audit-design.md)
**Implementation plan:** [`2026-04-17-skills-hooks-audit.md`](./2026-04-17-skills-hooks-audit.md)
**Audit teammates:** `skills-batch-1-auditor`, `skills-batch-2-auditor`, `hooks-auditor`

## Executive Summary

| Target               | Blockers | Quality | Forward |
|----------------------|----------|---------|---------|
| Skills batch 1 (6)   | 0        | 4       | 0       |
| Skills batch 2 (5)   | 0        | 4       | 2       |
| Hooks + run.cjs (11) | 0        | 6       | 1       |
| **Total**            | **0**    | **14**  | **3**   |

**Key result: zero blockers.** No finding breaks the user, misleads into wasted work unrecoverably, or fails on Windows. Every item is quality-improvement territory. The ship-vs-slip decision is a scope call, not a correctness call.

**Key positive findings:**
- **All 11 skills:** plugin copies in `julie-plugin/skills/` are byte-identical to the julie source. No dual-edit churn needed during fixes (beyond what we add going forward).
- **Cross-repo hooks:** `pretool-edit.cjs` and `pretool-agent.cjs` are byte-identical across julie and julie-plugin. No drift.
- **`pretool-broad-tests.cjs` regex:** traced against 8 command strings including Windows edge cases. Behaves correctly on every case except the literal `cargo.exe` form (which users effectively never type).

---

## Blockers (must-fix for v7.0)

None.

---

## Quality Findings

Grouped by target and ordered by lead's recommended priority. Each finding includes a `Lead Rec:` line with ship-vs-slip.

### Skills: `metrics`

**Last modified:** 2026-03-19. Stale in practice (not just by date).

```
File: /Users/murphy/source/julie/.claude/skills/metrics/SKILL.md
Finding: query_metrics supports doc_coverage and dead_code categories; skill documents only session and history.
Severity: quality
Recommendation: Add documentation for doc_coverage and dead_code categories with example usage.
Lead Rec: FIX-NOW. High-traffic skill tells agents how to interrogate codehealth; wrong content here = wasted agent work.
Evidence: src/tools/metrics/mod.rs:38, src/tools/metrics/code_quality.rs (teammate-verified).
```

### Skills: `search-debug`

**Last modified:** 2026-04-11. Not stale by date, but two factual errors in current content.

```
File: /Users/murphy/source/julie/.claude/skills/search-debug/SKILL.md
Finding: Skill names NL_PATH_PENALTY (0.95x) but actual constants are NL_PATH_PENALTY_DOCS/TESTS (0.95) and NL_PATH_PENALTY_FIXTURES (0.75); fixture penalty steeper and undocumented.
Severity: quality
Recommendation: Update to use accurate constant names and add the fixtures path penalty (0.75x).
Lead Rec: FIX-NOW. If agents use this skill to diagnose ranking issues, they'll cite wrong constant names to the user.
```

```
File: /Users/murphy/source/julie/.claude/skills/search-debug/SKILL.md
Finding: Step 6 report template shows "Important pattern boost: 1.5x/none" but compute_pattern_boost always returns 1.0 (unimplemented TODO at src/search/debug.rs:217-221).
Severity: quality
Recommendation: Update template to note that pattern boost is not yet computable from debug output (always 1.0).
Lead Rec: FIX-NOW. Actively misleads agents into reporting a value that's hardcoded.
```

### Skills: `editing`

**Last modified:** 2026-04-05. Three post-dating changes never backfilled.

```
File: /Users/murphy/source/julie/.claude/skills/editing/SKILL.md
Finding: rename_symbol tool registered in handler but absent from skill body and allowed-tools.
Severity: quality
Recommendation: Add mcp__julie__rename_symbol to allowed-tools; add a brief section describing it as the semantic rename path (vs edit_file text replacement).
Lead Rec: FIX-NOW. rename_symbol is the tool agents should prefer for renames; omitting it from the editing skill pushes them to worse approaches.
```

```
File: /Users/murphy/source/julie/.claude/skills/editing/SKILL.md
Finding: edit_symbol has a file_path disambiguation param (different from deep_dive's context_file) not mentioned in skill.
Severity: quality
Recommendation: Add a one-liner: "Use file_path to disambiguate when multiple symbols share a name."
Lead Rec: SLIP. Nice-to-have polish; not a frequent failure mode.
```

```
File: /Users/murphy/source/julie/.claude/skills/editing/SKILL.md
Finding: Line-granularity limitation ("same-line symbols may need manual adjustment") documented in commit 459df8ff but never added to skill body.
Severity: quality
Recommendation: Add a caveat under edit_symbol section noting line-granularity limitation.
Lead Rec: FIX-NOW-BONUS. Caveat lives in a commit message; users will hit it and be confused. Small addition.
```

### Skills: `architecture`

**Last modified:** 2026-03-27.

```
File: /Users/murphy/source/julie/.claude/skills/architecture/SKILL.md
Finding: `feat(workspace): add explicit global workspace targeting` (2026-04-05) not mentioned; skill has no cross-workspace note.
Severity: quality
Recommendation: Add cross-workspace note matching the pattern used in call-trace/dependency-graph/explore-area/impact-analysis skills.
Lead Rec: FIX-NOW-BONUS. Trivial (~3 lines). Batch 1's other 4 skills all have this note; architecture being the odd-one-out is a consistency bug.
```

### Skills: `logic-flow`

```
File: /Users/murphy/source/julie/.claude/skills/logic-flow/SKILL.md
Finding: Step 0 uses fast_search but allowed-tools header omits mcp__julie__fast_search.
Severity: quality
Recommendation: Add mcp__julie__fast_search to the allowed-tools frontmatter line.
Lead Rec: FIX-NOW-BONUS. Trivial one-line fix; allowed-tools list is a real harness contract.
```

```
File: /Users/murphy/source/julie/.claude/skills/logic-flow/SKILL.md
Finding: Important Notes references manage_workspace(operation="open") but it is not in allowed-tools.
Severity: quality (re-categorized from teammate's "forward" — manage_workspace is a real Julie tool, not a harness concern)
Recommendation: Add mcp__julie__manage_workspace to allowed-tools.
Lead Rec: FIX-NOW-BONUS. Same class of fix as the above — small but real.
```

### Skills: `type-flow`

```
File: /Users/murphy/source/julie/.claude/skills/type-flow/SKILL.md
Finding: Important Notes references manage_workspace(operation="open") but manage_workspace is not in allowed-tools.
Severity: quality (re-categorized from teammate's "forward")
Recommendation: Add mcp__julie__manage_workspace to allowed-tools.
Lead Rec: FIX-NOW-BONUS. Same as logic-flow above.
```

### Hooks: `pretool-broad-tests.cjs`

```
File: /Users/murphy/source/julie/.claude/hooks/pretool-broad-tests.cjs
Finding: Regex \bcargo\s+ won't match `cargo.exe xtask test dev`; broad run not blocked when .exe suffix used.
Severity: quality
Recommendation: Extend regex to \bcargo(?:\.exe)?\s+ for both broadTier and unfilteredLib patterns.
Lead Rec: FIX-NOW-BONUS. Two-char fix; belt-and-suspenders for Windows users who might script `cargo.exe` explicitly.
```

### Hooks: `session-start.cjs` (plugin)

```
File: /Users/murphy/source/julie-plugin/hooks/session-start.cjs
Finding: Fallback branch (neither CURSOR_PLUGIN_ROOT nor CLAUDE_PLUGIN_ROOT set) emits { additional_context: guidance } — the Cursor format, not Claude Code's hookSpecificOutput.additionalContext. Context silently not injected.
Severity: quality
Recommendation: Make the fallback emit Claude Code format, or add an explicit check with a stderr warning when neither var is set.
Lead Rec: FIX-NOW. Silent failure territory. CLAUDE_PLUGIN_ROOT is almost always set in practice, but "almost always" + "silent drop of Julie guidance" = exactly the kind of bug that only surfaces in a bad session.
```

### Hooks: `run.cjs`

```
File: /Users/murphy/source/julie-plugin/hooks/run.cjs
Finding: detectPlatform has zero test coverage — all three platform branches and the unsupported-platform null path are untested.
Severity: quality
Recommendation: Add unit tests for each detectPlatform branch, including the null/unsupported case.
Lead Rec: FIX-NOW-BONUS. run.cjs is the launcher; untested platform branches on a Windows user's first run = really bad first impression.
```

```
File: /Users/murphy/source/julie-plugin/hooks/run.cjs
Finding: extractBinary is mocked in all prepareBinaryForLaunch tests; no test covers tar.gz vs .zip branch, Windows bsdtar path, or "binary in use" fallback.
Severity: quality
Recommendation: Add direct extractBinary tests for each archive type and the locked-binary error path.
Lead Rec: SLIP. Non-trivial test work. The code itself already handles these correctly (confirmed by code review); adding tests is hardening, not a bugfix.
```

```
File: /Users/murphy/source/julie-plugin/hooks/run.cjs
Finding: "unsupported platform" and "extraction failed" error messages give the user no remediation hint.
Severity: quality
Recommendation: Append a docs/issue URL to those two error messages.
Lead Rec: FIX-NOW-BONUS. Trivial; first-run failures are high-stakes and this is the only place new users see us communicate.
```

---

## Forward Findings (harness-independence input)

These are consolidated as input to a future harness-independence brainstorm. **Do not fix in this audit pass.**

```
File: /Users/murphy/source/julie-plugin/hooks/run.cjs
Finding: run.cjs is Claude-Code-specific with no harness sniffing; session-start.cjs detects CURSOR vs CLAUDE env vars.
Severity: forward
Observation: Architecturally correct (launcher vs. guidance injector have different responsibilities), but Cursor/OpenCode/Codex users would need a parallel launcher entry point if the plugin is ever distributed for those harnesses.
```

Additional forward observations from skill audits (no action needed in this pass, but worth noting for the harness-independence plan):

- **Skills reference Claude Code tool names** (TaskCreate, Agent tool, slash commands). On other harnesses, the razorback skills already have tool-name mapping conventions (`codex-tools.md` etc.), but the julie-native skills don't. Future plan should either (a) map tool names per-harness like razorback does, or (b) use harness-neutral verbs ("spawn a subagent" instead of "use the Agent tool").
- **`session-start.cjs` is the only file that currently sniffs harness** and has one format branch per harness. If we standardize harness detection, it should probably live in a shared helper.
- **`${CLAUDE_PLUGIN_ROOT}` in `hooks.json`** is Claude-Code-specific. Cursor's equivalent is different. A cross-harness plugin manifest would need parallel hook registrations or a harness-adapter layer.

---

## Lead Recommendations

### Proposed v7.0 fix scope (5 items)

Fix before tagging v7.0:

1. **`metrics` skill** — add `doc_coverage` and `dead_code` documentation. *(Biggest user-value: agents currently told only about stale categories.)*
2. **`search-debug` skill** — fix NL_PATH_PENALTY constant names, add fixtures 0.75x penalty, mark pattern boost as not-yet-computable.
3. **`editing` skill** — add `rename_symbol` section + allowed-tools entry; add line-granularity caveat.
4. **`session-start.cjs` (plugin)** — fix fallback branch to emit Claude Code format or warn.
5. **Allowed-tools header additions** (small batch): `logic-flow` (fast_search + manage_workspace), `type-flow` (manage_workspace).

### Proposed bonus fixes (5 items, if time allows)

Small, trivial, one-by-one decisions:

6. **`architecture` skill** — add cross-workspace note (3 lines).
7. **`editing` skill** — add `file_path` disambiguation one-liner.
8. **`pretool-broad-tests.cjs`** — `\bcargo(?:\.exe)?\s+` regex extension.
9. **`run.cjs` error messages** — append docs/issue URL.
10. **`run.cjs` detectPlatform tests** — unit tests for all 3 branches + null case.

### Proposed slip to follow-up (1 item)

11. **`run.cjs` extractBinary tests** — non-trivial test work; code itself already correct.

### Open questions for the user

- **Dual-edit rule:** every skill fix lands in BOTH `julie/.claude/skills/` and `julie-plugin/skills/`. Plugin copies are currently byte-identical; we preserve that invariant. Confirm.
- **Severity re-categorizations:** teammate-B marked two `manage_workspace` findings as `forward`. I re-categorized to `quality` because `manage_workspace` is a real Julie tool, not a harness concern. Confirm (or override).
- **Fix execution path:** items 1-5 are almost all mechanical (add paragraphs / edit frontmatter / fix a regex). Recommend lead executes directly with `edit_file`. Items 10 (detectPlatform tests) is the only non-trivial one. Either (a) lead includes it in the direct batch using Julie tools, or (b) spawn a small fix-team for test work specifically.
