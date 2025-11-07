# Dependencies Management

**Last Updated:** 2025-11-07

## Tree-Sitter Version WARNING

### ⚠️ ABSOLUTELY DO NOT CHANGE TREE-SITTER VERSIONS ⚠️

**LOCKED AND TESTED VERSIONS:**
- `tree-sitter = "0.25"` (REQUIRED for harper-tree-sitter-dart)
- `tree-sitter-kotlin-ng = "1.1.0"` (modern Kotlin parser)
- `harper-tree-sitter-dart = "0.0.5"` (modern Dart parser)

**CHANGING THESE WILL CAUSE:**
- ❌ API incompatibilities between different tree-sitter versions
- ❌ Native library linking conflicts
- ❌ Hours of debugging version hell
- ❌ Complete build failures
- ❌ Breaking all extractors

**IF YOU MUST CHANGE VERSIONS:**
1. Update ALL parser crates simultaneously
2. Test every single extractor
3. Update API calls if needed (0.20 vs 0.25 APIs differ)
4. Verify no native library conflicts
5. Test on all platforms

## Adding New Dependencies

**CRITICAL: ALWAYS verify dependency versions first!**

Before adding any dependency:
1. Use crates.io search: https://crates.io/search?q=CRATE_NAME
2. Use web search to verify API and examples
3. Check current documentation for breaking changes
4. Does it break single binary deployment?
5. Does it require external libraries?
6. Is it cross-platform compatible?
7. Does it impact startup time significantly?

**Examples:**
- Before: `tokio = "1.47"` → Search crates.io (latest: 1.47.1)
- Before: `blake3 = "1.5"` → Search crates.io (latest: 1.8.0)
