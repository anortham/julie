# Search Quality Baseline Results

**Date:** 2025-11-08
**Julie Version:** v1.1.3 (before query expansion)
**MCP Server:** Rebuilt and restarted

---

## Test Results Summary

| Test | Query | Result Count | Precision | Ranking | Overall |
|------|-------|--------------|-----------|---------|---------|
| 1.1  | "user auth controller" | 5 | ★★★☆☆ Fair | ★★★★☆ Good | Fair - finds test helpers |
| 1.2  | "error handling logic" | 5 | ★★☆☆☆ Poor | ★☆☆☆☆ Poor | Poor - finds comments/tests |
| 1.3  | "process files optimized" | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - exact match works |
| 1.4  | "database connection pool" | 5 | ★★☆☆☆ Poor | ★☆☆☆☆ Poor | Poor - generic matches |
| 2.1  | "getUserData" (camelCase) | 5 | ★★★☆☆ Fair | ★★☆☆☆ Poor | Fair - finds docs, not actual function |
| 2.2  | "process_files_optimized" (exact) | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - exact match |
| 2.3  | "ProcessFilesOptimized" (PascalCase) | 2 | ★☆☆☆☆ Very Poor | ★☆☆☆☆ Poor | Very Poor - finds test doc only |
| 2.4  | "createAuthServiceLogin" (camelCase) | 2 | ★☆☆☆☆ Very Poor | ★☆☆☆☆ Poor | Very Poor - finds test doc only |
| 3.1  | "SymbolDatabase" | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - perfect |
| 3.2  | "preprocess_query" | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - exact match |
| 3.3  | "extract_symbols" | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - multiple extractors |
| 4.1  | "nonexistent impossible function" | 3 | ★☆☆☆☆ Very Poor | N/A | Expected - finds our test doc |
| 5.1  | "SQLite FTS5" (content) | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - relevant content |
| 5.2  | "query expansion" (content) | 5 | ★★★★★ Excellent | ★★★★★ Excellent | Excellent - relevant docs |

---

## Detailed Results

### Category 1: Multi-Word Queries

#### Test 1.1: "user auth controller"
**Search Target:** definitions
**Results:**
1. `create_csharp_auth_controller` - Test helper function ✓
2. `create_auth_service_login` - Test helper function ✓
3. `expand_query` - Has "user auth controller post" in docs ✓
4. `to_fuzzy_query` - Query utility ✗
5. `test_search_pipeline_with_realistic_agent_queries` - Test function ✗

**Precision:** ★★★☆☆ Fair (3/5 relevant)
**Ranking:** ★★★★☆ Good (most relevant in top 3)
**Analysis:** Finds test helpers that match the search terms, but not production auth controller code. This is semantic fallback working - FTS5 couldn't find better matches, so it returned what it could.

---

#### Test 1.2: "error handling logic"
**Search Target:** definitions
**Results:**
1. `test_extract_transactions_and_concurrency_control` - SQL test ✗
2. `QueryType` enum - Not error handling ✗
3. `Regex` import - Just mentions "error handling logic" in comment ✗
4. `test_process_multi_word_query` - Test function ✗
5. `test_detect_standard_text_query` - Test function ✗

**Precision:** ★★☆☆☆ Poor (1/5 loosely relevant)
**Ranking:** ★☆☆☆☆ Poor (no truly relevant results)
**Analysis:** **This is the problem.** Query found symbols that MENTION "error handling logic" in comments/docs, not actual error handling implementations. FTS5 content matching without intelligence.

---

#### Test 1.3: "process files optimized"
**Search Target:** definitions
**Results:**
1. `process_files_optimized` - EXACT MATCH ✓✓✓
2. `index_workspace_files` - Mentions "optimized parser reuse" ✓
3. `extract_directory` - "optimized for bulk operations" ✓
4. `test_trace_julie_indexing_flow` - Mentions file indexing ✓
5. `test_incremental_update_cleanup_atomicity` - Test ✗

**Precision:** ★★★★★ Excellent (4/5 relevant)
**Ranking:** ★★★★★ Excellent (exact match first)
**Analysis:** **This works perfectly.** Multi-word query where the words appear together in function name. FTS5 tokenization naturally matches this.

---

