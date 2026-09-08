# Julie revival assessment

Date: 2026-09-07. Status: investigation and recommendation, not an approved implementation design.

## Recommendation

Revive Julie as a retrieval-focused code intelligence tool. Keep Tantivy, retain optional semantic retrieval, and make useful answers available with bounded latency and honest freshness. Update extraction without importing a new storage architecture in the same change. Harden session ownership and recovery before comparing products.

The strongest potential advantage is a compact native path from source extraction to well-ranked, concise answers. Rust alone does not prove a memory or speed advantage. The comparison must measure complete agent tasks as well as individual queries.

The user's request supersedes the old retirement-only direction. Continuous testing is excluded. No production changes, dependency upgrade, or release were performed in this assessment.

## Source state and evidence limits

| Repository | Inspected main commit | Initial state |
|---|---|---|
| Julie | `0432158c173c30ae2f76d92773380c744ddc6542` | Clean, v7.18.1 |
| julie-extractors | `b7a7c62a061c707dd5809d4c401bf36ed286fbbe` | Clean, local v2.40.6 tag |
| julie-semantic-sidecar | `9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59` | Clean, v0.1.0 |
| Miller | `40f7c71a7703b369998903e5f9a536fda006da57` | Existing documentation and memory changes |

Existing worktrees were inventoried. Two clean sidecar worktrees contain unique fixes outside main: `35c8f13` activates a prepared model in a live broker, and `5c41f1d` tightens outcome-evidence validation. These need deliberate review and integration before qualification of a revived Julie. Extractor worktrees include existing audit documents; Miller has active worktrees and user research. None were modified.

Evidence comes from current source, local Git state, historical evaluations, read-only telemetry inspection, and one existing-binary CLI smoke check. Historical results are not fresh verification of these commits. Local tags were verified; remote release publication was not.

## 1. Updating julie-extractors

This is a migration from v2.34.3 to the locally available v2.40.6, not a manifest-only update.

Julie repeats the pin in the root manifest and five crate manifests. Upstream removed `ExtractorManager`, `Symbol.code_context`, public parser getters, and access to internal modules. Public canonical extraction functions and root-level fact types replace the old entry points. See [upstream migration contract](../../../julie-extractors/docs/contracts/extraction-output-changes.md), especially line 137, and [public exports](../../../julie-extractors/crates/julie-extractors/src/lib.rs), line 102.

The first implementation slice should:

1. Put canonical extraction behind one Julie-owned adapter and update all six dependency pins together.
2. Separate Julie's source/body representation from upstream symbol facts. Tantivy currently indexes `symbol.code_context`; losing it would quietly damage body search and embedding inputs. See [projection/apply.rs](../../crates/julie-index/src/search/projection/apply.rs), line 380.
3. Preserve parse validation for edits. `rewrite_symbol` and refactoring use the removed parser getter. Establish a supported validation API or adapter; do not remove validation to make the build pass. See [rewrite_symbol.rs](../../crates/julie-tools/src/editing/rewrite_symbol.rs), line 296, and [refactoring/mod.rs](../../crates/julie-tools/src/refactoring/mod.rs), line 460.
4. Carry new receiver/type evidence through storage and resolution. The current identifier SQL mapping drops the new `receiver_type` field. Audit the corresponding pending-relationship path too. See [identifier inserts](../../crates/julie-core/src/database/bulk/identifiers.rs), line 43.
5. Check discovery, visibility, spans, and language configuration against upstream capabilities. Upstream's 40 entries count JSX/TSX variants and qmldir; this is not four new parsers relative to Julie's advertised 36 languages. F# and additional XML project-file extensions are concrete additions.
6. Update extraction contract and tag identity and rebuild derived indexes. Existing symbols, spans and parent identities cannot be assumed compatible. See [engine_version.rs](../../src/tools/workspace/indexing/engine_version.rs), line 22.

Julie already retains source regions, structural facts, types, literals, type arguments, diagnostics, and complexity metrics. It does not need all modern facts rebuilt from scratch. Retaining a new fact is also different from making navigation or ranking use it.

The upstream CLI/artifact interface is a credible alternative, especially if both tools eventually share extraction. Adopting it now would couple the dependency migration to a larger storage and lifecycle change. Keep that decision separate.

Acceptance evidence should include extraction round trips, source-body search, edit validation, receiver disambiguation, new file discovery, and forced-reindex behavior. Then run the extractor-dep-integration bucket and relevant dev/system/dogfood gates on the implementation tree. No such gates were run during this investigation.

## 2. Native sidecar and the future of semantics

