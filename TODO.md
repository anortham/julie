* We still have build warnings, very unprofessional
* (lowest priority) we still need to figure out adding support for qmljs https://github.com/yuja/tree-sitter-qmljs
* we need to refactor large files into smaller files so they are readable
* we need better test coverage, I think we proved that tonight!
* we need multiple performance reviews
* we need better token optimization and guards against requesting too much data and going over the 25k token limit in claude. We should do a deeper dive into the coa-mcp-framework and coa-codesearch-mcp projects and learn from their optimizations better
* we need realistic integration tests that are not just verifying something runs but the quality of the results
* we need real cross language automated tests that can be continually verified and improved upon
* we still appear to have everything in memory, I don't see a db or indexes or anything really in the .julie folder
*  julie - fast_search (MCP)(query: "extractor", mode: "text", limit: 10)
  ⎿  Error: MCP tool "fast_search" response (149505 tokens) exceeds maximum allowed tokens (25000). Please use pagination, filtering, or limit
     parameters to reduce the response size.
  