---
name: dead-code-audit
description: Use when auditing Julie for dead code, test-only code paths, stale helpers, graph gaps, or cleanup candidates before deleting, privatizing, or simplifying code.
allowed-tools: mcp__julie__manage_workspace, mcp__julie__fast_search, mcp__julie__fast_refs, mcp__julie__deep_dive, mcp__julie__blast_radius, mcp__julie__get_context, mcp__julie__spillover_get, Bash
---

# Dead Code Audit

Use Julie's index as evidence, then verify with Julie tools before any edit. A zero-reference row is a lead, not a verdict.

## Scope

This skill is for Julie development:
- dead code and stale helper audits
- code referenced only by tests
- public APIs with no product callers
- graph linkage gaps that look like dead code
- cleanup campaigns before deletion, privatization, or module splitting

Do not use this as a blind deletion workflow. Dynamic dispatch, trait impls, CLI entry points, MCP handlers, generated fixtures, parser registries, macro usage, and cross-language extraction paths all create false positives.

## Inventory

1. Get the current workspace id:

```text
manage_workspace(operation="stats")
```

2. Run the bundled inventory script from the repo root:

```bash
python3 .claude/skills/dead-code-audit/scripts/dead_code_inventory.py --workspace-id <workspace_id> --limit 40
```

Use `--db <path>` if the workspace is not daemon-backed. Use `--json` when another script will postprocess the result. The default report excludes fixtures, docs, examples, and data/config languages. Add `--include-fixtures` or `--include-data-languages` when that scope matters.

3. Also run early-warning signals:

```bash
./target/debug/julie-server signals --workspace . --fresh --limit 40
```

Build debug first if needed. Keep this read-only.

## Triage Labels

Assign one label to each candidate:

| Label | Meaning |
| --- | --- |
| `delete` | No product reachability, no external dispatch role, no useful test-only value |
| `make-private` | Used in product code, but exported wider than needed |
| `merge-into-caller` | Single product caller and no independent concept |
| `test-fossil` | Product symbol exists only to satisfy tests |
| `graph-gap` | Julie failed to link real product usage |
| `keep` | Valid entry point, trait hook, registry item, dynamic target, or public surface |
| `needs-design-review` | Risky or ambiguous enough to discuss before edits |

## Required Verification

For every candidate before recommending deletion:

0. Confirm the index is fresh:

```text
manage_workspace(operation="health", detailed=true)
```

If files changed since indexing, refresh or rebuild the debug index before trusting the report.

1. Resolve the exact symbol:

```text
fast_search(query="<name>", search_target="definitions", file_pattern="<narrow path>")
```

2. Check references with identifier fallback:

```text
fast_refs(symbol="<name>", include_definition=true, limit=200)
```

If the name is overloaded, disambiguate with the file path and rerun.

3. Inspect the symbol and its role:

```text
deep_dive(symbol="<name>", context_file="<path>", depth="context")
```

Look for trait implementations, `#[cfg(test)]`, CLI/MCP registration, parser factory registration, annotation-driven entry points, and cross-language extractor contracts.

4. Search for non-graph usage:

```text
fast_search(query="<symbol-name>", search_target="content", limit=50)
```

Check strings, config, route names, command names, JSON/TOML/YAML keys, shell scripts, CI files, macro sites, and plugin manifests.

5. Check impact before editing:

```text
blast_radius(file_paths=["<path>"], include_tests=true, max_depth=2)
```

If the candidate lives in shared indexing, database, search ranking, daemon, parser extraction, or MCP handling code, escalate to design review before deletion.

## Reading Inventory Sections

- **Test-Only Relationship Refs**: strongest fossil signal. Tests call it, product code does not. Still verify trait hooks and registries.
- **Zero Relationship Refs And No Production Identifier Hits**: deletion candidate pool. High false-positive risk for dynamic entry points.
- **Likely Graph Gaps**: not dead-code candidates. These are cases where raw identifiers exist in product code but relationship edges are missing. Common names such as `new`, `run`, `handle`, `setup`, and `to_string` are noisy here.
- **Cfg-Test Markers In Source Paths**: test helpers embedded in source files. Classify as test cleanup, not product dead code.

## False-Positive Traps

- `reference_score = 0` means no scored incoming signal, not unused.
- `Contains` relationships are structure, not usage.
- Name-only identifier hits are leads, not proof.
- Wildcard imports can hide test-only calls from graph output. Use targeted content search before calling a symbol definition-only.
- Metadata is language-specific. Interpret keys such as `test_role`, `is_test`, decorators, impl metadata, and test linkage with context.
- Public symbols can be entry points through MCP registration, CLI dispatch, web routes, framework callbacks, parser factories, trait implementations, DI, or runtime naming conventions.

## Deletion Rules

Before editing code:
- write or identify the regression test that protects live behavior
- remove tests that only preserve the fossil behavior
- run the narrowest relevant test first
- after a batch, the lead session runs `cargo xtask test changed`, then `cargo xtask test dev`

Never add a new test whose only purpose is to keep a questionable symbol alive.

## Report Format

```text
Dead Code Audit: <scope>

Candidate: <symbol> at <path>:<line>
Evidence:
- inventory section: <section>
- fast_refs: <prod refs>, <test refs>
- deep_dive role: <entry point / helper / trait hook / unknown>
- blast_radius: <summary>

Decision: <label>
Action: <delete / make private / merge / keep / review>
Verification: <test command>
Confidence: <1-100>
```
