# Razorback

This file is the source of truth for razorback-specific workflow policy in Julie.
Do not duplicate this policy in `AGENTS.md` or `CLAUDE.md`; point to this file
instead when a harness-specific doc needs to mention razorback behavior.

## Model Routing

**Default policy:** Use the cheapest tier that can do the work safely. Escalate
on ambiguity, repeated failure, weak tests, hidden invariants, or high blast
radius.

| Tier | Use for | Codex | Claude | OpenCode |
|---|---|---|---|---|
| Strategy | Planning, architecture, decomposition, lead review, finding triage | GPT-5.5 medium/high | Opus or Sonnet, based on risk | Strongest available reasoning model |
| Implementation | Bounded worker tasks from a clear plan | GPT-5.4-mini high | Sonnet or Haiku for boxed-in edits | Fast implementation model |
| Mechanical | Docs, fixtures, rote edits, formatting, manifests | GPT-5.4-mini low/medium | Haiku or Sonnet low-cost equivalent | Fastest reliable model |
| Coupled implementation | Bounded but cross-file work with some coupling | GPT-5.4-mini xhigh, or strategy tier | Sonnet high or Opus | Stronger implementation model |
| Escalation | Security, subtle correctness, high-blast-radius refactors, weak tests, repeated worker failure | GPT-5.5 high/xhigh | Opus | Strongest available reasoning model |

If a harness cannot choose models or reasoning per agent, use `inherit` and note
that limitation in the plan or worker report.

## Worker Eligibility

Use implementation-tier workers only when all are true:

- The task has clear acceptance criteria.
- File ownership is narrow and non-overlapping.
- The expected change is local.
- The relevant behavior has a narrow verification scope.
- The task does not depend on hidden shared invariants.

Do not use implementation-tier workers unattended for:

- shared database or workspace lifecycle behavior
- search ranking, scoring, tokenization, or query semantics
- daemon, watcher, restart, or concurrency behavior
- parser extraction edge cases across languages
- public API contract changes with many callers
- weak or missing tests
- review findings involving subtle correctness

## Coupled Lanes

For coupled but still bounded implementation, choose one:

- use the coupled implementation tier for the worker
- keep the work in the lead session
- split strategy-tier investigation from implementation-tier edits

## Lead Duties

The lead must:

- assign disjoint write scopes
- provide worker verification ceilings
- review assumptions, not only diffs
- inspect tests for meaningful assertions
- run affected-change verification after batches
- do final integration review on the strategy tier

## Escalation Triggers

Escalate to strategy or escalation tier when:

- two worker attempts fail review
- one worker failure reveals hidden invariants
- tests pass but the lead sees a plausible second-order bug
- a fix touches shared state, lifecycle, indexing, query behavior, or public API
- the plan no longer matches codebase reality

## Verification

Use Julie's test hierarchy from `AGENTS.md` and `docs/TESTING_GUIDE.md`.
Workers run the assigned narrow scope. The lead owns affected-change,
specialist, and branch-gate verification.
