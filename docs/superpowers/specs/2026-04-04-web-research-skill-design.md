# Web Research Skill Design

**Date:** 2026-04-04
**Status:** Draft
**Type:** New skill (no Rust changes)

## Problem

AI agents regularly need to look up web content (documentation, API references, articles) during development tasks. Current options are wasteful:

1. **WebFetch** sends the full page through an intermediate model, consuming 10-75x more tokens than necessary, and returns a lossy summary
2. **Raw browser tools** dump entire pages into context with no structure or selective reading
3. **Web search** finds links but still requires a full-page fetch to read the content

Meanwhile, Julie already has the infrastructure to solve this: markdown extraction with hierarchical sections, full-text search with embeddings, and a filewatcher that auto-indexes new files. The missing piece is just getting web content onto disk as markdown.

## Solution

A **skill** (not an MCP tool) that teaches agents how to orchestrate browser39 + Julie's existing tools for token-efficient web research.

### Why a skill, not a native MCP tool

1. **Zero Rust code.** The entire pipeline exists: browser39 fetches and converts to markdown, the filewatcher indexes `.md` files, the markdown extractor creates hierarchical symbols, and existing tools provide selective reading. There is no missing capability.
2. **The "tool" is orchestration, not computation.** An MCP tool would just be glue code: shell out to browser39, write a file, wait for the filewatcher. That's what agents do well when given clear instructions.
3. **Decoupled evolution.** browser39 and Julie's extractors improve independently. Neither needs to know about the other.
4. **Clean dependency story.** Julie stays a single binary with zero external deps. Web research is an optional capability users opt into by installing browser39.

## Architecture

```
Agent                    browser39              Filesystem           Julie
  |                         |                      |                   |
  |-- fetch URL ----------->|                      |                   |
  |<-- markdown content ----|                      |                   |
  |                                                |                   |
  |-- write docs/web/{domain}/{path}.md ---------->|                   |
  |                                                |-- filewatcher --->|
  |                                                |   auto-indexes    |
  |                                                |                   |
  |-- get_symbols(file) ------------------------------------------------>|
  |<-- section TOC --------------------------------------------------------|
  |                                                                    |
  |-- fast_search(query, file_pattern="docs/web/**") ----------------->|
  |<-- targeted results ------------------------------------------------|
```

### Storage Convention

Fetched pages are saved to `docs/web/` in the workspace root, organized by domain:

```
docs/web/
  github.com/
    alejandroqh/
      browser39.md
  docs.rs/
    axum/
      latest.md
  developer.mozilla.org/
    Web/
      API/
        Fetch_API.md
```

**Naming rules:**
- Path mirrors the URL structure: `{domain}/{path}.md`
- The agent derives a reasonable filename from the URL path (strip query params, trailing slashes)
- If the URL path is just `/`, use `index.md`
- Agents should use their judgment for reasonable names; the skill provides conventions, not rigid rules

**Lifecycle:**
- Creating a file: filewatcher auto-indexes it (symbols, search, embeddings)
- Deleting a file: filewatcher auto-removes it from the index
- No special cleanup logic needed

### Fetching Workflow

The skill instructs agents to:

1. **Check browser39 is available**: `which browser39`. If missing, prompt the user to install it (`cargo install browser39`).

2. **Fetch a page**: Use browser39 batch mode to get the page as markdown:
   ```bash
   echo '{"id":"1","action":"fetch","v":1,"seq":1,"url":"URL","options":{"selector":"article","strip_nav":true,"include_links":true}}' > /tmp/b39-cmd.jsonl
   browser39 batch /tmp/b39-cmd.jsonl --output /tmp/b39-out.jsonl
   ```

3. **Extract the markdown**: Parse the JSONL output and extract the `markdown` field.

4. **Save to disk**: Write to `docs/web/{domain}/{path}.md`. The filewatcher picks it up within 1-2 seconds.

5. **Explore the content**: Use `get_symbols` on the saved file to see the page structure (sections become `Module` symbols).

6. **Read selectively**: Use `fast_search`, `get_context`, or `get_symbols` with a target to read specific sections without loading the entire page into context.

7. **Follow links**: The markdown contains links. The agent decides whether to fetch additional pages based on what it needs.

### Multi-Page Research

For researching a topic across multiple pages:
- The agent fetches pages one at a time, saving each to `docs/web/`
- All fetched pages are searchable together via `fast_search(query="...", file_pattern="docs/web/**")`
- The agent follows links from fetched pages as needed
- No automated crawling; the agent makes intelligent decisions about what to fetch

