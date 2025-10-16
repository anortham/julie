# Test Directory Reorganization Plan

## Current State
- **123+ test files** in flat `src/tests/` root directory
- **10 existing subdirectories**: bash/, c/, cpp/, csharp/, css/, gdscript/, html/, java/, lua/, sql/
- Inconsistent naming (`*_inline_tests.rs`, `*_tests.rs`, etc.)
- Hard to navigate and find related tests

## Target Structure

```
src/tests/
├── mod.rs                      # Updated module registry
├── test_utils.rs              # Keep test utilities
│
├── extractors/                # ALL LANGUAGE EXTRACTOR TESTS
│   ├── bash/
│   ├── c/                    # Keep existing directory
│   ├── cpp/                  # Keep existing directory
│   ├── csharp/               # Keep existing + add csharp_extractor_inline_tests.rs
│   ├── css/                  # Keep existing directory
│   ├── dart/
│   ├── gdscript/             # Keep existing directory
│   ├── go/
│   ├── html/                 # Keep existing directory
│   ├── java/                 # Keep existing directory
│   ├── javascript/
│   ├── kotlin/
│   ├── lua/                  # Keep existing + merge all lua_*_inline_tests.rs
│   ├── php/
│   ├── powershell/
│   ├── python/               # Merge python_tests.rs + all python_*_inline_tests.rs
│   ├── razor/
│   ├── regex/                # Merge regex_tests.rs + all regex_*_inline_tests.rs
│   ├── ruby/
│   ├── rust/                 # Merge rust_tests.rs + all rust_*_inline_tests.rs
│   ├── sql/                  # Keep existing directory
│   ├── swift/
│   ├── typescript/           # Merge typescript_tests.rs + all typescript_*_inline_tests.rs
│   ├── vue/
│   ├── zig/
│   └── base.rs               # base_inline_tests.rs → extractors/base.rs
│
├── tools/                    # ALL TOOL TESTS
│   ├── editing/
│   │   ├── fuzzy_replace.rs  # fuzzy_replace_tests.rs
│   │   ├── edit_lines.rs     # edit_lines_tests.rs
│   │   └── mod.rs            # editing_inline_tests.rs
│   ├── search/
│   │   ├── mod.rs            # search_inline_tests.rs
│   │   ├── line_mode.rs      # search_line_mode_tests.rs
│   │   ├── quality.rs        # search_quality_tests.rs
│   │   └── race_condition.rs # search_race_condition_tests.rs
│   ├── refactoring/
│   │   ├── mod.rs            # refactoring_inline_tests.rs
│   │   ├── smart_refactor.rs # refactoring_tests.rs
│   │   └── smart_refactor_control.rs # smart_refactor_control_tests.rs
│   ├── workspace/
│   │   ├── mod_tests.rs      # workspace_mod_tests.rs
│   │   ├── utils.rs          # workspace_utils_inline_tests.rs
│   │   ├── isolation.rs      # workspace_isolation_tests.rs
│   │   ├── management_token.rs # workspace_management_token_tests.rs
│   │   ├── registry.rs       # workspace_registry_inline_tests.rs
│   │   └── registry_service.rs # registry_service_inline_tests.rs
│   ├── navigation/
│   │   └── mod.rs            # navigation_tools_tests.rs
│   ├── exploration/
│   │   ├── mod.rs            # exploration_tools_tests.rs
│   │   └── find_logic.rs     # find_logic_tests.rs
│   ├── ast_symbol_finder.rs  # ast_symbol_finder_inline_tests.rs
│   ├── get_symbols.rs        # get_symbols_tests.rs
│   ├── get_symbols_target_filtering.rs # get_symbols_target_filtering_tests.rs
│   ├── get_symbols_token.rs  # get_symbols_token_tests.rs
│   ├── smart_read.rs         # smart_read_tests.rs
│   ├── trace_call_path/
│   │   ├── mod.rs            # trace_call_path_inline_tests.rs
│   │   └── comprehensive.rs  # trace_call_path_tests.rs
│   └── syntax_validation.rs  # syntax_validation_tests.rs
│
├── utils/                    # ALL UTILITY MODULE TESTS
│   ├── context_truncation.rs # context_truncation_inline_tests.rs
│   ├── cross_language_intelligence.rs # cross_language_intelligence_inline_tests.rs
│   ├── exact_match_boost/
│   │   ├── mod.rs            # exact_match_boost_inline_tests.rs
│   │   └── tests.rs          # exact_match_boost_tests.rs
│   ├── path_relevance/
│   │   ├── mod.rs            # path_relevance_inline_tests.rs
│   │   └── tests.rs          # path_relevance_tests.rs
│   ├── progressive_reduction.rs # progressive_reduction_inline_tests.rs
│   ├── query_expansion.rs    # query_expansion_inline_tests.rs
│   └── token_estimation.rs   # token_estimation_inline_tests.rs
│
├── cli/                      # ALL CLI TESTS
│   ├── codesearch.rs         # cli_codesearch_tests.rs
│   ├── semantic.rs           # cli_semantic_tests.rs
│   ├── output.rs             # cli_output_inline_tests.rs
│   ├── parallel.rs           # cli_parallel_inline_tests.rs
│   └── progress.rs           # cli_progress_inline_tests.rs
│
├── core/                     # CORE SYSTEM TESTS
│   ├── database.rs           # database_inline_tests.rs
│   ├── handler.rs            # handler_inline_tests.rs
│   ├── language.rs           # language_inline_tests.rs
│   ├── embeddings/
│   │   ├── mod.rs            # embeddings_inline_tests.rs
│   │   ├── cross_language.rs # cross_language_inline_tests.rs
│   │   ├── vector_store.rs   # vector_store_inline_tests.rs
│   │   └── hnsw_vector_store.rs # hnsw_vector_store_tests.rs
│   └── tracing.rs            # tracing_inline_tests.rs
│
└── integration/              # INTEGRATION & END-TO-END TESTS
    ├── real_world_validation.rs # real_world_validation.rs (keep as-is)
    ├── reference_workspace.rs # reference_workspace_tests.rs
    ├── lock_contention.rs    # lock_contention_tests.rs
    ├── stale_index_detection.rs # stale_index_detection_tests.rs
    ├── fts5_sanitization.rs  # fts5_sanitization_tests.rs
    └── watcher.rs            # watcher_tests.rs
```

