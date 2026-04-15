# julie-extractors

Tree-sitter-based extraction for Julie's code intelligence pipeline.

## Public Surface

Use one of these two entrypoints:

- `extract_canonical(file_path, content, workspace_root)`
- `ExtractorManager::extract_all(file_path, content, workspace_root)`

`ExtractorManager::extract_symbols`, `extract_identifiers`, and `extract_relationships` are thin projections over the canonical result. They do not own separate parsing or dispatch behavior.

The old pre-parsed-tree factory helper is now internal-only. External callers should not bypass the canonical pipeline.

## Result Semantics

`ExtractionResults` contains:

- `symbols`
- `identifiers`
- `relationships`
- `pending_relationships`
- `structured_pending_relationships`
- `types`

### Paths and IDs

- Stored file paths are normalized relative Unix-style paths.
- Symbol and identifier IDs are derived from normalized path plus normalized location.
- JSONL records are rekeyed after record offsets are applied, so repeated keys on different lines keep distinct IDs.

### JSONL

- `.jsonl` files go through the canonical production path.
- Records are parsed line-by-line as JSON.
- Returned spans use file-global line and byte positions.
- Empty lines are skipped without collapsing later record positions.

### Unresolved Relationships

- `structured_pending_relationships` is the canonical unresolved-edge surface.
- Each entry preserves a structured `target` with terminal name plus any receiver, namespace path, or import context the extractor can prove.
- `pending_relationships` remains as degraded compatibility output for consumers that still read the legacy shape.
- Wrong edges are treated as worse than missing edges, so ambiguous cross-file targets stay pending instead of being force-resolved by name.

## Supported Languages

The crate ships 33 concrete extractors:

- Systems: Rust, C, C++, Go, Zig
- Web: TypeScript, JavaScript, HTML, CSS, Vue, QML
- Backend: Python, Java, C#, PHP, Ruby, Swift, Kotlin, Dart
- Functional: Elixir, Scala
- Scripting: Lua, R, Bash, PowerShell
- Specialized: GDScript, Razor, SQL, Regex
- Documentation and data: Markdown, JSON, TOML, YAML

The registry also exposes JSX and TSX aliases on top of the JavaScript and TypeScript extractors.

## Minimal Example

```rust
use julie_extractors::{extract_canonical, ExtractorManager};
use std::path::Path;

let workspace_root = Path::new("/workspace/project");
let file_path = "src/main.ts";
let content = "export function greet() { return 'hi' }";

let canonical = extract_canonical(file_path, content, workspace_root)?;

let manager = ExtractorManager::new();
let projected_symbols = manager.extract_symbols(file_path, content, workspace_root)?;

assert_eq!(projected_symbols, canonical.symbols);
# Ok::<(), anyhow::Error>(())
```
