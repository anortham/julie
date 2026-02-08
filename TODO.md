# Plan tool

We need to revisit this tool and see if it needs work. Claude code plan mode has improved, skills like superpowers provide plans, but not everyone will use Julie in claude code.

## Search quality

### FIXED: file_pattern filtering (2026-02-08)

Root cause: `file_pattern` was applied as a post-filter AFTER results were already
capped by `limit`. Tantivy returned globally top-ranked results, the limit cap kept
only N of them, and then the glob filter removed everything because none of the top N
happened to be in the target files.

Fix: Move file_pattern filtering BEFORE the limit cap in all three search paths
(line_mode content, text_search definitions, text_search ref workspace). Also fixed
`language` being hardcoded to `None` in line_mode's SearchFilter. Un-ignored 2 tests
that were misattributed to "FTS timing issue."

### FIXED: get_symbols target filter / impl super:: methods (2026-02-08)

Root cause: `extract_impl()` in the Rust extractor only looked for `type_identifier`
children of `impl_item` nodes. But `impl super::Foo` produces a `scoped_type_identifier`
child instead. The function returned early, silently dropping the entire impl block
and all its methods. Affects 15+ files using `impl super::ExtractorName` pattern.

Fix: Handle `scoped_type_identifier` alongside `type_identifier`, extracting the last
type segment as the type name.

### Tools

deep_dive doesn't seem to be getting called, we need to look at the tool description and and the server instructions and make sure we are promoting that tool properly and using the correct behavioral adoption language

#### Fix all build warnings ALWAYS

We should probably validate that the filewatcher is keeping the index fresh too since we made all the changes with adding tantivy and removing the embeddings pipeline