## File Moves Summary

### Extractors (26 languages + base)
- Move all `*_tests.rs` and `*_inline_tests.rs` for each language into `extractors/<language>/`
- Rename files to remove suffixes (e.g., `typescript_functions_inline_tests.rs` → `typescript/functions.rs`)
- Total: ~85 files to organize

### Tools
- Move all tool-related tests into `tools/` subdirectories
- Group by tool category (editing, search, refactoring, workspace, etc.)
- Total: ~25 files

### Utils
- Move all utility tests into `utils/`
- Total: ~8 files

### CLI
- Move CLI tests into `cli/`
- Total: ~5 files

### Core
- Move core system tests into `core/`
- Total: ~8 files

### Integration
- Move integration tests into `integration/`
- Total: ~6 files

## Benefits

1. **Easy Navigation**: Tests organized by category and module
2. **Clear Structure**: Professional hierarchy matches src/ structure
3. **Consistent Naming**: Remove `_inline_tests` and `_tests` suffixes
4. **Discoverability**: Related tests grouped together
5. **Maintainability**: Clear ownership and responsibility

## Execution Strategy

1. Create new directory structure
2. Move and rename files in batches
3. Update mod.rs with new module paths
4. Run tests to verify no regressions
5. Clean up empty directories

## Estimated Impact
- **Files to move**: ~137 files
- **Directories to create**: ~35 new directories
- **mod.rs updates**: ~200+ module declarations to update