Keep semantics. Evaluate the native sidecar as its replacement runtime, with model choice as a separate decision.

| Concern | Current Python integration | Native sidecar at inspected main |
|---|---|---|
| Encoder | CodeRankEmbed, 768 dimensions | BGE-small, 384; Qwen3 0.6B, served at 512 |
| Packaging | Python environment, uv and PyTorch dependencies | Native runtime and pinned, checksum-verified model downloads |
| Hardware | CUDA, DirectML, MPS, CPU selection | CPU; Metal/Vulkan implementation; CUDA excluded from v0.1.0 |
| Shared use | Resident embedding host already exists | Broker with bounded scheduling, model ownership, watchdog and accelerator lease |
| Quality parity | Existing code-search model and thresholds | Not established against CodeRankEmbed on Julie tasks |

Native is not model-free or necessarily smaller in every configuration. Qwen's pinned file is about 1.2 GB and its native dimension is 1024 before serving at 512. BGE has a 512-token limit; Qwen's manifest allows much longer text. Input construction and truncation therefore belong in the quality comparison. See [native model manifest](../../../julie-semantic-sidecar/src/manifest.rs), line 84.

The retained CPU qualification reports BGE at 60.40 items/s and 177.8 MB post-benchmark RSS; Qwen at 2.55 items/s and 1,268 MB. These are synthetic measurements from a binary built at `60998de`, not current-head, Python-comparison, peak-memory, or agent-quality results. Linux Vulkan was explicitly unverified on physical hardware in that report. See [qualification report](../../../julie-semantic-sidecar/docs/findings/2026-09-04-runtime-qualification.md), line 52.

The protocol methods are close enough to reuse part of the existing client design, but integration still needs native preparation and launch, broker reconnect, late attachment, and health reporting. Julie currently omits native Metal/Vulkan and encoder-provenance fields from its health schema. Its vector migration checks model ID, dimensions and text-format version; extend this to the complete encoder identity, including checksum, pooling, normalization and instruction policy. Do not compare vectors from incompatible encoders while rebuilding. Recalibrate similarity thresholds.

Sources: [Julie sidecar protocol](../../crates/julie-pipeline/src/embeddings/sidecar_protocol.rs), line 58; [embedding rebuild logic](../../crates/julie-pipeline/src/embeddings/pipeline.rs), line 195; [similarity threshold](../../crates/julie-index/src/search/similarity.rs), line 14.

Deleting semantics would remove conceptual retrieval, hybrid search, related-symbol suggestions in `deep_dive`, and the similarity fallback in `fast_refs`. Similarity must remain labeled as a suggestion, never as a proven reference.

There is historical evidence of value. The [May 23 scorecard](../eval/semantic-value/results/2026-05-23T17-47-28Z.md) records 30 cases:

| Retrieval | Correct first result | Correct result in top five | p95 query latency |
|---|---:|---:|---:|
| Lexical | 26.7% | 43.3% | 101 ms |
| Semantic | 66.7% | 90.0% | 118 ms |
| Hybrid | 53.3% | 73.3% | 117 ms |

This corpus emphasizes concepts and implementations, was run on an older macOS checkout, and includes lexical hits in evaluation artifacts. It supports preserving and re-testing semantics, not promising those gains today. Hybrid underperforming semantic retrieval is a reason to examine routing and fusion rather than assume that combining results always helps.

Recommended product behavior: a genuine zero-semantic-work lexical mode, plus a full mode with background embedding and bounded query work. Exact symbol/path requests should not need inference. Conceptual requests can use semantics when ready. Report degradation explicitly and recover in the same session. Retain Python temporarily as the comparison baseline until native quality and hardware requirements are met.

## 3. Runtime, concurrent agents, and worktrees

The central daemon has already been removed. Current stdio sessions run in process. Canonicalized workspace roots determine independent index directories and leader locks. SQLite is canonical; Tantivy is a derived projection. The runtime has a per-workspace mutation gate and recovery machinery. Reuse those foundations.

Three concrete gaps deserve priority:

| Finding | Current evidence | Required behavior |
|---|---|---|
| Followers cannot use edit tools | [edit_file handler](../../src/handler/tools/edit_file.rs), line 33, rejects followers before preparing even a dry run | Every agent can preview; supported writes from any session are serialized safely |
| Leadership is fixed at session construction | [LeadershipState](../../src/leadership.rs), line 23, holds an immutable optional lock; [server entry](../../src/server_in_process.rs), line 208, elects once | A surviving session can acquire ownership and repair/index after the owner exits |
| Shared-host startup timeout does not attach the later result | [server entry](../../src/server_in_process.rs), line 239, passes None after the default five-second wait | Lexical startup stays available; semantic readiness can arrive and recover later |

