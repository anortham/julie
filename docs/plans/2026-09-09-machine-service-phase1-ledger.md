# Verification Ledger Template

Use this section in plan docs to capture proof for every required test scope.

## Verification Ledger

Record one row per verification run. Every column is required. Leave this table
empty until a command has actually run or evidence has actually been reused.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Service discovery, auth, status log, JSON API, and idle exit | `cargo test --lib http_api` | worker-exact | 48a2e07d63bc5d03e534557872817d98799f8e60 | pass | 2026-09-09T21:28:33Z | no |
| Machine service Task 1 worker ceiling tests and build pass | `cargo build && cargo nextest run --lib tests::service::` | worker-ceiling | 48a2e07d63bc5d03e534557872817d98799f8e60 | pass | 2026-09-09T21:26:55Z | no |
| MCP over HTTP Streamable server/discover, tools/list, and tools/call | `cargo nextest run --lib tests::service::mcp_http` | worker-exact | b9970f701c40ea4fc2e15b50d53c518b2605f63d | pass | 2026-09-09T21:37:46Z | no |
| Client connector auto-spawn/version mismatch and stdio shim forwarding | `cargo nextest run --lib tests::service::client tests::service::shim` | worker-exact | b9970f701c40ea4fc2e15b50d53c518b2605f63d | pass | 2026-09-09T21:39:00Z | no |
| Service control, shutdown endpoint, and dashboard mount | `cargo test --lib tests::service::control` | worker-exact | bb2719d4319e2c3adf1d59a6d249dc5fda984416 | pass | 2026-09-09T21:51:14Z | no |
| Machine service Task 4 worker ceiling suite and build | `cargo build && cargo test --lib tests::service::` | worker-ceiling | bb2719d4319e2c3adf1d59a6d249dc5fda984416 | pass | 2026-09-09T21:52:10Z | no |
| Service budget <=600 lines and zero banned coordination words | `cargo test --lib tests::service::budget` | worker-exact | 7922e1b1cd97a99ef1c26a5affdb45906af3ed75 | pass | 2026-09-09T21:56:39Z | no |
| Multi-process lifecycle: shim start, stale json replace, version mismatch, stop | `cargo test --lib tests::service::process -- --nocapture --test-threads 1` | worker-exact | 7922e1b1cd97a99ef1c26a5affdb45906af3ed75 | pass | 2026-09-09T21:55:41Z | no |
| Machine service Task 5 worker ceiling suite and xtask buckets | `cargo xtask test bucket service && cargo xtask test bucket service-process` | worker-ceiling | 7922e1b1cd97a99ef1c26a5affdb45906af3ed75 | pass | 2026-09-09T21:58:39Z | no |
| Three-host gate across Claude Code, Codex, and Cursor on HTTP and stdio | `claude mcp`, `codex exec`, `agent --print` (see findings doc) | lead-gate | facb50d9 | pass | 2026-09-09T22:20:00Z | no |
| Final dev tier regression gate with service buckets | `cargo xtask test dev` | dev-tier | facb50d9 | pass | 2026-09-09T22:28:16Z | no |
| Lead review fixes: MCP idle activity, shim reconnect, reqwest json-only | `cargo nextest run --lib tests::service::` | lead-changed | 3e2a6505 | pass | 2026-09-09T22:41:00Z | no |
| Dev tier at the review-fix commit (facb50d9 above is not on the branch; it was rewritten) | `cargo xtask test dev` | dev-tier | 3e2a6505 | pass | 2026-09-09T22:50:06Z | no |

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
