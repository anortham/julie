# Julie GitHub Pages Site — Design Spec

**Date:** 2026-03-16
**Status:** Reviewed
**Goal:** Create a single-page GitHub Pages site for Julie that showcases what it does, how it works, and why it matters — geared toward developers using AI coding tools.

---

## Audience

**Primary:** Developers who already use AI coding tools (Claude Code, Cursor, Copilot, etc.) and want better code intelligence. They know what MCP is. The pitch: "Julie makes your AI sessions longer and more productive."

**Secondary:** Developers evaluating AI coding tools more broadly, who may not know MCP. The pitch: "Here's a thing that makes AI coding actually work well."

**Strategy:** Hero section speaks to the broad audience (the problem of burning context on file reads). Deeper sections reward scrollers with technical depth on the reference graph, code health intelligence, and embeddings.

---

## Design Direction

### Tone
**Visual/demo-forward.** Less copy, more showing. Animated terminal demos, before/after comparisons, scroll-triggered reveals. Let the tool sell itself.

### Visual Style
**Dark Terminal.** Dark background (#0a0a0f to #1a1a2e range), monospace accents, green-on-dark code blocks. Terminal-style demos that look like real output. Think Warp, Fig, Ghostty. Native to the developer audience.

### Page Structure
**Single page with anchored sections.** One long scroll with a sticky nav that appears after the hero. Feels multi-page but is technically one file. Smooth scroll between sections.

---

## Technical Architecture

### File Structure

```
docs/site/
├── index.html      # Single page, all sections
├── style.css       # Dark theme, animations, responsive layout
├── script.js       # Terminal typing, context drain, scroll triggers
└── assets/         # Static images if needed (optional)
```

### Deployment

GitHub Pages can serve from `docs/` or repo root, but **not** from an arbitrary subdirectory like `docs/site/`. Since `docs/` already contains developer documentation, we use a **GitHub Actions workflow** to deploy from `docs/site/`:

```yaml
# .github/workflows/pages.yml
name: Deploy site to GitHub Pages
on:
  push:
    branches: [main]
    paths: [docs/site/**]
permissions:
  pages: write
  id-token: write
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/configure-pages@v5
      - uses: actions/upload-pages-artifact@v3
        with:
          path: docs/site
      - id: deployment
        uses: actions/deploy-pages@v4
```

Settings required: repo Settings → Pages → Source → "GitHub Actions".

The workflow only triggers on changes to `docs/site/**`, so normal code pushes don't redeploy.

### Why `docs/site/` not `docs/`
The existing `docs/` directory contains developer documentation (ARCHITECTURE.md, TESTING_GUIDE.md, etc.). A subdirectory keeps the public site cleanly separated.

### Tech Approach
- **CSS custom properties** for the dark theme — all colors defined in one place, easy to tweak
- **Intersection Observer API** for scroll-triggered animations — terminal demos animate as you scroll to them
- **Pure CSS/JS terminal typing effect** — no library dependencies
- **Sticky nav** with smooth scroll to anchored sections
- **Responsive** — works on mobile, optimized for desktop (primary audience)
- **Browser targets:** Modern evergreen browsers (Chrome, Firefox, Safari, Edge). No IE11.

### Fonts

- **Body text:** System sans-serif stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif` — zero network cost
- **Terminal demos / code:** `"JetBrains Mono", "Fira Code", "Cascadia Code", "SF Mono", "Consolas", monospace` — loaded via Google Fonts (JetBrains Mono, ~20KB woff2) with system monospace fallback
- **Hero numbers / stats:** Same monospace stack, weight 700

### SEO & Social Sharing

The `<head>` must include:
- `<title>Julie — Code Intelligence for AI Agents</title>`
- `<meta name="description" content="LSP-quality code intelligence across 31 languages. 90% fewer tokens. <5ms search. Built in Rust with tree-sitter.">`
- Open Graph tags: `og:title`, `og:description`, `og:image` (a dark-themed screenshot of the hero section or a branded card), `og:url`
- Twitter card: `twitter:card=summary_large_image`, `twitter:title`, `twitter:description`, `twitter:image`
- Favicon: a simple monospace "J" on dark background, or Julie wordmark. SVG favicon preferred (single file, scales perfectly).
- `<link rel="canonical" href="https://anortham.github.io/julie/">` (update if custom domain added later)

### Accessibility

- **`prefers-reduced-motion`:** All Intersection Observer animations and the hero context drain animation must be wrapped in a `prefers-reduced-motion: no-preference` check. When reduced motion is preferred, show final states immediately (no typing, no slide-in, no count-up).
- **Color contrast:** All text on dark backgrounds must meet WCAG AA (4.5:1 for body text, 3:1 for large text). The green (#00ff88) on dark (#0a0a0f) passes — verify all accent colors.
- **Colorblind safety:** The token savings table uses red/green to differentiate "Without Julie" vs "With Julie." Add a secondary differentiator: ✗ icon + "Without" label for the red column, ✓ icon + "With Julie" label for the green column. Same for risk labels — use text labels (HIGH/MEDIUM/LOW) as primary, color as secondary.
- **Terminal demos:** Add `aria-label` describing the content for screen readers (e.g., `aria-label="Terminal showing fast_search finding UserService in 3ms"`).
- **Graph visualization:** Include a text summary below the SVG for screen readers (`aria-describedby` linking to a visually-hidden description of the relationships).

---

## Sections

### 1. Hero (full viewport height)

**Purpose:** Hook the broad audience by making the problem visceral, then show the solution.

**Layout:** Julie name/logo top-center. One-line tagline beneath. Below that, a two-phase animation:

**Phase 1 — "The Problem" (context drain)**
- Horizontal progress bar labeled "Context Window"
- Simulated file reads stack up beneath: `Reading src/services.rs... 2,847 tokens`, `Reading src/handler.rs... 1,923 tokens`, etc.
- Bar fills rapidly, color transitions green → yellow → red
- Running token counter climbs fast
- After ~6 file reads, bar nearly full, text appears: *"Session over. You read 6 files."*

**Phase 2 — "With Julie" (terminal demo)**
- Bar resets. Same task replays.
- Terminal types out: `fast_search("UserService", definitions)` → instant result, 300 tokens
- Then: `deep_dive("UserService", overview)` → callers, callees, types, 200 tokens
- Bar barely moves. Counter shows ~500 tokens total.
- Text: *"Same understanding. 90% fewer tokens."*

**Below animation:** Two CTA buttons:
- "Get Started" → scrolls to Installation section
- "View on GitHub" → external link to repo

**Sticky nav** appears after scrolling past hero: `How it Works · Reference Graph · Code Health · Embeddings · Tools · Languages · Install`

---

### 2. Token Savings Table

**Purpose:** Concrete proof — the README's comparison table, brought to life.

**Layout:** Each row animates in on scroll. Two columns — "Without Julie" in red-tinted token counts, "With Julie" in green. Savings percentage pulses briefly on appear.

| Task | Without Julie | With Julie | Savings |
|------|--------------|------------|---------|
| Understand a file's API | ~2,000 tokens | ~200 tokens | ~90% |
| Find a function | ~5,000+ tokens | ~300 tokens | ~94% |
| Investigate before modifying | ~4,000+ tokens | ~200 tokens | ~95% |
| Orient on new area | ~10,000+ tokens | ~2,000 tokens | ~80% |

---

### 3. How It Works

**Purpose:** Mental model — three steps, dead simple.

**Layout:** Three-step horizontal flow with connecting arrows:

1. **Parse** — "Tree-sitter extracts symbols, relationships, and types from 31 languages"
   - Visual: syntax tree icon
2. **Index** — "Tantivy full-text search + SQLite graph + KNN embeddings"
   - Visual: database/index icon
3. **Query** — "Agent asks a question, gets exactly what it needs — no file reads"
   - Visual: terminal/prompt icon

Clean, diagrammatic. Each step has a brief description and a small icon. Arrows connect them left to right.

---

### 4. Reference Graph

**Purpose:** The key differentiator — Julie understands code *relationships*, not just symbols.

**Headline:** *"Julie doesn't just find symbols — it understands how they connect."*

**Visual:** A graph diagram on the dark background. Central node (`process_payment`) with edges to:
- **Callers:** `handle_checkout`, `retry_payment`, `batch_processor`
- **Callees:** `validate_card`, `charge_gateway`, `emit_event`
- **Types:** `PaymentRequest → PaymentResult`

Nodes glow subtly, edges colored by relationship type (callers = blue, callees = green, types = purple). Implemented as **inline SVG** — edges use `stroke-dashoffset` animation to draw in on scroll. Static final state, but the entrance animation makes it feel alive.

**Below diagram:** Mini terminal demo:
```
deep_dive("process_payment", overview)
→ Callers: handle_checkout, retry_payment, batch_processor
→ Callees: validate_card, charge_gateway, emit_event
→ Types: PaymentRequest → PaymentResult
→ ~200 tokens. Zero file reads.
```

**Closing copy:** *"One call. Full picture. Zero file reads."*

---

### 5. Code Health Intelligence

**Purpose:** Show that Julie computes risk and test quality at index time — no other tool does this.

**Headline:** *"Risk scores and test quality — computed at index time, not runtime."*

**Layout:** Two side-by-side terminal-styled panels:

**Left — Test Intelligence:**
```
Tests: 3 found
  ✓ test_process_payment    (thorough — 8 assertions, error paths)
  ✓ test_payment_validation (adequate — 4 assertions)
  ○ test_payment_retry      (thin — 1 assertion, no error paths)
```

**Right — Risk Scoring:**
```
Change Risk: MEDIUM (0.66)
  → 8 callers · public · thorough tests

Security Risk: HIGH (0.84)
  → calls execute · public · accepts string params
  → sink calls: execute
  → untested: yes
```

Color-coded risk labels: green (LOW), yellow (MEDIUM), red (HIGH).

**Closing copy:** *"Every symbol scored. Every risk surfaced. Before you touch the code."*

---

### 6. Semantic Embeddings

**Purpose:** Show the "find what you mean" capability beyond keyword search.

**Headline:** *"Find what you mean, not just what you typed."*

**Visual:** Two terminal demos side by side:

**Left — Keyword search:**
```
fast_search("handle user login")
→ handle_user_login    (exact match)
→ login_handler        (partial match)
```

**Right — Semantic similarity:**
```
deep_dive("authenticate") → Related symbols:
→ verify_credentials    (0.92 similarity)
→ validate_session      (0.87 similarity)
→ check_permissions     (0.81 similarity)
```

**Below:** *"GPU-accelerated embeddings. Works with CUDA, MPS, and DirectML. Falls back to CPU gracefully."*

---

### 7. Tools Showcase

**Purpose:** Feature catalogue — each tool with a mini demo.

**Layout:** 2-column card grid. Each card has:
- Tool name + one-line description
- Mini terminal demo showing realistic input → output
- Token cost badge (e.g., "~200 tokens")

**Cards (7 total):**

1. **fast_search** — Full-text code search with code-aware tokenization. Demo: definition search finding `UserService` in 3ms.
2. **get_context** — Token-budgeted context for a concept or task. Demo: area-level orientation with pivots and neighbors.
3. **deep_dive** — Progressive-depth symbol investigation. Demo: overview mode showing callers/callees/types.
4. **fast_refs** — Find all references to a symbol. Demo: finding 12 references across 8 files.
5. **get_symbols** — Smart file reading with 70-90% token savings. Demo: file structure mode showing 20-line overview of a 500-line file.
6. **rename_symbol** — Workspace-wide rename with dry-run preview. Demo: renaming across 15 files with preview diff.
7. **manage_workspace** — Index, add, remove, refresh workspaces. Demo: indexing a workspace in <2s.

Cards fade in on scroll, staggered.

---

### 8. Language Support

**Purpose:** The "whoa, that's a lot" moment.

**Layout:** Grid of 31 language badges, grouped by category:

- **Core (10):** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin
- **Systems (5):** C, C++, Go, Lua, Zig
- **Specialized (12):** GDScript, Vue, QML, R, Razor, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart
- **Documentation (4):** Markdown, JSON, TOML, YAML

*Note: JSONL is handled by the JSON extractor, not a separate language. 10 + 5 + 12 + 4 = 31.*

Each badge: language name with a subtle colored dot or icon. Whole grid visible at once — no accordion or tabs.

---

### 9. Performance Stats

**Purpose:** The big numbers, bold and undeniable.

**Layout:** Three large monospace figures in a row:

```
<5ms           <100MB          <2s
Search         Memory          Startup
```

**Subtitle:** *"Incremental updates: only changed files re-indexed. Typically 3-15 seconds."*

---

### 10. Installation

**Purpose:** Get started in 30 seconds.

**Layout:** Tabbed code blocks — click **Claude Code** / **VS Code** / **Cursor** to see the right snippet:

**Claude Code:**
```bash
git clone https://github.com/anortham/julie.git
cd julie && cargo build --release
claude mcp add julie -- /path/to/julie/target/release/julie-server
```

**VS Code (GitHub Copilot)** — `.vscode/mcp.json` snippet

**Cursor / Windsurf / Other** — `mcpServers` JSON snippet

**Below:** *"Julie indexes your workspace automatically on first connection (~2-5s for most projects)."*

---

### 11. Footer

**Layout:** Clean, minimal. Single row:
- GitHub repo link
- MIT License
- *"Built in Rust with tree-sitter and Tantivy"*
- Version number

---

## Responsive Behavior

- **Desktop (>1024px):** Full layout as described. Side-by-side panels, 2-column tool grid, horizontal 3-step flow.
- **Tablet (768-1024px):** Panels stack vertically, tool grid becomes single column, 3-step flow stays horizontal.
- **Mobile (<768px):** Everything stacks. Hero animation scales down. Sticky nav becomes a hamburger or horizontal scroll. Terminal demos get horizontal scroll if they overflow.

---

## Animation Strategy

All animations use **Intersection Observer** — nothing plays until you scroll to it. This keeps the page fast on load and prevents "animation fatigue." All animations respect `prefers-reduced-motion` (see Accessibility section).

- **Hero context drain:** Plays once on page load (above fold), then stops. Uses a `data-played` flag — does not replay if user scrolls away and back. This is the only section with the full typing effect.
- **Token savings table:** Rows slide in from left, staggered 100ms apart.
- **How It Works arrows:** Draw left-to-right (SVG `stroke-dashoffset`) as you scroll to the section.
- **Reference graph:** SVG nodes appear one by one, then edges draw between them via `stroke-dashoffset`.
- **Terminal demos (non-hero):** Instant reveal with a blinking cursor at the end, rather than character-by-character typing. Typing animation on every terminal demo would take too long and become tedious. The hero earns the slow typing; subsequent demos should feel snappy.
- **Tool cards:** Fade in and slide up, staggered.
- **Language badges:** Quick cascade appearance.
- **Performance numbers:** Count up from 0 to final value using `requestAnimationFrame`. Animate the numeric portion only (e.g., 0→5), then append the unit ("ms"). Duration: ~1s.

**Replay:** Animations play once per page load. No infinite loops, no attention-grabbing motion after the initial reveal.

### Anchor IDs

Sticky nav links and their corresponding section IDs:

| Nav Label | Section ID |
|-----------|-----------|
| How it Works | `#how-it-works` |
| Reference Graph | `#reference-graph` |
| Code Health | `#code-health` |
| Embeddings | `#embeddings` |
| Tools | `#tools` |
| Languages | `#languages` |
| Install | `#install` |

### Interactive Details

- **Installation code blocks:** Include a copy-to-clipboard button (small clipboard icon, top-right of each code block). On click, copies the snippet and shows a brief "Copied!" tooltip. Trivial to implement with `navigator.clipboard.writeText()`.
- **Version number in footer:** Hardcoded. Accepted as potentially stale between releases — this is a static site with no build step. Can be updated manually during release, or automated later if warranted.
- **404 page:** Include a simple `404.html` in `docs/site/` — dark themed, "Page not found" message with a link back to the main page. GitHub Pages serves this automatically.

---

## Out of Scope (for now)

- Blog / changelog page
- Search within the site
- Dark/light mode toggle (it's always dark)
- Analytics / tracking
- Custom domain (uses default `anortham.github.io/julie`)
- Static site generator migration

---

## Resolved Questions

1. **Julie logo:** Styled monospace text for v1. A proper logo can come later — styled text in the dark terminal aesthetic looks intentional, not cheap.
2. **Real terminal recordings vs simulated:** Simulated (pure CSS/JS). Asciinema embeds are heavy, hard to theme consistently, and can't be tuned for timing. Full control over pacing and styling.
3. **`.superpowers/` in `.gitignore`:** The brainstorm mockups in `.superpowers/brainstorm/` should be added to `.gitignore` — they're ephemeral design artifacts, not project knowledge.

## Performance Budget

- **Total page weight:** Under 150KB (HTML + CSS + JS + font). No images in v1 (all visuals are CSS/SVG).
- **First Contentful Paint:** Under 1s on broadband, under 2s on 3G.
- **No external dependencies** beyond the JetBrains Mono font from Google Fonts (loaded async, system monospace fallback).
