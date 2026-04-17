# Skills & Hooks Pre-v7.0 Audit — Design

**Date:** 2026-04-17
**Status:** Design — awaiting audit findings
**Ship gate:** v7.0

## Goal

Before shipping v7.0, verify that:

1. All 11 skills in `.claude/skills/` (and their distribution copies in `julie-plugin/skills/`) are factually correct (no dead tool references, no outdated paths) and cover relevant features shipped since each skill's last-modified date.
2. All hooks in both the julie and julie-plugin repos behave correctly on Windows and honor the Claude Code hook contract (exit codes, stdout shape, fail-open semantics).
3. `julie-plugin/hooks/run.cjs` — the MCP launcher on end-user machines — is Windows-safe.

Capture harness-specific observations as forward-looking notes for a future harness-independence dot release; do not implement harness-independence fixes in this pass.

## Background

### Why this audit

- Julie has shipped significant infra changes since most skills were last touched: dashboard (v6.x), daemon mode (v6.7), sidecar embeddings (v6.5 removal of ORT), VB.NET language (v7.0), diff-scoped xtask test workflow (today), narrow-test PreToolUse guardrail (today).
- Two new hooks shipped in the julie repo today: `pretool-broad-tests.cjs` and `session-start-tests.cjs`. These have not been exercised on Windows.
- User concern: the project has leaned too far into Claude Code as a harness. Harness-independence is acknowledged as future work but not part of this audit's fix scope.

### Current inventory

**Skills (11, in both `julie/.claude/skills/` and `julie-plugin/skills/`):**
`architecture`, `call-trace`, `dependency-graph`, `editing`, `explore-area`, `impact-analysis`, `logic-flow`, `metrics`, `search-debug`, `type-flow`, `web-research`

Skill last-modified spread (in julie repo): Mar 19 (`metrics`) → Apr 11 (most). Likely-stale candidates based on dates alone: `metrics`, `architecture`, `editing`.

**Hooks (julie repo, dev-only — 4 files + hooks.json):**
- `pretool-edit.cjs` (nudges toward `edit_file`/`edit_symbol`)
- `pretool-agent.cjs` (reminds to give subagents Julie-tool instructions)
- `pretool-broad-tests.cjs` (new today — blocks broad `cargo test` runs)
- `session-start-tests.cjs` (new today — reminds about narrow-test workflow)

**Hooks (julie-plugin, distributed — 3 files + hooks.json + MCP launcher):**
- `pretool-edit.cjs`
- `pretool-agent.cjs`
- `session-start.cjs` (injects Julie tool behavioral guidance)
- `run.cjs` (MCP launcher, ~200 lines)
- `run.test.cjs` (DI-extracted unit tests)

## Audit Methodology

### Finding format

Each finding is structured as:

- **File:** absolute path
- **Finding:** one sentence describing what's wrong or missing
- **Severity:**
  - `blocker` — broken (refs a nonexistent thing), Windows-unsafe, or misleads the user in a way that causes wasted work
  - `quality` — missing coverage, stale wording, or inconsistency that degrades UX but does not break
  - `forward` — harness-specific assumption; note for later, do not fix now
- **Recommendation:** concrete fix, or "defer," or "no action"

### Skill audit passes (per skill, in order)

1. **Broken.** Does the skill reference Julie tools, paths, flags, or features that don't exist?
   - Verify each tool reference (e.g., `fast_search`, `deep_dive`, `edit_symbol`) against the actual tool surface via `fast_search`/`deep_dive` + the MCP handler registration.
   - Verify any cited parameter names, search target values, or flags against the tool's current schema.
2. **Missing coverage.** Since the skill's last-modified date, have relevant features shipped that the skill does not mention?
   - Examples: `metrics` predates the dashboard and codehealth unreliability finding; `search-debug` predates recent tokenizer/centrality changes; `architecture` predates the daemon/sidecar refactor.
   - Compare `git log --since=<skill-mtime> -- <relevant-paths>` for the skill's subject matter.
3. **Alignment.** Does the skill contradict current `CLAUDE.md` or `julie-plugin/hooks/session-start.cjs`?
   - The three sources of truth (CLAUDE.md, session-start.cjs, skills) should tell the same story about Julie tool usage. Diff the concepts.
