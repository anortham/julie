# Dependency upgrade audit and measured pilot design

## Summary

Run a measured dependency audit for Julie's core runtime and storage stack, focusing on packages whose upstream changes in the last 6+ months could deliver direct product benefit. Produce a ranked shortlist, then implement and test one or two contained upgrades with the best payoff-to-risk ratio.

## Why

Julie depends on a few core libraries that shape search quality, indexing behavior, MCP transport, workspace lifecycle, and vector storage. Some of those dependencies are behind current upstream releases. We want to know which upgrades buy us something meaningful for Julie, not churn for its own sake.

## Scope

### In scope

- Audit these dependency areas first:
  - `tantivy`
  - `sqlite-vec`
  - `rusqlite`
  - `rmcp`
  - `notify`
  - `tokio`
- Add other dependencies only if they are clearly part of Julie's core search, database, daemon, or MCP surface and have meaningful upstream changes.
- Research upstream releases from roughly the last 6+ months.
- Rank upgrade candidates by direct Julie impact, migration risk, and test cost.
- Implement and test one or two contained upgrades in this same effort.

### Out of scope

- `tree-sitter` core upgrades.
- Tree-sitter grammar upgrades.
- Broad dependency sweeps that bump most packages in one pass.
- Version churn with no direct Julie payoff.

## Current known versions

From the current manifests:

- `tantivy = "0.22"`
- `sqlite-vec = "0.1"`
- `rusqlite = "0.37"`
- `rmcp = "1.2"`
- `notify = "8.2"`
- `tokio = "1.47.1"`

Tree-sitter is intentionally excluded from this effort, even though it is present in both root and extractor manifests.

## Julie usage hotspots

The audit should stay tied to where Julie uses these packages today.

- `tantivy`: core search engine, tokenizer, query building, ranking-related tests under `src/search/**` and `src/tests/tools/search/**`
- `sqlite-vec`: vector registration and vector search paths under `src/database/**` and embedding dependency tests
- `rusqlite`: database access layer across workspace, symbol, vector, revision, projection, and repair code
- `rmcp`: handler, tool response model, daemon IPC session, and MCP-facing tests
- `notify`: watcher implementation and watcher integration tests
- `tokio`: daemon lifecycle, async runtime, IPC, watcher coordination, and test harness plumbing

## Research deliverable

Produce a short findings document or sectioned summary with one entry per candidate dependency:

1. Current Julie version
2. Latest realistic target version
3. Upstream changes in the last 6+ months that matter to Julie
4. Why those changes help Julie, or why they do not
5. Migration risk, including API churn and test blast radius
6. Recommendation:
   - upgrade now
   - defer
   - split into a dedicated plan

## Decision rubric

Prefer upgrades that meet most of these conditions:

- Clear Julie-facing benefit, such as search correctness, performance, stability, or MCP compatibility
- Contained migration surface
- Strong release-note evidence
- Testable with narrow, existing coverage before the batch gate

Defer upgrades that meet any of these conditions:

- They require broad architecture changes
- They drag in large transitive changes with weak payoff
- They need a separate design pass, like tree-sitter

## Execution phases

### Phase 1, inventory and research

- Confirm current pinned versions from `Cargo.toml` and any related manifests.
- Review upstream release notes for the target dependencies.
- Map release-note changes to Julie code paths and tests.
- Produce a ranked shortlist.

### Phase 2, choose pilot upgrades

- Pick one or two candidates with the best payoff and manageable migration cost.
- Avoid coupling two high-risk upgrades together.
- If all promising candidates turn out to be risky, stop and report that instead of forcing an upgrade.

### Phase 3, implementation and validation

- Update dependency versions and any required Julie code.
- Follow TDD for any bug fix or behavior change discovered during the upgrade.
- Run the narrowest tests first for touched areas.
- After the local loop, run `cargo xtask test changed`.
- After the batch is complete, run `cargo xtask test dev` once.

## Expected files to touch

This work will likely touch some subset of:

- `Cargo.toml`
- `Cargo.lock`
- Search code under `src/search/**`
- Database code under `src/database/**`
- MCP handler or daemon code under `src/**`
- Existing tests under `src/tests/**`
- A findings or implementation note under `docs/plans/` if the research yields enough material to preserve

## Acceptance criteria

- [ ] We identify which dependency bumps have direct Julie payoff, not version churn.
- [ ] We produce a ranked shortlist with benefit, risk, and recommendation for each candidate.
- [ ] We leave tree-sitter and grammar upgrades out of this effort.
- [ ] We implement and test one or two contained upgrades, unless research shows no safe candidate is worth doing.
- [ ] We validate upgraded code with narrow tests first, then `cargo xtask test changed`, then `cargo xtask test dev` once for the completed batch.
- [ ] We call out any upgrade that deserves its own follow-up design.

## Risks and guardrails

- Release notes can overstate practical benefit. Tie every claim back to Julie code paths.
- Cargo resolver changes can surface transitive churn. Keep the pilot set small.
- Search and database upgrades can shift behavior in ways that only show up in targeted tests. Do not skip the narrow loop.
- MCP and runtime upgrades can expand blast radius fast. Keep those separate from search or storage changes if needed.

## Open decisions to resolve during planning

- Which one or two upgrades have the strongest payoff with manageable migration risk?
- Whether the research summary belongs only in the session report or also in a checked-in findings doc.

## Implementation handoff notes

If this moves to implementation planning, the plan should break the work into:

1. release-note audit and shortlist
2. pilot upgrade A
3. pilot upgrade B, only if independent and worth doing
4. verification and summary
