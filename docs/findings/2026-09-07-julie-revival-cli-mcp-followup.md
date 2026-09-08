# Revival follow-up: extractor contracts, CLI parity, and stateless MCP

Status: recommendations and updated requirements, not an implemented migration.

This supplements [the initial assessment](2026-09-07-julie-revival-assessment.md). It updates the runtime direction to account for the released stateless MCP protocol and makes CLI flexibility a first-class acceptance criterion.

## Restore capabilities at the right boundary

Upstream removed Rust interfaces Julie depended on; that does not mean all underlying extraction capabilities disappeared. Canonical extraction functions replace the old manager. Upstream source-body removal was intended to eliminate copying and storage work that its artifact consumers did not need. Julie is a different consumer and must preserve its body search, embeddings, navigation and edits.

Recommended division:

| Need | Owner and approach |
|---|---|
| Symbols and structured facts | Use upstream public canonical extraction functions through a Julie adapter |
| Source snippets and searchable symbol bodies | Julie owns source snapshots and derives text from verified spans; avoid mandatory per-symbol body copies upstream |
| Grammar selection and syntax access | Add a small supported upstream API instead of exposing internal registry/factory modules |
| Ranking, text budgets, embedding inputs | Julie policy, tested against its own tool outcomes |
| Cross-version compatibility | Upstream consumer contract tests plus Julie integration gates before adopting a release |

An important correction to the initial assessment: syntax diagnostics alone would not preserve Julie's edits. `rewrite_symbol` uses the syntax tree to locate body/signature boundaries, while rename walks identifier nodes to avoid strings and comments. The upstream API must support those needs, whether through a bounded syntax API or a documented parse-result interface. The exact Rust interface needs design review. Do not restore the entire old manager or export private modules merely to make old imports compile.

The existing implementation checks input syntax before editing. Rename rejects existing file diagnostics; rewrite checks diagnostics affecting the target symbol. The inspected rewrite preparation does not reparse the edited output. Rejecting newly introduced syntax errors is a worthwhile proposed improvement, but is not an existing safeguard that the migration can claim to preserve.

Evidence: [upstream output changes](../../../julie-extractors/docs/contracts/extraction-output-changes.md), [upstream audit](../../../julie-extractors/docs/findings/2026-09-04-architecture-and-performance-audit.md), [rewrite implementation](../../crates/julie-tools/src/editing/rewrite_symbol.rs), [rename/refactoring implementation](../../crates/julie-tools/src/refactoring/mod.rs).

## CLI flexibility is a required product feature

The user wants to build a debug binary and exercise all tool behavior without registering it in an interactive MCP session or restarting that session after each change.

Julie already has named CLI commands and generic `tool <name> --params <json>` dispatch for 13 tools. This is a useful starting point. However, the CLI directly calls tool implementations while MCP handlers add other behavior, and CLI bootstrap uses standalone storage. That is not proof of full execution parity. See [generic dispatch](../../src/cli_tools/generic.rs), line 35, [standalone execution](../../src/cli_tools/mod.rs), line 250, and [MCP edit handler](../../src/handler/tools/edit_file.rs), line 26.

Miller's CLI exposes explicit retrieval modes and machine-readable capability information. Julie should match the flexibility needed for development without copying Miller's command names or large dispatcher structure. See [Miller CLI capabilities](../../../miller/src/Miller.Server/Cli/CliCapabilities.cs) and [CLI dispatch](../../../miller/src/Miller.Server/Cli/CliDispatch.cs).

Proposed acceptance criteria:

- CLI and MCP invoke the same request-execution layer, including validation, workspace resolution, edit safety, error classification and telemetry.
- Every tool and parameter is reachable, through ergonomic named flags or generic structured invocation.
- Structured input can come from a file or stdin, avoiding shell escaping for source edits and large requests.
- Tool/schema/capability discovery is available from the binary.
- Output has stable JSON and readable forms, meaningful exit status, and stderr diagnostics that do not corrupt stdout.
- Workspace, freshness policy, retrieval mode, budgets and semantic readiness are explicit where relevant.
- A separate isolated-index option is intentional; CLI versus MCP does not silently choose different source/index state.
- Saved requests can be replayed; an optional batch runner can amortize startup for measurements while keeping workspace identity explicit per request.
- Automated subprocess tests exercise the real MCP transport and compare results with CLI execution. Direct CLI tests alone cannot prove wire compatibility.

These are requirements for the revival, not claims that every option exists today. Rebuilding the binary remains necessary after Rust changes; restarting an interactive coding session should not be necessary to verify behavior.

## Stateless MCP changes the runtime design now

The official protocol revision is `2026-07-28`. Its released core removes the initialize handshake and protocol sessions. Requests carry their own protocol/client metadata; discovery is optional. Application state remains allowed and should be addressed explicitly. See the [official release announcement](https://blog.modelcontextprotocol.io/posts/2026-07-28/) and [specification](https://modelcontextprotocol.io/specification/2026-07-28).

Julie currently declares rmcp `1.6` in the root and julie-core manifests. Official rmcp 3.x supports the new revision; the releases page lists 3.0.1 as the latest release checked on September 7. Upgrade to a verified stable 3.x release as a dedicated migration with a lockfile and integration checks. Do not treat a manifest bump as protocol conformance. Sources: [SDK releases](https://github.com/modelcontextprotocol/rust-sdk/releases), [3.x migration guide](https://github.com/modelcontextprotocol/rust-sdk/discussions/969).

The SDK migration changes handler response types and metadata models. It also retains version-gated legacy interoperability. Test both the modern request lifecycle and the older lifecycle used by clients we intend to support. Avoid adding new dependencies on deprecated Roots, Sampling or Logging behavior.

Julie-specific consequences:

1. Move readiness/index startup out of exclusive dependence on `on_initialized`. That hook currently performs startup work in [handler.rs](../../src/handler.rs), line 2789. A valid first modern request must work without prior initialization or discovery.
2. Resolve workspace identity from each request or an explicit fixed server configuration. Avoid a mutable session-wide current-workspace binding.
3. Keep indexes, watchers, writer locks and embedding services outside request-handler lifetime. Stateless requests must neither recreate those services each time nor own them implicitly.
4. Preserve stateful operations through explicit, scoped handles where needed. Review pagination/spillover, prepared edits and long-running work for dependence on the original process/session, expiry and stale-index behavior.
5. Make edit retries conflict-aware. Removing protocol sessions does not make filesystem writes idempotent or eliminate concurrency races.

This revises the first assessment's session-centered ownership proposal. Writer authority should belong to the workspace runtime and be recoverable independently of any one request. A stdio process may host that runtime, but request handling must not depend on an MCP initialization handshake or a particular request-handler instance. This does not require an HTTP deployment or a new global daemon.

## Revised implementation ordering

Resolve the supported upstream syntax contract first. Design the shared CLI/MCP request layer and workspace lifetime against the stateless protocol before hardening the old session model. Carry out extractor and SDK migrations in separately verifiable changes; then implement complete CLI parity, lifecycle recovery, and the measured semantic replacement.

No dependencies or production source were changed in this follow-up. Evidence is source inspection and current official protocol/SDK documentation. No build or regression suite was run.
