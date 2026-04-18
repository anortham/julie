# Skill Set Reduction Design

**Date:** 2026-04-18
**Status:** Design

## Goal

Reduce Julie's shipped skill surface to the small set that carries distinct value, remove skills that are obsolete or redundant with the live tool surface, and keep the julie and julie-plugin copies aligned.

This pass is limited to skills. Hook cleanup is a separate pass.

## Why

The current skill set has three problems:

1. The tool surface changed on 2026-04-18, but the shipped skills did not. `edit_symbol` and `query_metrics` were removed, while `rewrite_symbol` and `call_path` were added.
2. Several skills are thin prompt templates around `deep_dive`, `get_context`, or `fast_refs`, with little unique workflow value.
3. The distributed plugin copies in `~/source/julie-plugin/skills/` drift unless they are updated in the same pass as the julie source copies.

## Decision

### Keep as public skills

- `editing`
- `explore-area`
- `impact-analysis`
- `web-research`

### Keep as repo-only dogfood skill

- `search-debug`

This remains useful for Julie development and search-quality investigation, but it should not ship in the distributed plugin skill set.

### Remove

- `architecture`
- `call-trace`
- `dependency-graph`
- `logic-flow`
- `metrics`
- `type-flow`

## Rationale

### `editing`

Keep, but rewrite around the live editing surface:

- `edit_file` for text replacement
- `rewrite_symbol` for symbol rewriting
- `rename_symbol` for semantic renames

This skill prevents the most common bad workflow, `Read` plus `Edit`, and teaches the highest-value editing path Julie offers.

### `explore-area`

Keep. It is the cleanest starting workflow for `get_context`, then optional `deep_dive` and `get_symbols`.

### `impact-analysis`

Keep. It turns `fast_refs` into a risk-oriented blast-radius workflow instead of a raw reference dump.

### `web-research`

Keep. It coordinates `browser39`, local markdown capture, filewatcher indexing, and selective Julie reads. That workflow is not replaceable with a tool description alone.

### `search-debug`

Keep in the julie repo as dogfood. It encodes product-specific search-debugging knowledge for Julie contributors. It should not ship as a plugin skill because it is niche and Julie-internal.

### Removed skills

- `call-trace` is superseded by `call_path`
- `metrics` is broken because `query_metrics` was removed
- `architecture`, `dependency-graph`, `logic-flow`, and `type-flow` do not justify separate shipped skills because they are report templates over the existing tool descriptions

## Scope

### In scope

- Rewrite kept skills in `julie/.claude/skills/`
- Mirror kept-skill changes into `julie-plugin/skills/`
- Delete removed skills from both repos
- Keep `search-debug` in julie only
- Update julie-plugin workflow and docs so the distributed skill set matches the intended list

### Out of scope

- Hook changes
- SessionStart content changes
- New skill authoring
- Non-skill docs outside the plugin surfaces touched by skill count or skill list changes

## File Map

### julie repo

- `.claude/skills/editing/SKILL.md`
- `.claude/skills/explore-area/SKILL.md`
- `.claude/skills/impact-analysis/SKILL.md`
- `.claude/skills/web-research/SKILL.md`
- `.claude/skills/search-debug/SKILL.md`
- Remove:
  - `.claude/skills/architecture/SKILL.md`
  - `.claude/skills/call-trace/SKILL.md`
  - `.claude/skills/dependency-graph/SKILL.md`
  - `.claude/skills/logic-flow/SKILL.md`
  - `.claude/skills/metrics/SKILL.md`
  - `.claude/skills/type-flow/SKILL.md`

### julie-plugin repo

- `skills/editing/SKILL.md`
- `skills/explore-area/SKILL.md`
- `skills/impact-analysis/SKILL.md`
- `skills/web-research/SKILL.md`
- Remove:
  - `skills/architecture/SKILL.md`
  - `skills/call-trace/SKILL.md`
  - `skills/dependency-graph/SKILL.md`
  - `skills/logic-flow/SKILL.md`
  - `skills/metrics/SKILL.md`
  - `skills/search-debug/SKILL.md`
  - `skills/type-flow/SKILL.md`
- Update:
  - `.github/workflows/update-binaries.yml`
  - `README.md`

## Implementation Notes

### `editing`

Rewrite the skill so it no longer references `edit_symbol`.

Required behavior:

- point code-symbol edits to `rewrite_symbol`
- describe supported `rewrite_symbol` operations
- keep `rename_symbol` as the rename path
- keep `edit_file` as the text-edit path
- preserve the anti-`Read` guidance

### `explore-area`

Keep the current flow with small cleanup only if needed for clarity.

### `impact-analysis`

Keep the current flow with small cleanup only if needed for clarity.

### `web-research`

Keep the workflow, but adjust only if needed for consistency with the reduced public skill set.

### `search-debug`

Refresh factual details so the repo-only copy matches the current search behavior.

### Plugin distribution

The plugin workflow currently copies all 11 skills from the julie repo. Update it to copy only:

- `editing`
- `explore-area`
- `impact-analysis`
- `web-research`

Update the expected skill count and plugin README text to match the reduced set.

## Acceptance Criteria

- [ ] julie keeps exactly 5 skills: `editing`, `explore-area`, `impact-analysis`, `search-debug`, `web-research`
- [ ] julie-plugin ships exactly 4 skills: `editing`, `explore-area`, `impact-analysis`, `web-research`
- [ ] no kept skill references removed tools such as `edit_symbol` or `query_metrics`
- [ ] plugin workflow copies only the intended shipped skills
- [ ] plugin README advertises the correct skill count and names
- [ ] no hook files are changed in this pass

## Risks

- Plugin workflow drift if the copy list and README are updated but a skill deletion is missed
- Stale references in other plugin docs if the README is updated but other docs still describe the removed skill set
- Over-trimming public skills if users relied on one of the removed report-template skills; the remaining tool descriptions and kept skills must cover the common cases