The last issue has an important qualification. [embedding_init.rs](../../src/handler/embedding_init.rs), line 56, also has a legacy per-workspace lazy initialization route. It is inaccurate to claim that every timed-out session stays lexical-only forever. The problem is multiple initialization routes without one clear recovery lifecycle.

Likewise, the test named [leader handoff recovery](../../src/tests/integration/t9_handoff_recovery.rs), line 86, proves that Tantivy can be rebuilt from SQLite. It does not prove that a live follower takes over automatically. Tests of OS lock release are useful but do not substitute for that session-level scenario.

My preferred design direction is:

- One index writer and watcher per canonical worktree root, with read-only query handles for other sessions.
- Explicit session states for following, acquiring ownership, repairing and ready. Only the OS lock grants writer authority.
- A short-lived cross-process source-edit lock shared by all Julie sessions, separate from index ownership. Check original content inside that lock and use stable ordering for multi-file edits. The existing [edit-lock ADR](../adr/ADR-0004-per-path-edit-lock-invariant.md) covers in-process locking; cross-process coordination requires an explicit extension. External editors cannot be forced to honor Julie locks, so conflicts and multi-file rollback limits must remain honest.
- SQLite-to-Tantivy revisions that make stale results visible and allow crash recovery to rebuild the projection.
- One optional model service per compatible encoder and machine resource policy, with interactive queries prioritized over indexing batches. It owns inference, not workspace routing or source files.
- Machine-wide limits on expensive indexing and embedding jobs so ten worktrees cannot each consume the whole machine.

Keep worktrees physically independent first. A branch switch within one root triggers reconciliation; a different worktree gets a separate identity. Do not key indexes only by repository or branch name. Reuse immutable extraction/embedding content later only if disk and indexing measurements justify it, keyed by content and full extraction/encoder identity.

A small workspace worker independent of a client is the alternative if transferable session ownership becomes too complex. That would avoid owner churn but add process supervision and idle shutdown. Rebuilding a global daemon with routing, transport and shared mutable workspace state is the highest-risk option and is not justified by the current evidence.

Architecture risk is high for lifecycle and concurrent edits. The next design must settle the owner transition, write authority and failure semantics before implementation. Test through real client processes: two readers, simultaneous previews/edits, owner exit, owner kill during projection update, semantic-host failure, branch switch, worktree removal, and independent worktrees edited concurrently. Assert freshness and correct files, not merely absence of crashes.

## 4. Where the stack may favor Julie

Julie has a dedicated Tantivy index with positional text fields, exact metadata filters, relationship text and code-aware ranking. Miller uses SQLite FTS5 candidate retrieval, word and trigram arms, and C# BM25 ranking. See [Julie schema](../../crates/julie-index/src/search/schema.rs), line 60, and [Miller search index](../../../miller/src/Miller.Indexing/FtsSymbolSearchIndex.cs), line 8.

Tantivy makes rich lexical ranking and high query concurrency a plausible area for Julie to compete. Its independent query index can also keep retrieval work separate from relational queries. The cost is duplicated storage and a projection-consistency problem that Miller can handle differently. Neither benefit nor cost should be assumed negligible.

Tantivy's upstream documentation confirms positional/phrase queries, configurable tokenizers, mmap storage and incremental indexing. SQLite FTS5 also supports sophisticated full-text retrieval; BM25 is not exclusive to Tantivy. These library capabilities are not Julie-versus-Miller benchmarks. Sources: [Tantivy](https://github.com/quickwit-oss/tantivy), [SQLite FTS5](https://www.sqlite.org/fts5.html).

Other plausible advantages are a native extraction-to-query path, a smaller retained runtime working set, and a narrow tool workflow with concise answers. These remain hypotheses. Both products use the same extractor family, Miller's tokenizer derives from Julie, and both have semantic/context capabilities. Do not market those shared capabilities as exclusive advantages.

## 5. Lighter and faster without reducing the contest

Aim for less work per useful answer, not fewer supported tasks. Keep search, symbols/context, references, call paths, impact and safe editing. Avoid expanding Julie's product scope during revival.

Measure cold readiness, warm latency, memory, background CPU, index size, freshness after edits, and cost under concurrent sessions. A lexical-only configuration is useful in its own right, but the requested full-function comparison must also run with embeddings genuinely ready and populated.

Keep full mode easy to qualify. Health should state extractor identity, index revision, projection lag, writer state, vector coverage, exact encoder and hardware backend, queue pressure, and why any capability is unavailable. A fast response silently missing semantics is not a full-mode result.

