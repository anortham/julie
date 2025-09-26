* We still have build warnings, very unprofessional
* (lowest priority) we still need to figure out adding support for qmljs https://github.com/yuja/tree-sitter-qmljs
* tools.rs was refactored but we have other large files too. Remember claude can't read a file completely if it's over 25K tokens. This is a serious blocker for development work. It's also just unprofessional to have such bloated files. Smaller, organized files are much easier to search and navigate and especially EDIT.
* we need better test coverage, I think we proved that tonight!
* we need multiple performance reviews
* we need better token optimization and guards against requesting too much data and going over the 25k token limit in claude. We should do a deeper dive into the coa-mcp-framework and coa-codesearch-mcp projects and learn from their optimizations better
* we need realistic integration tests that are not just verifying something runs but the quality of the results
* we need real cross language automated tests that can be continually verified and improved upon
* we still appear to have everything in memory, I don't see a db or indexes or anything really in the .julie folder
*  julie - fast_search (MCP)(query: "extractor", mode: "text", limit: 10)
  ⎿  Error: MCP tool "fast_search" response (149505 tokens) exceeds maximum allowed tokens (25000). Please use pagination, filtering, or limit
     parameters to reduce the response size.
* In codesearch we used resources to provide the agent an avenue to view all results if needed. I don't know if that was actually helpful or not, just mentioning it for consideration.
* in codesearch we also provided a summary results mode and a details mode, again to help manage context usage/token usage
* search_and_replace was a high use tool in codesearch, we should have an equivalent. We have search, we have replace, should be easy enough to add a facade function that delegates to those 2.
* codesearch tools for investigation: delete_lines, search_and_replace, recent_files, text_search, find_references, get_symbols_overview, insert_at_line, symbol_search, line_search, file_search, batch_operations, trace_call_path, directory_search, goto_definition, similar_files, find_patterns, replace_lines. Not saying we need all of these, but we need to evaluate them and learn what we can from them and if possible have functional parity built into our tool designs
* we should be able to specify searching for text, file, directory, we also need to think about context in the results
* I see that FindLogicTool is currently a placeholder with "coming soon" functionality.
* 