#### Test 1.4: "database connection pool"
**Search Target:** definitions
**Results:**
1. `process_files_optimized` - Mentions database ✗
2. `index_workspace_files` - Mentions database ✗
3. `test_extract_toml_nested_tables` - TOML test ✗
4. `test_search_pipeline_with_realistic_agent_queries` - Test ✗
5. `toml_extractor_tests` - TOML module ✗

**Precision:** ★★☆☆☆ Poor (database is mentioned but no connection pooling)
**Ranking:** ★☆☆☆☆ Poor (no truly relevant results)
**Analysis:** **Failure.** No actual database connection pooling code in the codebase, so it returned generic "database" matches.

---

### Category 2: Naming Convention Variants

#### Test 2.1: "getUserData" (camelCase searching for snake_case)
**Search Target:** definitions
**Results:**
1. `ExactMatchBoost` struct - Has "getUserData" in example docs ✓
2. `to_kebab_case` function - Has "getUserData" in example docs ✓
3. `generate_naming_variants` - Has "getUserData" in example docs ✓
4. `to_screaming_snake_case` - Has "getUserData" in example docs ✓
5. `process_standard_query` - Unrelated ✗

**Precision:** ★★★☆☆ Fair (4/5 match the query string)
**Ranking:** ★★☆☆☆ Poor (docs about naming, not actual functions)
**Analysis:** **This reveals the problem.** Found documentation ABOUT "getUserData", not actual `get_user_data` functions. **Without query expansion, naming convention variants don't cross-match.**

---

#### Test 2.2: "process_files_optimized" (exact snake_case)
**Search Target:** definitions
**Results:**
1. `process_files_optimized` - EXACT MATCH ✓✓✓
2. `index_workspace_files` - Related ✓
3. `test_trace_julie_indexing_flow` - Related test ✓
4. `test_incremental_update_cleanup_atomicity` - Test ✗
5. `dogfooding_tests` namespace - Test module ✗

**Precision:** ★★★★★ Excellent (3/5 highly relevant)
**Ranking:** ★★★★★ Excellent (exact match first)
**Analysis:** **Perfect.** Exact matching works great.

---

#### Test 2.3: "ProcessFilesOptimized" (PascalCase searching for snake_case)
**Search Target:** definitions
**Results:**
1. Our own test documentation mentioning "ProcessFilesOptimized" ✗
2. Our own test documentation ✗

