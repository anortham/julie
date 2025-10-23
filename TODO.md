● julie - fast_search (MCP)(query: "handle_file_created_or_modified embeddings", search_method: "text", limit: 10, search_target: "content", file_pattern: "src/watcher/**")
  ⎿  Error: Tool execution failed: fts5: missing row 550 from content table 'main'.'files'

  ● julie - fast_search (MCP)(query: "FileChangeType::Deleted", search_method: "text", limit: 10, search_target: "content", output: "content", -n: "true")
  ⎿  🔍 No results found for: 'FileChangeType::Deleted'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "Deleted", search_method: "text", limit: 15, search_target: "content", file_pattern: "src/watcher/**", output: "content", -n: "true")
  ⎿  🔍 No results found for: 'Deleted'
     💡 Try a broader search term, different mode, or check spelling

● julie - fast_search (MCP)(query: "handle_file_change_static", search_method: "text", limit: 5, search_target: "content", output: "content", -n: "true")
  ⎿  🔍 No results found for: 'handle_file_change_static'
     💡 Try a broader search term, different mode, or check spelling

# we need to check server instructions for referneces to old tools now that we removed the smart_refactor tool and replaced it with 2 new tools.

# when you call refresh on a reference workspace, we should kick off an incremental update, not a complete rebuild, that's what reindex with the force parameter is for

TODOs, Stubs, garbage! We have got to systematically go through all of them and either implement them or delete them, or at least figure out why they exist!