### Cleanup

When the agent or user is done with reference material:
- Delete individual files: `rm docs/web/{domain}/{path}.md`
- Delete everything: `rm -rf docs/web/`
- The filewatcher handles deindexing automatically
- The skill should remind agents to suggest cleanup when research is complete

## What Gets Indexed

When a markdown file lands in `docs/web/`, Julie's markdown extractor creates:
- **Top-level module**: The page's `# Title` heading
- **Nested modules**: Each `## Section` heading, with section body text stored as `doc_comment` for RAG embeddings
- **Hierarchical nesting**: `###` headings nest under `##`, etc.

This means:
- `get_symbols` returns a table of contents
- `get_symbols` with `target` extracts a specific section's content
- `fast_search` finds content across all fetched pages
- `get_context` returns token-budgeted relevant sections
- Embeddings enable semantic similarity search across fetched content

## Deliverables

### 1. Skill file: `.claude/skills/web-research/SKILL.md`

A new skill in Julie's skill directory that:
- Describes the web research workflow
- Documents the `docs/web/` storage convention
- Shows how to use browser39 batch mode for fetching
- Shows how to use Julie tools for selective reading
- Covers multi-page research and cleanup
- Notes browser39 as a prerequisite with install instructions
- Is user-invocable (e.g., `/web-research <url>` or `/web-research <topic>`)

### 2. Plugin distribution

**Immediate:** Copy the skill to `~/source/julie-plugin/skills/web-research/SKILL.md`

**CI update:** Add `web-research` to the skill copy list in `julie-plugin/.github/workflows/update-binaries.yml`:
```bash
for skill in architecture call-trace dependency-graph editing explore-area \
             impact-analysis logic-flow metrics search-debug type-flow web-research; do
```

Update the skill count check from 10 to 11.

### 3. README updates

**Julie README (`README.md`):**
- Add `web-research` to the Skills table with description
- Add browser39 as an optional prerequisite in the Installation section

**Plugin README (`~/source/julie-plugin/README.md`):**
- Add `web-research` to the skill count (8 -> 9) and list
- Add browser39 as an optional prerequisite in the Prerequisites section

### 4. CLAUDE.md update

Update the plugin distribution section in CLAUDE.md to reflect the new skill count.

## Validated Assumptions

These were validated during the brainstorming session on 2026-04-04:

1. **browser39 output quality**: browser39 produces clean markdown that Julie's extractor handles well. Tested with the browser39 GitHub repo page; extractor created 8 well-structured symbols with correct hierarchical nesting.

2. **Filewatcher latency**: Files written to `docs/web/` are indexed within ~2 seconds. Acceptable for the research workflow.

3. **Selective reading works**: `get_symbols`, `fast_search`, and `get_context` all work correctly on fetched web content. `get_symbols` with `target` extracts specific sections cleanly.

4. **No markdown cleanup needed**: browser39's output has minor noise (e.g., GitHub permalink markers) but it doesn't impact search quality or agent usability. The agent can handle any rough edges.

## Non-Goals

- **Automated crawling**: No depth parameter, no sitemap parsing. The agent follows links manually.
- **JavaScript-heavy SPA support**: browser39 handles basic JS; for heavy SPAs, agents should fall back to Chrome automation tools.
- **Caching or deduplication**: If the agent fetches the same URL twice, it overwrites the file. Simple and predictable.
- **Native Rust implementation**: No new MCP tool. The skill orchestrates existing tools.
- **Markdown post-processing**: No cleaning, stripping, or transforming browser39's output. Trust the existing pipeline.

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| browser39 changes its output format | Low | Skill is format-agnostic; instructs agents to parse JSONL, not hardcode fields |
| Large pages create large index entries | Low | browser39 has `max_tokens` and `selector` options for targeted fetching |
| `docs/web/` pollutes git status | Medium | Document in the skill that users should gitignore `docs/web/` or clean up after research |
| Filewatcher misses the new file | Very low | Validated at <2s latency; agent can retry `get_symbols` if needed |

## Future Possibilities (not in scope)

- A `.gitignore` entry for `docs/web/` could be added to project templates
- The skill could evolve to support other fetching backends (Lightpanda, agent-browser) if browser39 doesn't cover a use case
- Persistent reference libraries could be committed to the repo (e.g., `docs/references/`) for shared team knowledge