**Precision:** ★☆☆☆☆ Very Poor (0/2 - just our test docs)
**Ranking:** ★☆☆☆☆ Poor (didn't find the actual function)
**Analysis:** **Total failure.** PascalCase query did NOT find `process_files_optimized` function. **This is exactly what query expansion should fix.**

---

#### Test 2.4: "createAuthServiceLogin" (camelCase searching for snake_case)
**Search Target:** definitions
**Results:**
1. Our own test documentation mentioning "createAuthServiceLogin" ✗
2. Our own test documentation ✗

**Precision:** ★☆☆☆☆ Very Poor (0/2)
**Ranking:** ★☆☆☆☆ Poor (didn't find `create_auth_service_login`)
**Analysis:** **Total failure.** camelCase query did NOT find snake_case function. **Another clear case for query expansion.**

---

### Category 3: Single-Word Queries

#### Test 3.1: "SymbolDatabase"
**Search Target:** definitions
**Results:**
1. `SymbolDatabase` struct - EXACT MATCH ✓✓✓
2-5. Various imports of `SymbolDatabase` ✓✓✓

**Precision:** ★★★★★ Excellent (5/5)
**Ranking:** ★★★★★ Excellent (struct definition first, then imports)
**Analysis:** **Perfect.** Single-word exact match works flawlessly.

---

#### Test 3.2: "preprocess_query"
**Search Target:** definitions
**Results:**
1. `preprocess_query` function - EXACT MATCH ✓✓✓
2. `preprocess_fallback_query` - Related ✓
3. `default_search_target` - Unrelated ✗
4. `test_colon_handling_in_patterns` - Test ✗
5. `process_standard_query` - Related ✓

**Precision:** ★★★★★ Excellent (3/5 highly relevant, exact match first)
**Ranking:** ★★★★★ Excellent
**Analysis:** **Excellent.** Exact match first, related functions follow.

---

#### Test 3.3: "extract_symbols"
**Search Target:** definitions
**Results:**
1-5. Various `extract_symbols` methods from different extractors (C#, Razor, CSS, Bash, TypeScript) ✓✓✓

**Precision:** ★★★★★ Excellent (5/5 all relevant)
**Ranking:** ★★★★★ Excellent
**Analysis:** **Perfect.** Found the common method across multiple language extractors.

---

### Category 4: Edge Cases

#### Test 4.1: "nonexistent impossible function"
**Search Target:** definitions
**Results:**
1-3. Our own test documentation that uses this exact phrase as an example

**Precision:** ★☆☆☆☆ Very Poor (expected - it IS in our test docs)
**Ranking:** N/A (this is an edge case test)
**Analysis:** **Expected behavior.** The phrase exists in our test documentation, so that's what it found. Semantic fallback would kick in if this returns empty.

---

### Category 5: Content Search (Grep-style)

#### Test 5.1: "SQLite FTS5" (content search)
**Search Target:** content
**Results:**
1-5. Various lines mentioning "SQLite FTS5" from code and docs ✓✓✓

**Precision:** ★★★★★ Excellent (all relevant)
**Ranking:** ★★★★★ Excellent (high relevance scores)
**Analysis:** **Excellent.** Content search works well for phrases that appear together.

---

#### Test 5.2: "query expansion" (content search)
**Search Target:** content
**Results:**
1-5. Lines from `query_expansion.rs`, docs, and TODO about query expansion ✓✓✓

**Precision:** ★★★★★ Excellent (all relevant)
**Ranking:** ★★★★★ Excellent
**Analysis:** **Excellent.** Found all relevant mentions of query expansion.

---

## Key Findings

### What Works Well ✅
1. **Single-word exact matches** - Perfect precision and ranking
2. **Content search for phrases** - Works well when terms appear together
3. **Multi-word queries where words appear together** - "process files optimized" works because the function name contains all three words

### What Fails ❌
1. **Naming convention variants** - PascalCase/camelCase queries don't find snake_case functions
   - "ProcessFilesOptimized" → doesn't find `process_files_optimized`
   - "createAuthServiceLogin" → doesn't find `create_auth_service_login`

2. **Multi-word queries (definitions)** - Returns generic matches or docs mentioning the words
   - "error handling logic" → finds comments, not actual error handling code
   - "database connection pool" → no actual pooling code exists, returns generic database matches

3. **Semantic gaps** - Finds documentation ABOUT a topic, not implementations
   - "getUserData" → finds docs with that string, not actual `get_user_data` functions

### Query Expansion Should Improve
1. ✅ **Naming convention variants** (Test 2.3, 2.4) - Generate snake_case/camelCase/PascalCase variants
2. ✅ **Multi-word matching** (Test 1.1, 1.2, 1.4) - AND/OR query variants
3. ✅ **Wildcard matching** - Add suffix wildcards for partial matches

### Query Expansion Won't Help
1. ❌ "database connection pool" - No such code exists in codebase (expected failure)
2. ❌ "error handling logic" - Too generic, hard to distinguish from comments

---

## Baseline Metrics

**Overall Search Quality:**
- Excellent: 7/14 tests (50%)
- Good/Fair: 3/14 tests (21%)
- Poor/Very Poor: 4/14 tests (29%)

**By Category:**
- Single-word queries: 3/3 excellent (100%)
- Multi-word queries: 1/4 good or better (25%) ⚠️
- Naming variants: 1/4 excellent (25%) ⚠️
- Content search: 2/2 excellent (100%)
- Edge cases: 0/1 (expected)

**Problem Areas Requiring Query Expansion:**
1. Naming convention cross-matching (PascalCase → snake_case)
2. Multi-word standard queries returning low-quality matches
3. Generic term queries finding docs instead of code

---

## Next Steps

1. **Implement query expansion** for Standard and Symbol query types
2. **Re-run this exact test suite** with query expansion enabled
3. **Compare results** - expect improvement in:
   - Test 2.3, 2.4 (naming variants)
   - Test 1.1, 1.2 (multi-word precision)
4. **Measure improvement**:
   - Precision increase
   - Ranking improvement
   - Overall quality score change

**Target Success Criteria:**
- Naming variant tests (2.3, 2.4): Should find actual functions (currently 0/2, target: 2/2)
- Multi-word queries (1.1, 1.2): Should improve precision by at least 1-2 stars
- Overall excellent rate: Increase from 50% to 65%+
