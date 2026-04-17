# Skills & Hooks Pre-v7.0 Audit — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development to execute this plan with three parallel teammates and inline lead review.

**Goal:** Complete the audit phase (Tasks 1–3 parallel, Task 4 consolidation) so the user can make a ship-vs-slip gate decision before v7.0 tagging.

**Architecture:** Three Sonnet teammates run in parallel, each owning a disjoint slice of files. Each produces a structured findings report. Lead consolidates into a single findings document at `docs/plans/2026-04-17-skills-hooks-audit-findings.md`. Fix phase is deliberately NOT in this plan — it depends on the user's gate decisions and will be planned after findings land.

**Tech Stack:** Julie MCP tools (`fast_search`, `deep_dive`, `get_symbols`, `fast_refs`, `get_context`). No test execution in this plan phase. No file modifications by teammates.

**Design doc:** `docs/plans/2026-04-17-skills-hooks-audit-design.md`

---

## Shared Teammate Conventions

All three audit teammates (Tasks 1–3) follow these rules. The lead includes them in every teammate prompt.

**Finding format (strict):**

```
File: <absolute path>
Finding: <one sentence — what's wrong, missing, or worth flagging>
Severity: blocker | quality | forward
Recommendation: <concrete fix, or "defer", or "no action">
```

**Severity definitions:**
- `blocker` — broken (references a nonexistent tool/path/feature), Windows-unsafe, or misleads the user into wasted work
- `quality` — missing coverage of a shipped feature, stale wording, or inconsistency that degrades UX but does not break
- `forward` — harness-specific assumption (Claude-Code-only vocabulary, slash commands, `${CLAUDE_PLUGIN_ROOT}`); note only, do not fix

**Max 25 words per finding.** No paragraphs, no speculation.

**Julie tools mandate:**
- Use `fast_search`, `get_symbols`, `deep_dive`, `fast_refs`, `get_context` for all codebase exploration.
- Do NOT fall back to Glob/Grep/Read chains.
- Use `get_symbols` before `Read` to see file structure first.

**Hard rules:**
- Audit only. Do NOT modify any files.
- Do NOT run `cargo test`, `cargo xtask`, or any test tier. The orchestrating session handles any test runs.
- If a file is plain markdown (skill SKILL.md), still use `get_context` and `get_symbols` first where applicable; fall back to `Read` for small markdown files.
- Report the final findings as a single markdown block at the end of your response, under a heading `## Findings`.

---

## Task 1: Teammate-A — Skills batch 1 audit

**Files (input, read-only):**
- `/Users/murphy/source/julie/.claude/skills/architecture/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/call-trace/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/dependency-graph/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/editing/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/explore-area/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/impact-analysis/SKILL.md`

**Cross-reference inputs (read as needed):**
- `/Users/murphy/source/julie/CLAUDE.md`
- `/Users/murphy/source/julie-plugin/hooks/session-start.cjs`
- Julie tool surface via `fast_search(query="<tool_name>", search_target="definitions")` — verify tool references exist
- `git log --oneline -30` to see recent commits touching each skill's subject area

**What to build:** A structured findings report covering all 6 skills in batch 1, applying the four audit passes (Broken, Missing coverage, Alignment, Forward) from the design doc.

**Approach:**
- For each skill, read the SKILL.md file.
- Pass 1 (Broken): extract every Julie tool name, parameter, or feature reference. Verify each exists in the current julie codebase via `fast_search(search_target="definitions")`. Flag any that don't.
- Pass 2 (Missing coverage): check the skill's last-modified date via `git log -1 --format=%ai -- <path>`. Run `git log --since=<that date> --oneline` scoped to relevant subsystem paths. Flag shipped features the skill doesn't mention.
- Pass 3 (Alignment): skim CLAUDE.md and session-start.cjs. If the skill says something contradictory, flag.
- Pass 4 (Forward): flag Claude-Code-only vocabulary (`TaskCreate`, `Agent` tool, slash commands, `${CLAUDE_PLUGIN_ROOT}`). Severity: `forward`.
- Also spot-check each skill's plugin copy at `/Users/murphy/source/julie-plugin/skills/<name>/SKILL.md` — if the content differs from julie's copy in a user-visible way, flag as a `blocker` with "plugin copy drifted from source of truth."

**Acceptance criteria:**
- [ ] All 6 skills scanned with all four audit passes.
- [ ] Findings reported in the exact format above, under a `## Findings` heading.
- [ ] Every finding has a severity and a recommendation.
- [ ] No file modifications.
- [ ] No test runs.
- [ ] If no findings for a skill, state "No findings" explicitly so the lead knows the pass happened.

---

