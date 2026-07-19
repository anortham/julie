# Julie Extractors v2.16 Consumer Upgrade Verification

## Verification Ledger

Record one row per verification run. Evidence may be reused only when the
required scope label and commit SHA both match.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Clean v2.14 baseline | `cargo xtask test nano` | baseline-nano | `d3dcda40c16a8f93b2dc4a9de9bd20ac62e72295` | PASS: 2 buckets, 106.7s | 2026-07-18 | no |
| All direct consumers pin the tagged upstream release | `cargo nextest run -p xtask extractor_dependency_release_is_v2_16_0` | task-1-exact | `4ec00f8673cc724a1250f5d3a363388284521b1b` | PASS: 1 test | 2026-07-19T00:49:54Z | no |
| Engine version includes the upstream extraction contract | `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract` | task-1-exact | `4ec00f8673cc724a1250f5d3a363388284521b1b` | PASS: 1 test | 2026-07-19T00:50:51Z | no |
| Enrichment rows round-trip, replace, and delete atomically | `cargo nextest run --lib test_extractor_enrichment_domains_roundtrip_replace_and_delete` | task-2-exact | `f765789ea8e35a5e7a22eda17bfd87f35a6baf6d` | PASS: 1 test | 2026-07-19 | no |
| Migration 029 creates all enrichment tables | `cargo nextest run -p julie-core test_migration_029_adds_extractor_enrichment_tables` | task-2-exact | `f765789ea8e35a5e7a22eda17bfd87f35a6baf6d` | PASS: 1 test | 2026-07-19 | no |
| Workspace cleanup removes all owned enrichment rows | `cargo nextest run --lib test_delete_workspace_data_clears_all_owned_tables` | task-2-exact | `f765789ea8e35a5e7a22eda17bfd87f35a6baf6d` | PASS: 1 test | 2026-07-19 | no |
| External scan persists all v2.16 domains | `cargo nextest run --lib extract_scan_persists_v2_16_enrichment_domains` | task-3-exact | `38283c4ba4bf246ec4436736c68347380ab3fabb` | PASS: 1 test | 2026-07-19 | no |
| Watcher replacement converges all v2.16 domains | `cargo nextest run -p julie-runtime watcher_replaces_all_extractor_enrichment_domains` | task-3-exact | `38283c4ba4bf246ec4436736c68347380ab3fabb` | PASS: 1 test | 2026-07-19 | no |
| Foundation revision range passes the changed gate | `XTASK_CHANGED_PATHS="$(git diff --name-only 4ec00f86..HEAD)" cargo xtask test changed` | lead-changed | `38283c4ba4bf246ec4436736c68347380ab3fabb` | PASS: 33 buckets, 442.5s | 2026-07-19 | no |
| Patterns supports list, search, summary, and filters | `cargo nextest run --lib patterns_lists_searches_summarizes_and_filters_metadata` | task-4-exact | `f7275c4e1344eb2fe9909ff5de11cd88fd321b3c` | PASS: 1 test | 2026-07-19 | no |
| Patterns rejects malformed parameters | `cargo nextest run --lib patterns_rejects_invalid_parameters` | task-4-exact | `f7275c4e1344eb2fe9909ff5de11cd88fd321b3c` | PASS: 1 test | 2026-07-19 | no |
| Patterns reads the explicitly targeted workspace | `cargo nextest run --lib patterns_respects_target_workspace` | task-4-exact | `4aba2659a01bbb3910cc4697c4f16c4a8f8667f6` | PASS: 1 test | 2026-07-19 | no |
| Region search returns only lines inside allowed source regions | `cargo nextest run --lib fast_search_regions_returns_only_matching_source_region_lines` | task-5-exact | `bf9f55a35cd84a29c821b89e0978f680febdbd0a` | PASS: 1 test | 2026-07-19 | no |
| Region search rejects unknown regions and unsupported backends | `cargo nextest run --lib fast_search_regions_rejects_unknown_region_and_symbol_backends` | task-5-exact | `bf9f55a35cd84a29c821b89e0978f680febdbd0a` | PASS: 1 test | 2026-07-19 | no |
| Region search preserves target-workspace isolation | `cargo nextest run --lib fast_search_regions_respects_target_workspace` | task-5-exact | `bf9f55a35cd84a29c821b89e0978f680febdbd0a` | PASS: 1 test | 2026-07-19 | no |
| Deep dive prints stored complexity at every depth | `cargo nextest run --lib deep_dive_prints_stored_complexity_for_selected_symbol` | task-6-exact | `bf9f55a35cd84a29c821b89e0978f680febdbd0a` | PASS: 1 test | 2026-07-19 | no |
| Deep dive omits complexity when no metric exists | `cargo nextest run --lib deep_dive_omits_complexity_line_when_metric_is_absent` | task-6-exact | `bf9f55a35cd84a29c821b89e0978f680febdbd0a` | PASS: 1 test | 2026-07-19 | no |

The lead-owned final docs contract, formatting, lint, extractor integration,
system, dogfood, dev, build, and standalone CLI rows are appended only after
those commands run against the final committed source state.
