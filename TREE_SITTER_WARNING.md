# 🚨🔴 TREE-SITTER VERSION WARNING 🔴🚨

## ⚠️ DO NOT TOUCH THESE VERSIONS ⚠️

The tree-sitter ecosystem is a minefield of version incompatibilities. After hours of debugging, we have found a working combination:

### LOCKED VERSIONS (DO NOT CHANGE)
```toml
tree-sitter = "0.25"                    # Core parser library
tree-sitter-kotlin-ng = "1.1.0"         # Modern Kotlin parser
harper-tree-sitter-dart = "0.0.5"       # Modern Dart parser
# All other parsers use compatible versions
```

### WHY THESE VERSIONS?
- `harper-tree-sitter-dart` requires tree-sitter 0.25.6+
- Most other parsers work with tree-sitter 0.23+
- tree-sitter 0.25 satisfies all requirements
- Different tree-sitter versions have incompatible APIs
- Native library linking prevents multiple versions

### WHAT HAPPENS IF YOU CHANGE THEM?
- ❌ Build failures due to API incompatibilities
- ❌ Native library conflicts (only one tree-sitter version allowed)
- ❌ Hours of debugging version hell
- ❌ Breaking all language extractors
- ❌ Regression to old, unmaintained parser crates

### IF YOU MUST UPDATE VERSIONS:
1. **Research thoroughly** - Check ALL parser crate dependencies
2. **Update ALL at once** - Don't change versions piecemeal
3. **Test everything** - Every extractor, every test
4. **Update APIs** - Different tree-sitter versions have different APIs
5. **Document changes** - Update this file and CLAUDE.md

### LESSON LEARNED:
Tree-sitter is powerful but the ecosystem versioning is chaotic. We solved it once - don't break it again.

**REMEMBER: IF IT BUILDS, DON'T TOUCH IT!**