## Task 2: Teammate-B — Skills batch 2 audit

**Files (input, read-only):**
- `/Users/murphy/source/julie/.claude/skills/logic-flow/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/metrics/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/search-debug/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/type-flow/SKILL.md`
- `/Users/murphy/source/julie/.claude/skills/web-research/SKILL.md`

**Cross-reference inputs:** Same as Task 1.

**What to build:** Same audit as Task 1, applied to the 5 skills in batch 2.

**Approach:**
- Same four audit passes as Task 1.
- Extra attention on known likely-stale candidates:
  - `metrics` (last touched Mar 19 — predates the dashboard and the `project_codehealth_unreliable` finding that codehealth metrics are shelved).
  - `search-debug` (check against recent tokenizer/centrality/stemming changes — see commits like `325aaeb9`, `a5516cbb`).
  - `web-research` (fresh — Apr 6/Apr 8 — but verify the browser39 + filewatcher integration still matches its stated flow).

**Acceptance criteria:**
- [ ] All 5 skills scanned with all four audit passes.
- [ ] Findings reported in the exact format, under a `## Findings` heading.
- [ ] Every finding has a severity and a recommendation.
- [ ] Known-stale candidates (`metrics`, `search-debug`) get extra scrutiny; the report explicitly notes whether they are stale in practice or only stale by date.
- [ ] No file modifications, no test runs.
- [ ] If no findings for a skill, state "No findings" explicitly.

---

## Task 3: Teammate-C — Hooks + run.cjs audit

**Files (input, read-only):**

*julie repo (dev-only hooks, 5 files):*
- `/Users/murphy/source/julie/.claude/hooks/hooks.json`
- `/Users/murphy/source/julie/.claude/hooks/pretool-edit.cjs`
- `/Users/murphy/source/julie/.claude/hooks/pretool-agent.cjs`
- `/Users/murphy/source/julie/.claude/hooks/pretool-broad-tests.cjs`
- `/Users/murphy/source/julie/.claude/hooks/session-start-tests.cjs`

*julie-plugin repo (distributed hooks, 6 files):*
- `/Users/murphy/source/julie-plugin/hooks/hooks.json`
- `/Users/murphy/source/julie-plugin/hooks/pretool-edit.cjs`
- `/Users/murphy/source/julie-plugin/hooks/pretool-agent.cjs`
- `/Users/murphy/source/julie-plugin/hooks/session-start.cjs`
- `/Users/murphy/source/julie-plugin/hooks/run.cjs`
- `/Users/murphy/source/julie-plugin/hooks/run.test.cjs` (read for context; this is unit tests, not a hook)

**Cross-reference inputs:**
- Claude Code hook contract (what you already know; do not web-fetch): exit code 0 = pass, 2 = block; SessionStart outputs JSON with `hookSpecificOutput.additionalContext` OR plain text stdout; PreToolUse can use stderr for blocking messages.
- `/Users/murphy/source/julie/CLAUDE.md` — for cross-reference with project rules.

**What to build:** A structured findings report covering all 11 hook-related files, applying the four hook audit passes from the design doc (Windows correctness, Hook contract, Cross-repo consistency, `run.cjs`-specific).

**Approach:**
- **Pass 1 (Windows correctness):**
  - Grep each `.cjs` file for hardcoded `\\` separators, `require('child_process').exec` with unquoted paths, shell builtins assumed (`echo`, `cat`, `rm`, `cp`), POSIX-only env var forms (`$VAR`), and line-ending assumptions in stdin parsing.
  - For `pretool-broad-tests.cjs`: manually trace the regex against realistic Windows command strings (`cargo.exe xtask test dev`, `cargo nextest run --lib`, commands with `\\` paths). The regex should match regardless of shell.
  - For `run.cjs`: special attention on binary path resolution (`.exe` suffix handling), archive extraction (`tar.gz` vs. `.zip`), cache directory writability, and any `execSync`/`spawn` calls.
- **Pass 2 (Hook contract):**
  - For each hook, verify: exit code policy (0/2), stdout shape, `process.stdin` end-awareness, fail-open JSON parse error handling.
  - Verify `hooks.json` matcher and command shape is current with Claude Code's hook spec (e.g., `matcher: "Edit"`, not a regex if the spec changed; `type: "command"`).
- **Pass 3 (Cross-repo consistency):**
  - Diff `julie/.claude/hooks/pretool-edit.cjs` against `julie-plugin/hooks/pretool-edit.cjs`. If they differ, decide which is canonical and flag the drift.
  - Same for `pretool-agent.cjs`.
  - Verify `session-start-tests.cjs` (julie) and `session-start.cjs` (plugin) are intentionally different (different audiences: dev vs. end-user).