4. **Forward (notes only).** Flag Claude-Code-only vocabulary (e.g., `TaskCreate`, `Agent` tool, slash commands, `${CLAUDE_PLUGIN_ROOT}`). Record; do not fix.

### Hook audit passes (per file)

1. **Windows correctness.**
   - Path separators (use `/` or `path.join`, never hardcoded `\\`).
   - Env var expansion behavior (`${VAR}` works in `hooks.json` as Claude Code's hook runner handles it; avoid shell-specific forms).
   - No shell builtin assumptions (`echo`, `cat`, etc.) in command strings — commands run through Claude Code's hook dispatcher, not a POSIX shell.
   - Line ending handling in stdin parsing.
   - Special attention to `pretool-broad-tests.cjs` (regex against shell commands — verify the regex is shell-dialect-agnostic) and `run.cjs` (binary path resolution, archive extraction, cache writability on Windows).
2. **Hook contract correctness.**
   - Exit code 0 = pass, 2 = block (per Claude Code hook spec).
   - SessionStart hooks output either plain text to stdout OR JSON with `hookSpecificOutput.additionalContext`.
   - PreToolUse hooks can write advisory text to stdout (stderr for blocking messages).
   - `process.stdin` handling must be end-aware (Node's stdin does not auto-close).
   - Fail-open on JSON parse errors — do not block the user because the framework input shape changed.
3. **Cross-repo consistency.**
   - `pretool-edit.cjs` exists in both julie and julie-plugin. Diff them. If they disagree, decide which is canonical and align the other.
   - Same for `pretool-agent.cjs`.
   - `session-start-tests.cjs` (julie repo) and `session-start.cjs` (plugin) serve different audiences (dev vs. end-user) — that's intentional, not a bug. Verify the audience boundary is clear.
4. **`run.cjs`-specific.**
   - Unit test coverage: `run.test.cjs` already exists; confirm it runs clean with `node hooks/run.test.cjs`.
   - Binary path resolution across platforms.
   - Archive extraction (tar.gz on macOS/Linux, zip on Windows).
   - Cache directory writability and fallback paths.
   - Error messages for missing binaries / network failures.

## Teammate Assignments (Parallel Audit Phase)

Three Sonnet teammates run in parallel via `razorback:team-driven-development`, audit-only.

**Each teammate prompt includes:**
- The finding format above.
- The audit criteria for their scope.
- Mandate to use Julie tools: `fast_search`, `get_symbols`, `deep_dive`, `fast_refs`, `get_context`. Do NOT fall back to Glob/Grep/Read chains.
- Mandate: audit only. Do not modify files. Do not run tests.
- Max 25 words per finding.

**Teammate-A — Skills batch 1 (6 skills):**
`architecture`, `call-trace`, `dependency-graph`, `editing`, `explore-area`, `impact-analysis`

**Teammate-B — Skills batch 2 (5 skills):**
`logic-flow`, `metrics`, `search-debug`, `type-flow`, `web-research`

**Teammate-C — Hooks + run.cjs:**
- `julie/.claude/hooks/hooks.json`
- `julie/.claude/hooks/pretool-edit.cjs`
- `julie/.claude/hooks/pretool-agent.cjs`
- `julie/.claude/hooks/pretool-broad-tests.cjs`
- `julie/.claude/hooks/session-start-tests.cjs`
- `julie-plugin/hooks/hooks.json`
- `julie-plugin/hooks/pretool-edit.cjs`
- `julie-plugin/hooks/pretool-agent.cjs`
- `julie-plugin/hooks/session-start.cjs`
- `julie-plugin/hooks/run.cjs`
- `julie-plugin/hooks/run.test.cjs` (context only)

## Consolidation

After all three teammates report, the lead consolidates findings into this design doc's sibling:

`docs/plans/2026-04-17-skills-hooks-audit-findings.md`

With sections:

1. **Executive summary** — counts by severity (blockers / quality / forward), one-line per target.
2. **Per-file findings** — grouped by target, structured per the finding format.
3. **Proposed fix scope for v7.0** — blockers by default; quality findings marked with a fix-or-defer recommendation.
4. **Deferred items** — quality findings the lead recommends slipping, with rationale.
5. **Forward findings** — all harness-specific observations consolidated into one section as input to a future harness-independence brainstorm.

## v7.0 Ship Gate

After the findings doc is committed, the user reviews and marks each finding:

- **fix-now-blocker** — must fix before v7.0 tag.
- **fix-now-bonus** — worth one more day of work.
- **defer** — followup issue, does not gate v7.0.

This is the explicit ship-vs-slip decision point. The user's call, not the lead's default.

## Fix Phase (after gate)

Branching by finding shape:

- **Mechanical fixes** (rename a path, delete a line, add a sentence, align two files): lead executes directly with `edit_file`/`edit_symbol`. No separate plan, no team, inline review.
- **Non-trivial fixes** (rewrite a skill section, change hook logic): invoke `razorback:writing-plans` for an execution plan, then `razorback:team-driven-development` for parallel fix work.

**Dual-editing rule for skills:**
Skills live in two places: `julie/.claude/skills/<name>/SKILL.md` (source of truth) and `julie-plugin/skills/<name>/SKILL.md` (distribution copy). The GHA workflow in julie-plugin syncs skills from julie on release, but only on release. Until then, fixes must be applied to *both* copies so the plugin remains consistent until v7.0 tagging triggers the GHA sync.

**Hooks are NOT dual-edited:**
- julie repo hooks are dev-only (apply when working in the julie repo).
- julie-plugin hooks are distributed.
- They are deliberately different and serve different audiences. Findings about one do not automatically apply to the other. Cross-repo consistency findings (e.g., "these two files drifted") will be flagged as such and resolved per-finding.

## Verification

- **Skill content changes:** manual read-through post-fix; re-verify any Julie tool references via `fast_search`. No automated test possible.
- **Hook `.cjs` files:** run each standalone with simulated stdin, e.g.:
  ```bash
  echo '{"tool_input":{"command":"cargo test"}}' | node .claude/hooks/pretool-broad-tests.cjs
  echo $?
  ```
  Assert expected exit code.
- **`run.cjs`:** run `node hooks/run.test.cjs` (existing test suite) to confirm no regression.
- **If any Rust source gets touched (unlikely for this audit):** lead runs `cargo xtask test changed` once per completed batch. Subagents do NOT run broader tests — only narrow `cargo nextest run --lib <name>` for their specific changes, per the project's narrow-test guardrail.

## Out of Scope (explicit)

- **Harness-independence implementation.** Captured as forward findings only; fix in a later dot release.
- **Skill triggering-effectiveness eval.** That's `skill-creator` territory and is a separate research project.
- **Any refactor of `run.cjs`** beyond Windows-correctness and hook-contract fixes.
- **GHA workflow changes** in julie-plugin. If the audit finds a workflow bug, flag and defer.

## Acceptance Criteria

- [ ] Design doc committed to `docs/plans/2026-04-17-skills-hooks-audit-design.md`.
- [ ] Three teammates complete their audit scopes without touching any files.
- [ ] Findings doc committed to `docs/plans/2026-04-17-skills-hooks-audit-findings.md`.
- [ ] All 11 skills scanned; findings recorded with severity.
- [ ] All 11 hook-related files scanned (5 in julie: 4 hook scripts + `hooks.json`; 6 in julie-plugin: 3 hook scripts + `hooks.json` + `run.cjs` + `run.test.cjs`); findings recorded.
- [ ] `run.test.cjs` passes clean after any `run.cjs` edits.
- [ ] Every `blocker`-severity finding is either fixed or explicitly waived with rationale in the findings doc.
- [ ] Every `quality`-severity finding has a disposition (fix now / slip to followup).
- [ ] All `forward`-severity findings are consolidated into a single section for the future harness-independence brainstorm.
- [ ] Any skill fix is applied to both `julie/.claude/skills/` and `julie-plugin/skills/` (dual-edit rule).

## Risks

- **Windows behavior inference from macOS.** We cannot actually run hooks on Windows from this session. Audit is code-review only for Windows concerns. Accept this limitation; document any findings as "suspected" until user or CI validates on Windows.
- **Teammate-C scope is larger than A and B** (10 files vs. ~6 skills each). The hook files are short, but `run.cjs` alone is ~200 lines. If Teammate-C is overloaded, split it: one teammate on julie-repo hooks + plugin hooks, another on `run.cjs` + `run.test.cjs`.
- **Harness-agnostic noise.** The forward-findings section could balloon if teammates over-flag. Keep the bar high: only flag things that would actually prevent the skill from working on another harness, not things that merely *mention* Claude Code.
