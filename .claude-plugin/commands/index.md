---
name: index
description: Index the current workspace for code intelligence (search, navigation, refactoring)
---

# Index Workspace Command

Initialize Julie's code intelligence for the current workspace.

## Task

1. **Detect current workspace:**
   Use the current working directory as the workspace root.

2. **Call Julie's manage_workspace tool:**
   ```
   manage_workspace({
     operation: "index",
     workspace_path: ".", // current directory
     workspace_type: "primary"
   })
   ```

3. **Show progress:**
   ```markdown
   🔍 Indexing workspace...

   This will:
   - Scan files for 25 supported languages
   - Build SQLite database with FTS5 full-text search
   - Generate semantic embeddings (background, ~20-30s)
   - Create call graph relationships

   Search will work immediately, semantic search enhances in background.
   ```

4. **Handle result:**
   ```markdown
   ✅ Workspace indexed successfully!

   **Indexed:**
   - [N] files across [M] languages
   - [P] symbols extracted
   - [Q] relationships mapped

   **Available now:**
   - Fast search (text + semantic)
   - Symbol navigation (goto, references)
   - Call path tracing
   - Safe refactoring

   **Background:**
   - Semantic embeddings building (~20-30s)
   - GPU acceleration: [status]

   **Next steps:**
   - Try: /search [query]
   - Try: /symbols [file]
   - Or just ask me to find code!
   ```

## Supported Languages

Julie indexes these 25 languages:
- TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin
- C, C++, Go, Rust, Lua
- GDScript, Vue, Razor, SQL, HTML, CSS, Regex
- Bash, PowerShell, Zig, Dart

## Index Location

Indexes stored at project level:
```
.julie/
├── indexes/
│   └── {workspace_id}/
│       ├── db/symbols.db        # SQLite + FTS5
│       └── vectors/             # Semantic embeddings
```

## Optional Arguments

Users may specify:
- `/index` - Index current directory (default)
- `/index /path/to/workspace` - Index specific path
- `/index --reindex` - Force re-index existing workspace

## Re-indexing

If workspace already indexed:
```markdown
📋 Workspace already indexed

Last indexed: [timestamp]
Files: [N], Symbols: [M]

Options:
- Incremental update (fast): /index --update
- Full re-index (slow): /index --reindex
- View status: manage_workspace({ operation: "list" })
```

## Error Handling

- If not in a code workspace, explain indexing requires project root
- If Julie server not running, provide setup instructions
- If indexing fails, show error and suggest troubleshooting

## Performance Expectations

- **Small project** (100 files): <1s
- **Medium project** (1000 files): ~2s
- **Large project** (10,000 files): ~10-15s
- **Semantic embeddings**: Additional 20-30s in background

Search works immediately, semantic enhancement happens asynchronously.
