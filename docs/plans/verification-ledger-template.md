# Verification Ledger Template

Use this section in plan docs to capture proof for every required test scope.

## Verification Ledger

Record one row per verification run. Every column is required. Leave this table
empty until a command has actually run or evidence has actually been reused.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|

## Example Rows

These rows show the expected shape. Do not copy them into plan evidence.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Example: worker exact test for docs contract passes before handoff | `cargo nextest run --lib docs_contract_tests_verification_ledger_template_is_operational 2>&1 \| tail -10` | worker-red-green | example-sha | pass | 2026-05-03T15:20:00Z | no |
| Example: the lead runs one optional broad diagnostic before the final gate | `cargo xtask test dev` | dev | example-sha | pass | 2026-05-03T15:42:00Z | no |
| Example: the final branch gate runs after code freeze | `cargo xtask test full` | full | example-sha | pass | 2026-05-03T16:35:00Z | no |

## Reuse Rule

You may reuse evidence only when all of the following are true:

1. The required `Scope Label` matches.
2. The reused row already has `Result` set to `pass`.
3. The tested commit is an ancestor of the current commit.
4. The intervening diff cannot affect the command. Evidence-only changes under `.memories/`, `docs/plans/`, or `docs/findings/` qualify only when the command does not consume those files.

When reusing evidence, add a new row with `Evidence Reused` set to `yes`. Keep the tested SHA in `Commit SHA` and record the current SHA in `Result`. Do not rerun a gate solely because a later commit recorded its evidence.

## Broad gate limit

Run a broad gate once after code freeze. If it fails, use exact tests for in-scope fixes, then allow one retry. Stop after a second broad failure and report the open gate. A third run requires an explicit owner decision.