- **Pass 4 (`run.cjs`-specific):**
  - Verify `run.test.cjs` covers the current `run.cjs` surface. If `run.cjs` has code paths not covered by tests, flag as `quality`.
  - Check `run.cjs` error messages — do they point the user to a useful action on failure?
  - Check the harness sniffing in `session-start.cjs` (the `CURSOR_PLUGIN_ROOT` vs `CLAUDE_PLUGIN_ROOT` branch) — is the logic correct? Does `run.cjs` do anything similar, or is it Claude-Code-only?

**Acceptance criteria:**
- [ ] All 11 hook-related files scanned with applicable audit passes.
- [ ] Findings reported in the exact format, under a `## Findings` heading.
- [ ] Every finding has a severity and a recommendation.
- [ ] `pretool-broad-tests.cjs` regex is manually traced against at least 3 Windows command strings; the trace is included in the report.
- [ ] Cross-repo consistency check is explicit: either "matches" or "drifted — <diff summary>."
- [ ] `run.test.cjs` coverage assessment is explicit: either "covers current run.cjs surface" or "gaps: <list>."
- [ ] No file modifications, no test runs (including no `node run.test.cjs`).
- [ ] If no findings for a file, state "No findings" explicitly.

---

## Task 4: Lead — Consolidation (sequential, after Tasks 1–3)

**Files:**
- Create: `/Users/murphy/source/julie/docs/plans/2026-04-17-skills-hooks-audit-findings.md`

**What to build:** A single consolidated findings document that merges the three teammate reports into a structured artifact the user can review and gate on.

**Approach:**
- Collect the three `## Findings` blocks from the teammate reports.
- Deduplicate where two teammates flagged the same cross-cutting concern.
- Structure the findings doc:

  ```markdown
  # Skills & Hooks Audit Findings

  **Date:** 2026-04-17
  **Design doc:** docs/plans/2026-04-17-skills-hooks-audit-design.md

  ## Executive Summary

  | Target               | Blockers | Quality | Forward |
  |----------------------|----------|---------|---------|
  | Skills batch 1       | N        | N       | N       |
  | Skills batch 2       | N        | N       | N       |
  | Hooks + run.cjs      | N        | N       | N       |
  | **Total**            | **N**    | **N**   | **N**   |

  ## Blockers (must-fix for v7.0 by default)

  [One subsection per blocker, grouped by file.]

  ## Quality Findings (fix-or-defer)

  [One subsection per quality finding, with lead's recommendation.]

  ## Forward Findings (harness-independence input)

  [All `forward` severity findings consolidated into one section as input to a future harness-independence brainstorm. No fixes here.]

  ## Lead Recommendations

  - Proposed v7.0 fix scope: [list of findings the lead thinks should block the tag]
  - Proposed slip: [list of findings the lead thinks can wait]
  - Open questions for the user: [any finding where the lead couldn't decide]
  ```

- Announce to the user that the findings doc is ready and invite the gate decision. Do NOT commit the findings doc — the user's explicit commit policy applies.

**Acceptance criteria:**
- [ ] Findings doc created at the exact path above.
- [ ] Executive summary table populated with real counts.
- [ ] Every teammate finding is preserved (no silent dropping). Duplicates across teammates are merged with a note.
- [ ] Forward findings are isolated in their own section — not mixed into blockers or quality.
- [ ] Lead recommendations section is present and non-empty.
- [ ] User is prompted to review and make the gate decision.

---

## Out of scope (explicit)

- **Fix phase tasks.** The fix phase depends on the user's gate decisions and will be planned after Task 4 completes. Trying to plan fixes upfront would require placeholders (banned by writing-plans skill).
- **Any file modifications during Tasks 1–4.** Even if a teammate sees an obvious one-character typo fix, they do not touch files. This is an audit, not a fix pass.
- **Harness-independence implementation.** Forward findings feed a future brainstorm; this plan does not address them.

## Post-plan steps (for the lead, after Task 4)

1. Present findings doc to the user.
2. User marks each finding `fix-now-blocker` / `fix-now-bonus` / `defer`.
3. Branch based on gate decisions:
   - **Mechanical-only findings:** lead fixes directly with `edit_file`/`edit_symbol`, inline, no new plan.
   - **Non-trivial findings:** invoke `razorback:writing-plans` again to produce a fix plan, then `razorback:team-driven-development` to execute.
4. Dual-edit rule: any skill fix applies to BOTH `julie/.claude/skills/<name>/SKILL.md` AND `julie-plugin/skills/<name>/SKILL.md`.
5. Hooks fixes are NOT dual-edited — julie-repo and julie-plugin hooks are deliberately different.
