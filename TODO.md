# Plan tool

We need to revisit this tool and see if it needs work. Claude code plan mode has improved, skills like superpowers provide plans, but not everyone will use Julie in claude code.

## Search quality

Why do these not return results?

⏺ julie - Fast Code Search (MCP)(query: "create_extractor parse extract_symbols", search_target: "content", file_pattern:
                                "src/tests/typescript/*.rs", limit: 5)
  ⎿  🔍 No lines found matching: 'create_extractor parse extract_symbols'
     💡 Try a broader search term or different query

⏺ julie - Fast Code Search (MCP)(query: "fn create_extractor_and_parse", search_target: "definitions", file_pattern:
                                "src/tests/**/*.rs", limit: 5)
  ⎿  🔍 No results found for: 'fn create_extractor_and_parse'
     💡 Try a broader search term or different keywords

     