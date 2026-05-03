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
| Example: worker exact test for docs contract passes before handoff | `cargo nextest run --lib docs_contract_tests_verification_ledger_template_is_operational 2>&1 \| tail -10` | worker-exact | example-sha | pass | 2026-05-03T15:20:00Z | no |
| Example: diff-scoped bucket selection is recorded for lead validation | `cargo xtask test changed` | lead-changed | example-sha | pass | 2026-05-03T15:42:00Z | no |
| Example: expensive search-quality gate is documented once per HEAD | `cargo xtask test dogfood` | lead-expensive-gate | example-sha | pass | 2026-05-03T16:35:00Z | no |

## Reuse Rule

You may reuse evidence only when all of the following are true:

1. The required `Scope Label` matches.
2. The `Commit SHA` matches the current HEAD exactly.
3. The reused row already has `Result` set to `pass`.

When reusing evidence, add a new row with `Evidence Reused` set to `yes` and record the reused command and commit SHA.
