# Go Verification — spf13/cobra

**Workspace:** cobra_8b201fd3 | **Files:** 65 | **Symbols:** 1441 | **Relationships:** 1590
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | 118 symbols from command.go at depth=1; struct fields, methods, constants, imports all captured |
| 2. Relationships | PASS | Cross-file refs found; `Command` has 123 dependents across command.go, cobra_test.go, bash_completions_test.go |
| 3. Identifiers | PASS | Definition + 20 references returned; call sites (cobra_test.go:289), method receivers (Uses) all present |
| 4. Centrality | PASS | `Command` centrality=1.00 (123 refs), `Execute` centrality=0.69 (15 refs), `SetErrPrefix` centrality=0.43 (3 refs) — correct gradient |
| 5. Definition Search | **PARTIAL** | With `language=go` filter: `Command` struct ranks #1. Without filter: 6 markdown doc titles ranked above it (see issue below) |
| 6. deep_dive Resolution | PASS | Methods listed via used-by (15 of 123 shown at context depth), test locations identified, body returned |
| 7. get_context Orientation | PASS | 3 pivots (Execute, IsAvailableCommand, Traverse) + 21 neighbors; pivots are high-centrality methods, not docs |
| 8. Test Detection | PASS | `exclude_tests=false`: 5 results from command_test.go; `exclude_tests=true`: 0 results (all correctly filtered out) |

**Overall: 7 PASS, 1 PARTIAL (definition search ranking without language filter)**

## Issues Found

### Issue: Markdown doc titles outrank Go struct definition (Check 5, cosmetic/minor)

When searching `fast_search(query="Command", search_target="definitions")` **without** a language filter, the `Command` struct definition at `command.go:54` ranked **2nd**, behind `site/content/completions/powershell.md:1` ("Generating PowerShell Completions For Your Own cobra.Command"). Five more markdown doc titles also ranked above other Go definitions.

With `language="go"` filter applied, `Command` struct correctly ranks **#1**.

**Impact:** Low — agents typically know the language and can filter. But for multi-language workspaces, markdown module titles containing symbol names can pollute definition search results.

**Root cause hypothesis:** Markdown headings are extracted as `module` symbols. When the heading text contains the search term ("cobra.Command"), the module definition matches the definition search. The markdown module symbols may lack a centrality penalty relative to the actual struct definition.

## Raw Evidence

### Check 1: get_symbols — command.go structure
```
command.go — 118 symbols
  namespace package cobra (17-17)
  constant const FlagSetByCobraAnnotation = "cobra_annotation_flag_set_by_cobra" (34-34)
  constant const CommandDisplayNameAnnotation = "cobra_annotation_command_display_name" (35-35)
  class type Command struct (54-54)
    field Use string (64-64)
    field Aliases []string (67-67)
    ...60+ fields...
  method func (c *Command) Context() context.Context (269-271)
  method func (c *Command) SetContext(ctx context.Context) (275-277)
  method func (c *Command) SetArgs(a []string) (281-283)
  ...50+ methods...
```
- SymbolKinds correct: `namespace` for package, `class` for struct, `field` for struct fields, `method` for receiver functions, `constant` for const, `import` for imports
- Private symbols marked with `(private)`
- 118 symbols is reasonable for a 2000-line Go file

### Check 2 & 3: fast_refs — Command
```
21 references to "Command":
  Definition: command.go:54 (class)
  References (20): mix of Uses (method receivers) and Calls (cobra_test.go:289)
```
Note: The 20-reference cap is the limit parameter; deep_dive reports 123 total dependents.

### Check 2 & 3: fast_refs — Execute, AddCommand, PersistentFlags
```
Execute: 17 references (2 definitions — public Execute + private execute)
  Cross-file: command_test.go, completions_test.go, cobra.go, flag_groups_test.go

AddCommand: 16 references (1 definition)
  Cross-file: args_test.go, bash_completions_test.go, command_test.go

PersistentFlags: 16 references (1 definition)
  Cross-file: command_test.go, completions_test.go
```
All show healthy cross-file reference patterns.

### Check 4: Centrality gradient
```
Command:       centrality=1.00, 123 incoming refs, risk=HIGH (0.85)
Execute:       centrality=0.69,  15 incoming refs, risk=MEDIUM (0.62)
SetErrPrefix:  centrality=0.43,   3 incoming refs, risk=HIGH (0.74)*
```
*SetErrPrefix risk=HIGH despite low centrality because test coverage is stub-only. Risk formula correctly accounts for both centrality and test quality.

### Check 5: Definition search ranking
Without language filter — first 3 results:
```
1. site/content/completions/powershell.md:1 (module)  ← markdown heading
2. command.go:54 (class, public) type Command struct   ← actual definition
3. site/content/docgen/yaml.md:1 (module)              ← markdown heading
```

With `language="go"` filter — first 3 results:
```
1. command.go:54 (class, public) type Command struct   ← correct #1
2. command.go:1342 AddCommand
3. command_test.go:32 emptyRun
```

### Check 6: deep_dive context depth
```
command.go:54 (class, public) — type Command struct
Used by (15 of 123): SetContext, SetArgs, SetOutput, SetOut, SetErr, SetIn, ...
Test locations (3): TestDeadcodeElimination [thin], runShellCheck, TestBashCompletions [stub]
Change Risk: HIGH (0.85) — 123 dependents, public, thin tests
Body returned with line numbers
```

### Check 7: get_context orientation
```
3 pivots: Execute (method, high centrality), IsAvailableCommand (method, high centrality),
          Traverse (method, medium centrality)
21 neighbors: Command struct, doc generators, completion helpers, etc.
Files spanned: command.go, doc/util.go, doc/yaml_docs.go, doc/rest_docs.go, doc/md_docs.go,
               bash_completions.go, doc/man_docs.go, cobra.go, completions.go
```
All pivots are Go source methods (not docs). Good orientation for "command execution" query.

### Check 8: Test detection
```
exclude_tests=false: 5 results in command_test.go (lines 274, 1913, 2177, 1906, 262)
exclude_tests=true:  0 results ("No results found")
```
All test functions correctly identified and excluded when requested.
