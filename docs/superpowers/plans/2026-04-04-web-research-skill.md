# Web Research Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a web-research skill that teaches agents to use browser39 + Julie's existing tools for token-efficient web content retrieval and selective reading.

**Architecture:** Pure skill-based (no Rust code). A new `SKILL.md` teaches agents to: (1) fetch web pages as markdown via browser39, (2) save to `docs/web/{domain}/{path}.md` where the filewatcher auto-indexes them, (3) use Julie's `get_symbols`/`fast_search`/`get_context` for selective reading. Plugin distribution via skill copy + CI update. README updates for browser39 prerequisite.

**Tech Stack:** Markdown skill file, bash (browser39 CLI), Julie MCP tools

**Spec:** `docs/superpowers/specs/2026-04-04-web-research-skill-design.md`

---

### Task 1: Create the web-research skill

**Files:**
- Create: `.claude/skills/web-research/SKILL.md`

- [ ] **Step 1: Write the skill file**

Create `.claude/skills/web-research/SKILL.md` with the following content:

```markdown
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

### Step 1: Fetch a Page

Use browser39 batch mode to fetch a URL as markdown:

```bash
echo '{"id":"1","action":"fetch","v":1,"seq":1,"url":"THE_URL","options":{"selector":"article","strip_nav":true,"include_links":true}}' > /tmp/b39-cmd.jsonl
browser39 batch /tmp/b39-cmd.jsonl --output /tmp/b39-out.jsonl
```

Extract the markdown from the JSONL output:

```bash
cat /tmp/b39-out.jsonl | python3 -c "
import sys,json
for line in sys.stdin:
    d=json.loads(line)
    url = d.get('url','')
    if 'TARGET_DOMAIN' in url:
        print(d.get('markdown',''))
"
```

If the page content is empty or too short, try without the `selector` option, or try a different selector like `"main"`, `".content"`, or `"body"`.

### Step 2: Save to docs/web/

Write the markdown to `docs/web/{domain}/{path}.md`. The directory structure mirrors the URL:

```
docs/web/
  docs.rs/axum/latest.md
  developer.mozilla.org/Web/API/Fetch_API.md
  github.com/tokio-rs/tokio.md
```

Create parent directories as needed (`mkdir -p`). Use the Write tool to save the file.

The filewatcher automatically indexes the file within 1-2 seconds (symbols, full-text search, embeddings).

### Step 3: Explore the Content

See the page structure (table of contents):

```
get_symbols(file_path="docs/web/{domain}/{path}.md", mode="structure")
```

This returns section headings as a hierarchy, letting you see what's on the page without reading it.

### Step 4: Read Selectively

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

### Step 5: Follow Links (Optional)

The fetched markdown contains links. If you need more information, fetch additional pages by repeating Steps 1-2 with the linked URLs. The agent decides which links are worth following based on the research goal.

### Step 6: Clean Up

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
```

- [ ] **Step 2: Verify the skill file is well-formed**

Open the file and confirm it has valid YAML frontmatter, all sections are present, and all code blocks are properly fenced.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/web-research/SKILL.md
git commit -m "feat(skills): add web-research skill for browser39 + Julie integration"
```

---

### Task 2: Copy skill to plugin repo

**Files:**
- Create: `~/source/julie-plugin/skills/web-research/SKILL.md`

- [ ] **Step 1: Copy the skill directory**

```bash
mkdir -p ~/source/julie-plugin/skills/web-research
cp .claude/skills/web-research/SKILL.md ~/source/julie-plugin/skills/web-research/SKILL.md
```

- [ ] **Step 2: Verify the copy**

```bash
diff .claude/skills/web-research/SKILL.md ~/source/julie-plugin/skills/web-research/SKILL.md
```

Expected: no output (files are identical).

---

### Task 3: Update plugin CI workflow

**Files:**
- Modify: `~/source/julie-plugin/.github/workflows/update-binaries.yml`

- [ ] **Step 1: Add web-research to the skill copy list**

In `~/source/julie-plugin/.github/workflows/update-binaries.yml`, find the `for skill in` line and add `web-research`:

Change:
```bash
          for skill in architecture call-trace dependency-graph editing explore-area \
                       impact-analysis logic-flow metrics search-debug type-flow; do
```

To:
```bash
          for skill in architecture call-trace dependency-graph editing explore-area \
                       impact-analysis logic-flow metrics search-debug type-flow \
                       web-research; do
```

- [ ] **Step 2: Update the skill count check**

In the same file, find the skill count check and update from 10 to 11:

Change:
```bash
          if [ "$SKILL_COUNT" -lt 10 ]; then
            echo "WARNING: Expected 10 skills, got ${SKILL_COUNT}" >&2
          fi
```

To:
```bash
          if [ "$SKILL_COUNT" -lt 11 ]; then
            echo "WARNING: Expected 11 skills, got ${SKILL_COUNT}" >&2
          fi