## 6. Use telemetry to beat both current products

The first target should be correct completed tasks per unit of time and agent context, with no stale or wrong-worktree answers.

Available telemetry is uneven. The local Julie standalone database had a `tool_calls` table with zero rows, and this host had no `~/.julie` directory. That does not mean historical telemetry is gone from other machines. Miller's onboarding statistics for the Julie workspace describe Miller calls against Julie, not Julie calls.

Miller's local telemetry is substantial but spans many versions. For its own root over the sampled 30-day period, `inspect:full` averaged 1,184 ms across 7,274 calls. The exact `1.28.1+40f7c71a7703` slice averaged 489 ms across 111 calls. These uncontrolled samples illustrate why mixed-version averages cannot establish current performance or a cross-product winner.

The existing [Miller usefulness review](../../../miller/docs/findings/2026-09-07-agent-usefulness-review.md) and [validation](../../../miller/docs/findings/2026-09-07-agent-usefulness-validation.md) show another useful lesson. Five tasks were correct with both approaches, but one oversized impact response dominated Miller's output cost. Removing that task reduced the reported output ratio to 1.56x. Optimize answer shape and retrieval intent, not a misleading overall average.

For revived Julie:

- Preserve successful narrow answers; do not automatically turn every lookup into a graph walk or semantic query.
- Return the body or evidence needed for the task within a real output budget. Do not spend most of the budget on neighboring symbol lists.
- Separate exact references, heuristic links and semantic suggestions in output so agents know what the result proves.
- Make useful empty results explain coverage, exclusions and a concrete recovery path.
- Record request mode, result size, freshness, queue/startup cost and failure reason. Record completed-task outcomes separately; successful tool calls do not prove successful work.
- Replay representative misses as held-out tasks before changing ranking. Keep evaluation answers and result artifacts outside indexed corpora.

## Comparison design

Use two complementary experiments. First compare complete products in their best supported full mode. Then run controlled variants to explain differences: lexical-only, alternative encoders, semantic-only and hybrid where supported. Match encoders when isolating search-engine effects; allow different encoders in the best-product contest and report the distinction.

Pin source commits, extractor versions, binaries, model files, hardware, indexing exclusions, agent model/prompt and budgets. Use independent agent sessions and randomize tool/task order. Run products separately for resource measurements, then a separate coexistence stress test. Include a raw-tools baseline.

| Workload | Evidence to collect |
|---|---|
| Exact names, fragments, literals, prose, conceptual queries | Correct first/top-five result, rank, false negatives, latency |
| Explain code, locate a defect, trace references, verified edit/rename | Correct task completion, wall time, calls, tool-output/model tokens, retries and shell fallback |
| Cold start and incremental updates | Time to lexical readiness, time to full readiness, time until edits become searchable |
| Concurrent agents and worktrees | Correct workspace, no lost acknowledged edits, post-failure recovery, p95/p99 latency |
| Sustained use and worktree churn | Peak/process-tree memory, background CPU, disk growth, cleanup and repeated indexing cost |

Use a mixed-language corpus, including projects outside Julie/Miller. Run discovery and representative feature checks across the complete advertised language set, with concept-specific exclusions verified. Choose budgets and pass criteria before running the comparison. Publish per-task results and failure distributions, not only a single winner score.

## Suggested sequence for approval

1. Restore a current full-function extraction baseline, preserving body search and safe editing.
2. Settle and implement session ownership, edit concurrency, same-session semantic recovery and observable freshness.
3. Integrate a reconciled native sidecar behind the provider boundary; compare it with the retained Python baseline and choose the model from task outcomes.
4. Run the controlled head-to-head and use observed failures to choose the next improvements.

Do not announce a faster or better Julie before those measurements. This assessment recommends the direction; it does not claim the revival has shipped.

## Verification in this session

- Checked main branches, commits, dirty states and existing worktree inventories; preserved unrelated work.
- Inspected source via Miller and checked critical API, model and evaluation claims against bounded source reads.
- Used the existing Julie release binary to search `LeadershipState` through the standalone CLI. It returned the correct definition and exit status 0. Its reported 26.1-second standalone elapsed time includes startup work and is not warm query latency. The command remained alive after printing its result before eventually exiting; no precise shutdown timing or cause was established.
- No compile, regression suite, fresh quality benchmark, or multi-process lifecycle qualification was run. This is evidence for an assessment, not an implementation pass.
- Added this assessment and a Goldfish revival brief/checkpoint. No production code or existing user research was changed.
