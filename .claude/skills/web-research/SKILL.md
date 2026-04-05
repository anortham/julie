---
name: web-research
description: Fetch web pages and index them locally for token-efficient research. Use when you need to read documentation, articles, or any web content. Fetches via browser39, saves as markdown, indexes via Julie's filewatcher, then uses Julie tools for selective reading. Requires browser39 (`cargo install browser39`).
user-invocable: true
arguments: "<url or research topic>"
allowed-tools: mcp__julie__fast_search, mcp__julie__get_symbols, mcp__julie__get_context, Bash, Write
---

# Web Research

Fetch web pages, index them locally, and read selectively using Julie's tools. This replaces dumping entire web pages into context (which wastes thousands of tokens) with a workflow that indexes the content and lets you pull out just the sections you need.

## Prerequisites

browser39 must be installed. Check with:

```bash
which browser39
```

If missing, tell the user to install it:

```bash
cargo install browser39
```

## Workflow

### Step 1: Fetch and Save

Determine the target file path from the URL. The directory structure mirrors the URL:

```
docs/web/
  docs.rs/axum/latest.md
  developer.mozilla.org/Web/API/Fetch_API.md
  github.com/tokio-rs/tokio.md
```

Fetch the page and save directly to the target file. Never print full page content to stdout.

```bash
# Fetch
echo '{"id":"1","action":"fetch","v":1,"seq":1,"url":"THE_URL","options":{"selector":"article","strip_nav":true,"include_links":true}}' > /tmp/b39-cmd.jsonl
browser39 batch /tmp/b39-cmd.jsonl --output /tmp/b39-out.jsonl

# Extract markdown directly to file (no stdout leak)
mkdir -p docs/web/TARGET_DOMAIN/TARGET_PATH_DIR
python3 -c "
import sys,json
with open('/tmp/b39-out.jsonl') as f:
    for line in f:
        d=json.loads(line)
        md = d.get('markdown','')
        if md:
            with open('docs/web/TARGET_DOMAIN/TARGET_PATH.md','w') as out:
                out.write(md)
            print(f'Saved {len(md)} chars to docs/web/TARGET_DOMAIN/TARGET_PATH.md')
"
```

If the page content is empty or too short, retry without the `selector` option, or try `"main"`, `".content"`, or `"body"`.

The filewatcher automatically indexes the file within 1-2 seconds. If `get_symbols` returns no results, wait a moment and retry, or fall back to `Read` for the specific section you need.

### Step 2: Explore the Content

See the page structure (table of contents):

```
get_symbols(file_path="docs/web/{domain}/{path}.md", mode="structure")
```

This returns section headings as a hierarchy, letting you see what's on the page without reading it.

### Step 3: Read Selectively

**Read a specific section** by name:

```
get_symbols(file_path="docs/web/{domain}/{path}.md", mode="minimal", target="Section Name")
```

**Search across all fetched pages:**

```
fast_search(query="your search terms", file_pattern="docs/web/**")
```

**Get token-budgeted context** for a concept:

```
get_context(query="concept or question", file_pattern="docs/web/**")
```

### Step 4: Follow Links (Optional)

The fetched markdown contains links. If you need more information, fetch additional pages by repeating Steps 1-2 with the linked URLs. The agent decides which links are worth following based on the research goal.

### Step 5: Clean Up

When research is complete, suggest removing fetched content:

```bash
# Remove a specific page
rm docs/web/{domain}/{path}.md

# Remove all fetched content
rm -rf docs/web/
```

The filewatcher automatically removes deleted files from the index.

## Tips

- **GitHub README pages**: Use `selector: "article"` to get just the README content
- **Documentation sites**: Try `selector: "main"` or `selector: ".content"` to skip navigation
- **Large pages**: Use browser39's `max_tokens` option to limit the fetch, then paginate with `offset`
- **Multiple pages at once**: Write multiple commands to the JSONL file (one per line) for batch fetching
- **Searching across fetched content**: `file_pattern="docs/web/**"` scopes any Julie tool to just the fetched web content
- **Empty results from get_symbols**: The filewatcher may not have finished indexing yet. Wait a moment and retry.