```

- [ ] **Step 3: Commit in the plugin repo**

```bash
cd ~/source/julie-plugin
git add .github/workflows/update-binaries.yml skills/web-research/SKILL.md
git commit -m "feat: add web-research skill, update CI for 11 skills"
```

---

### Task 4: Update Julie README

**Files:**
- Modify: `README.md` (julie repo root)

- [ ] **Step 1: Update skill count**

In `README.md`, find:
```markdown
Julie ships with 9 pre-built skills
```

Change to:
```markdown
Julie ships with 10 pre-built skills
```

- [ ] **Step 2: Add web-research to the Navigation & Analysis Skills table**

In `README.md`, after the `/type-flow` row in the Navigation & Analysis Skills table, add a new "Research Skills" section:

```markdown

### Research Skills

| Skill | Description |
|-------|-------------|
| `/web-research` | Fetch web pages via browser39, index locally, and read selectively with Julie tools |
```

- [ ] **Step 3: Add browser39 as optional prerequisite**

In the Installation section of `README.md`, after the "Build from Source" subsection and before "Connect Your AI Tool", add:

```markdown
### Optional: Web Research

To enable the `/web-research` skill for fetching and indexing web content:

```bash
cargo install browser39
```

This is optional. All other Julie features work without it.
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add web-research skill to README, browser39 as optional prereq"
```

---

### Task 5: Update plugin README

**Files:**
- Modify: `~/source/julie-plugin/README.md`

- [ ] **Step 1: Update skill count in "What the Plugin Provides"**

In `~/source/julie-plugin/README.md`, find:
```markdown
- **8 skills** (`/explore-area`, `/call-trace`, `/impact-analysis`, `/dependency-graph`, `/logic-flow`, `/type-flow`, `/architecture`, `/metrics`)
```

Change to:
```markdown
- **9 skills** (`/explore-area`, `/call-trace`, `/impact-analysis`, `/dependency-graph`, `/logic-flow`, `/type-flow`, `/architecture`, `/metrics`, `/web-research`)
```

- [ ] **Step 2: Update skill count in Project Structure**

In the same file, find:
```markdown
skills/                8 skill directories copied from anortham/julie during updates
```

Change to:
```markdown
skills/                9 skill directories copied from anortham/julie during updates
```

- [ ] **Step 3: Add browser39 as optional prerequisite**

In the Prerequisites section, after the `uv` install instructions and before the "## Installation" heading, add:

```markdown
### Optional: Web Research

To enable the `/web-research` skill for fetching and indexing web content, install [browser39](https://github.com/alejandroqh/browser39):

```bash
cargo install browser39
```

This is optional. All other Julie features work without browser39.
```

- [ ] **Step 4: Commit in the plugin repo**

```bash
cd ~/source/julie-plugin
git add README.md
git commit -m "docs: add web-research skill, browser39 as optional prereq"
```

---

### Task 6: Clean up validation artifacts

**Files:**
- Delete: `docs/web/github.com/alejandroqh/browser39.md` (created during brainstorming validation)

- [ ] **Step 1: Remove the test file**

```bash
rm -rf docs/web/
```

- [ ] **Step 2: Verify filewatcher deindexed it**

Wait 2 seconds, then verify the file is no longer indexed:

```
fast_search(query="browser39", file_pattern="docs/web/**")
```

Expected: no results.

- [ ] **Step 3: Verify clean git state**

The `docs/web/` directory was created during brainstorming but never committed. Verify it's not tracked:

```bash
git status -s docs/web/
```

If there's output (files are staged/tracked), unstage them. If there's no output or only `??` markers, the deletion already cleaned things up.

---

### Task 7: Final verification

- [ ] **Step 1: Verify skill is discoverable in Julie repo**

```bash
ls .claude/skills/web-research/SKILL.md
```

Expected: file exists.

- [ ] **Step 2: Verify skill is in plugin repo**

```bash
ls ~/source/julie-plugin/skills/web-research/SKILL.md
```

Expected: file exists.

- [ ] **Step 3: End-to-end test**

Invoke the skill workflow manually:

1. Check browser39: `which browser39`
2. Fetch a test page (e.g., the Rust book intro):
   ```bash
   echo '{"id":"1","action":"fetch","v":1,"seq":1,"url":"https://doc.rust-lang.org/book/","options":{"selector":"main","strip_nav":true}}' > /tmp/b39-cmd.jsonl
   browser39 batch /tmp/b39-cmd.jsonl --output /tmp/b39-out.jsonl
   ```
3. Extract and save:
   ```bash
   mkdir -p docs/web/doc.rust-lang.org/book
   cat /tmp/b39-out.jsonl | python3 -c "
   import sys,json
   for line in sys.stdin:
       d=json.loads(line)
       if 'rust-lang.org' in d.get('url',''):
           print(d.get('markdown',''))
   " > docs/web/doc.rust-lang.org/book/index.md
   ```
4. Wait 2 seconds for indexing
5. Verify with Julie:
   ```
   get_symbols(file_path="docs/web/doc.rust-lang.org/book/index.md", mode="structure")
   ```
   Expected: hierarchical section structure
6. Clean up: `rm -rf docs/web/`

- [ ] **Step 4: Done**

All deliverables complete:
- `.claude/skills/web-research/SKILL.md` (julie repo)
- `~/source/julie-plugin/skills/web-research/SKILL.md` (plugin repo)
- CI workflow updated for 11 skills
- Both READMEs updated with browser39 optional prereq and skill listing
