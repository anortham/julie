# Julie GitHub Pages Site — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a single-page GitHub Pages site showcasing Julie's code intelligence capabilities, with dark terminal theme, demo-forward design, and scroll-triggered animations.

**Architecture:** Pure static HTML/CSS/JS in `docs/site/`, deployed via GitHub Actions workflow. No build step, no dependencies beyond one Google Font. Single `index.html` with anchored sections, `style.css` for dark theme + animations + responsive layout, `script.js` for hero animation + scroll observers + interactive elements.

**Tech Stack:** HTML5, CSS3 (custom properties, grid, flexbox, animations), vanilla JavaScript (Intersection Observer, requestAnimationFrame), inline SVG for graphs/diagrams.

**Spec:** `docs/superpowers/specs/2026-03-16-github-pages-site-design.md`

---

## Chunk 1: Foundation + Hero

### Task 1: Project Scaffolding

**Files:**
- Create: `docs/site/index.html`
- Create: `docs/site/style.css`
- Create: `docs/site/script.js`
- Create: `docs/site/favicon.svg`
- Create: `.github/workflows/pages.yml`
- Modify: `.gitignore`

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p docs/site
```

- [ ] **Step 2: Create the GitHub Actions workflow**

Create `.github/workflows/pages.yml`:

```yaml
name: Deploy site to GitHub Pages
on:
  push:
    branches: [main]
    paths: ['docs/site/**']
permissions:
  pages: write
  id-token: write
concurrency:
  group: pages
  cancel-in-progress: false
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

- [ ] **Step 3: Create SVG favicon**

Create `docs/site/favicon.svg` — a monospace "J" on dark background:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <rect width="32" height="32" rx="6" fill="#0a0a0f"/>
  <text x="16" y="23" font-family="monospace" font-size="20" font-weight="700" fill="#00ff88" text-anchor="middle">J</text>
</svg>
```

- [ ] **Step 4: Create `index.html` with full `<head>` and empty body skeleton**

Create `docs/site/index.html`. This is the complete `<head>` with SEO, OG tags, fonts, and the body skeleton with all 11 section divs using correct anchor IDs. Sections are empty placeholders — subsequent tasks fill them in.

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Julie — Code Intelligence for AI Agents</title>
  <meta name="description" content="LSP-quality code intelligence across 31 languages. 90% fewer tokens. <5ms search. Built in Rust with tree-sitter.">
  <link rel="icon" type="image/svg+xml" href="favicon.svg">
  <link rel="canonical" href="https://anortham.github.io/julie/">

  <!-- Open Graph -->
  <meta property="og:type" content="website">
  <meta property="og:title" content="Julie — Code Intelligence for AI Agents">
  <meta property="og:description" content="LSP-quality code intelligence across 31 languages. 90% fewer tokens. <5ms search. Built in Rust with tree-sitter.">
  <meta property="og:url" content="https://anortham.github.io/julie/">

  <!-- Open Graph image (TODO: replace with actual screenshot/card after site is live) -->
  <meta property="og:image" content="https://anortham.github.io/julie/og-card.png">

  <!-- Twitter Card -->
  <meta name="twitter:card" content="summary_large_image">
  <meta name="twitter:title" content="Julie — Code Intelligence for AI Agents">
  <meta name="twitter:description" content="LSP-quality code intelligence across 31 languages. 90% fewer tokens. <5ms search. Built in Rust with tree-sitter.">
  <meta name="twitter:image" content="https://anortham.github.io/julie/og-card.png">

  <!-- Fonts -->
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;700&display=swap">

  <link rel="stylesheet" href="style.css">
</head>
<body>
  <!-- Sticky Nav (hidden until scroll past hero) -->
  <nav id="sticky-nav" class="sticky-nav" aria-label="Main navigation">
    <div class="nav-inner">
      <a href="#" class="nav-logo">Julie</a>
      <div class="nav-links">
        <a href="#how-it-works">How it Works</a>
        <a href="#reference-graph">Reference Graph</a>
        <a href="#code-health">Code Health</a>
        <a href="#embeddings">Embeddings</a>
        <a href="#tools">Tools</a>
        <a href="#languages">Languages</a>
        <a href="#install">Install</a>
      </div>
    </div>
  </nav>

  <!-- Section 1: Hero -->
  <section id="hero" class="hero">
    <!-- Task 3 fills this -->
  </section>

  <!-- Section 2: Token Savings -->
  <section id="token-savings" class="section">
    <!-- Task 4 fills this -->
  </section>

  <!-- Section 3: How It Works -->
  <section id="how-it-works" class="section">
    <!-- Task 5 fills this -->
  </section>

  <!-- Section 4: Reference Graph -->
  <section id="reference-graph" class="section">
    <!-- Task 5 fills this -->
  </section>

  <!-- Section 5: Code Health -->
  <section id="code-health" class="section">
    <!-- Task 6 fills this -->
  </section>

  <!-- Section 6: Embeddings -->
  <section id="embeddings" class="section">
    <!-- Task 6 fills this -->
  </section>

  <!-- Section 7: Tools -->
  <section id="tools" class="section">
    <!-- Task 7 fills this -->
  </section>

  <!-- Section 8: Languages -->
  <section id="languages" class="section">
    <!-- Task 8 fills this -->
  </section>

  <!-- Section 9: Performance -->
  <section id="performance" class="section">
    <!-- Task 8 fills this -->
  </section>

  <!-- Section 10: Installation -->
  <section id="install" class="section">
    <!-- Task 8 fills this -->
  </section>

  <!-- Section 11: Footer -->
  <footer id="footer" class="footer">
    <!-- Task 8 fills this -->
  </footer>

  <script src="script.js"></script>
</body>
</html>
```

- [ ] **Step 5: Create `style.css` with CSS custom properties and reset**

Create `docs/site/style.css` with just the foundation — theme variables, reset, and basic layout. Section-specific styles are added in their respective tasks.

```css
/* ============================================
   Julie Site — CSS Foundation
   ============================================ */

/* --- Theme Variables --- */
:root {
  --bg-primary: #0a0a0f;
  --bg-secondary: #12121a;
  --bg-terminal: #0d0d14;
  --bg-card: #16161f;
  --bg-nav: rgba(10, 10, 15, 0.9);

  --text-primary: #e0e0e8;
  --text-secondary: #8888a0;
  --text-muted: #55556a;

  --accent-green: #00ff88;
  --accent-blue: #6495ed;
  --accent-purple: #a78bfa;
  --accent-red: #ff4444;
  --accent-yellow: #ffc107;
  --accent-orange: #ff8c00;
  --accent-cyan: #00c8ff;

  --font-body: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  --font-mono: "JetBrains Mono", "Fira Code", "Cascadia Code", "SF Mono", Consolas, monospace;

  --max-width: 1100px;
  --section-padding: 100px 24px;
  --border-radius: 8px;
  --terminal-radius: 10px;
}

/* --- Reset --- */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html {
  scroll-behavior: smooth;
  font-size: 16px;
}

body {
  font-family: var(--font-body);
  background: var(--bg-primary);
  color: var(--text-primary);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

a {
  color: var(--accent-green);
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}

/* --- Layout --- */
.section {
  padding: var(--section-padding);
  max-width: var(--max-width);
  margin: 0 auto;
}

.section-title {
  font-size: 2rem;
  font-weight: 700;
  margin-bottom: 12px;
  color: var(--text-primary);
}

.section-subtitle {
  font-size: 1.1rem;
  color: var(--text-secondary);
  margin-bottom: 48px;
  max-width: 600px;
}

/* --- Sticky Nav --- */
.sticky-nav {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 100;
  background: var(--bg-nav);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  transform: translateY(-100%);
  transition: transform 0.3s ease;
}

.sticky-nav.visible {
  transform: translateY(0);
}

.nav-inner {
  max-width: var(--max-width);
  margin: 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 24px;
}

.nav-logo {
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 1.1rem;
  color: var(--accent-green);
}

.nav-logo:hover {
  text-decoration: none;
}

.nav-links {
  display: flex;
  gap: 24px;
}

.nav-links a {
  font-size: 0.85rem;
  color: var(--text-secondary);
  transition: color 0.2s;
}

.nav-links a:hover,
.nav-links a.active {
  color: var(--text-primary);
  text-decoration: none;
}

/* --- Terminal Block --- */
.terminal {
  background: var(--bg-terminal);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: var(--terminal-radius);
  padding: 20px 24px;
  font-family: var(--font-mono);
  font-size: 0.85rem;
  line-height: 1.7;
  overflow-x: auto;
  position: relative;
}

.terminal-header {
  display: flex;
  gap: 6px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.terminal-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.terminal-dot.red { background: #ff5f56; }
.terminal-dot.yellow { background: #ffbd2e; }
.terminal-dot.green { background: #27c93f; }

.terminal .prompt { color: var(--text-muted); }
.terminal .command { color: var(--accent-green); }
.terminal .result { color: var(--text-primary); }
.terminal .comment { color: var(--text-muted); }
.terminal .highlight { color: var(--accent-cyan); }
.terminal .warn { color: var(--accent-yellow); }
.terminal .error { color: var(--accent-red); }

/* Blinking cursor for non-hero terminal demos */
.terminal .cursor {
  display: inline-block;
  width: 8px;
  height: 1.1em;
  background: var(--accent-green);
  vertical-align: text-bottom;
  animation: blink 1s step-end infinite;
}

@keyframes blink {
  50% { opacity: 0; }
}

/* --- Side-by-side panels --- */
.split-panels {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 24px;
}

/* --- Card grid --- */
.card-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 24px;
}

.card {
  background: var(--bg-card);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: var(--border-radius);
  padding: 24px;
  transition: border-color 0.2s;
}

.card:hover {
  border-color: rgba(255, 255, 255, 0.12);
}

/* --- Scroll animation base --- */
.animate-on-scroll {
  opacity: 0;
  transform: translateY(20px);
  transition: opacity 0.5s ease, transform 0.5s ease;
}

.animate-on-scroll.visible {
  opacity: 1;
  transform: translateY(0);
}

/* --- Reduced motion --- */
@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
  .animate-on-scroll {
    opacity: 1;
    transform: none;
    transition: none;
  }
  .terminal .cursor { animation: none; opacity: 1; }
  .sticky-nav { transition: none; }
}

/* --- Responsive --- */
@media (max-width: 768px) {
  :root {
    --section-padding: 60px 16px;
  }

  .split-panels,
  .card-grid {
    grid-template-columns: 1fr;
  }

  .nav-links {
    gap: 12px;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    scrollbar-width: none;
  }

  .nav-links::-webkit-scrollbar { display: none; }

  .section-title { font-size: 1.5rem; }
}

@media (max-width: 480px) {
  .nav-links a { font-size: 0.75rem; }
  .terminal { font-size: 0.75rem; padding: 14px 16px; }
}
```

- [ ] **Step 6: Create empty `script.js` with nav scroll logic**

Create `docs/site/script.js` — start with the sticky nav show/hide and active section tracking. Animation utilities are added in subsequent tasks.

```javascript
/* ============================================
   Julie Site — Script
   ============================================ */

(function () {
  'use strict';

  // --- Reduced motion check (live query — responds to runtime changes) ---
  const reducedMotionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
  function prefersReducedMotion() { return reducedMotionQuery.matches; }

  // --- Sticky nav show/hide ---
  const nav = document.getElementById('sticky-nav');
  const hero = document.getElementById('hero');

  if (nav && hero) {
    const observer = new IntersectionObserver(
      ([entry]) => {
        nav.classList.toggle('visible', !entry.isIntersecting);
      },
      { threshold: 0 }
    );
    observer.observe(hero);
  }

  // --- Active nav link tracking ---
  const navLinks = document.querySelectorAll('.nav-links a');
  const sections = document.querySelectorAll('.section, .hero');

  if (navLinks.length > 0 && sections.length > 0) {
    const sectionObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const id = entry.target.id;
            navLinks.forEach((link) => {
              link.classList.toggle('active', link.getAttribute('href') === '#' + id);
            });
          }
        });
      },
      { rootMargin: '-40% 0px -60% 0px' }
    );
    sections.forEach((s) => sectionObserver.observe(s));
  }

  // --- Scroll-triggered animation utility ---
  // Elements with class "animate-on-scroll" become visible when scrolled into view.
  // For staggered animations, add data-delay="100" (ms) on each element.
  function initScrollAnimations() {
    if (prefersReducedMotion()) {
      // Show everything immediately
      document.querySelectorAll('.animate-on-scroll').forEach((el) => {
        el.classList.add('visible');
      });
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const delay = parseInt(entry.target.dataset.delay || '0', 10);
            setTimeout(() => entry.target.classList.add('visible'), delay);
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.1 }
    );

    document.querySelectorAll('.animate-on-scroll').forEach((el) => {
      observer.observe(el);
    });
  }

  // Run after DOM is ready (script is at bottom of body, so DOM is ready)
  initScrollAnimations();
})();
```

- [ ] **Step 7: Add `.superpowers/` to `.gitignore`**

Append to `.gitignore`:

```
# Brainstorm mockups (ephemeral)
.superpowers/
```

- [ ] **Step 8: Verify in browser**

Open `docs/site/index.html` directly in a browser (`open docs/site/index.html` on macOS). Verify:
- Dark background renders
- Sticky nav is hidden (hero is visible)
- Scrolling down would show the nav (once sections have content)
- No console errors

- [ ] **Step 9: Commit**

```bash
git add docs/site/ .github/workflows/pages.yml .gitignore
git commit -m "feat(site): scaffold GitHub Pages site with dark theme foundation"
```

---

### Task 2: CSS for Section-Specific Components

**Files:**
- Modify: `docs/site/style.css`

This task adds all the CSS that section-building tasks (3-8) will need. Adding it upfront means HTML tasks can focus purely on markup and JS, without interleaving CSS changes.

- [ ] **Step 1: Add hero section styles**

Append to `style.css`:

```css
/* ============================================
   Hero Section
   ============================================ */
.hero {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 24px;
  text-align: center;
  position: relative;
}

.hero-title {
  font-family: var(--font-mono);
  font-size: 3.5rem;
  font-weight: 700;
  color: var(--accent-green);
  margin-bottom: 12px;
  letter-spacing: -1px;
}

.hero-tagline {
  font-size: 1.2rem;
  color: var(--text-secondary);
  margin-bottom: 48px;
  max-width: 500px;
}

/* Context window progress bar */
.context-bar-container {
  width: 100%;
  max-width: 600px;
  margin-bottom: 24px;
}

.context-bar-label {
  display: flex;
  justify-content: space-between;
  font-family: var(--font-mono);
  font-size: 0.8rem;
  color: var(--text-secondary);
  margin-bottom: 8px;
}

.context-bar {
  width: 100%;
  height: 24px;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 12px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.context-bar-fill {
  height: 100%;
  width: 0%;
  border-radius: 12px;
  transition: width 0.3s ease, background 0.3s ease;
  background: var(--accent-green);
}

.context-bar-fill.warning { background: var(--accent-yellow); }
.context-bar-fill.danger { background: var(--accent-red); }

/* File read log */
.file-reads {
  width: 100%;
  max-width: 600px;
  text-align: left;
  min-height: 200px;
}

.file-read-line {
  font-family: var(--font-mono);
  font-size: 0.8rem;
  padding: 4px 0;
  opacity: 0;
  transform: translateX(-10px);
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.file-read-line.visible {
  opacity: 1;
  transform: translateX(0);
}

.file-read-line .filename { color: var(--text-secondary); }
.file-read-line .tokens { color: var(--accent-orange); }
.file-read-line .julie-cmd { color: var(--accent-green); }
.file-read-line .julie-result { color: var(--accent-cyan); }

.hero-message {
  font-family: var(--font-mono);
  font-size: 1.1rem;
  margin-top: 20px;
  opacity: 0;
  transition: opacity 0.5s ease;
}

.hero-message.visible { opacity: 1; }
.hero-message.problem { color: var(--accent-red); }
.hero-message.solution { color: var(--accent-green); }

/* CTA buttons */
.hero-ctas {
  display: flex;
  gap: 16px;
  margin-top: 36px;
  opacity: 0;
  transition: opacity 0.5s ease;
}

.hero-ctas.visible { opacity: 1; }

.btn {
  font-family: var(--font-mono);
  font-size: 0.9rem;
  font-weight: 700;
  padding: 12px 28px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.2s, transform 0.1s;
  border: none;
  text-decoration: none;
}

.btn:hover { transform: translateY(-1px); text-decoration: none; }

.btn-primary {
  background: var(--accent-green);
  color: var(--bg-primary);
}

.btn-primary:hover { background: #00e07a; }

.btn-secondary {
  background: transparent;
  color: var(--text-primary);
  border: 1.5px solid rgba(255, 255, 255, 0.2);
}

.btn-secondary:hover {
  border-color: rgba(255, 255, 255, 0.4);
  color: var(--text-primary);
}

@media (max-width: 768px) {
  .hero-title { font-size: 2.2rem; }
  .hero-tagline { font-size: 1rem; }
  .hero-ctas { flex-direction: column; align-items: center; }
}

@media (prefers-reduced-motion: reduce) {
  .file-read-line {
    opacity: 1;
    transform: none;
    transition: none;
  }
  .hero-message, .hero-ctas {
    opacity: 1;
    transition: none;
  }
  .context-bar-fill { transition: none; }
}
```

- [ ] **Step 2: Add token savings table styles**

Append to `style.css`:

```css
/* ============================================
   Token Savings Table
   ============================================ */
.savings-table {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0 8px;
}

.savings-table th {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 1px;
  color: var(--text-muted);
  text-align: left;
  padding: 8px 16px;
}

.savings-table td {
  padding: 16px;
  font-size: 0.95rem;
}

.savings-table tr.data-row {
  background: var(--bg-card);
}

.savings-table tr.data-row td:first-child {
  border-radius: var(--border-radius) 0 0 var(--border-radius);
  color: var(--text-primary);
}

.savings-table tr.data-row td:last-child {
  border-radius: 0 var(--border-radius) var(--border-radius) 0;
}

.savings-table .without {
  color: var(--accent-red);
  font-family: var(--font-mono);
  font-size: 0.9rem;
}

.savings-table .with-julie {
  color: var(--accent-green);
  font-family: var(--font-mono);
  font-size: 0.9rem;
}

.savings-table .savings-pct {
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 1.1rem;
  color: var(--accent-green);
}

.savings-icon {
  display: inline-block;
  margin-right: 6px;
  font-size: 0.85rem;
}

/* Savings table rows slide in from left (not up) */
.savings-table tr.animate-on-scroll {
  opacity: 0;
  transform: translateX(-20px);
  transition: opacity 0.5s ease, transform 0.5s ease;
}

.savings-table tr.animate-on-scroll.visible {
  opacity: 1;
  transform: translateX(0);
}

/* Savings percentage pulse on appear */
@keyframes pulse-savings {
  0% { transform: scale(1); }
  50% { transform: scale(1.15); }
  100% { transform: scale(1); }
}

.savings-table tr.animate-on-scroll.visible .savings-pct {
  animation: pulse-savings 0.4s ease 0.3s;
}

@media (max-width: 768px) {
  .savings-table { font-size: 0.85rem; }
  .savings-table td { padding: 12px 10px; }
  .savings-table th { font-size: 0.65rem; }
}

@media (prefers-reduced-motion: reduce) {
  .savings-table tr.animate-on-scroll {
    opacity: 1;
    transform: none;
    transition: none;
  }
  .savings-table tr.animate-on-scroll.visible .savings-pct {
    animation: none;
  }
}
```

- [ ] **Step 3: Add How It Works + Reference Graph styles**

Append to `style.css`:

```css
/* ============================================
   How It Works — 3-Step Flow
   ============================================ */
.steps-flow {
  display: flex;
  align-items: flex-start;
  justify-content: center;
  gap: 0;
  position: relative;
}

.step {
  flex: 1;
  max-width: 280px;
  text-align: center;
  padding: 0 20px;
}

.step-icon {
  width: 64px;
  height: 64px;
  margin: 0 auto 16px;
  background: var(--bg-card);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1.5rem;
}

.step-number {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--accent-green);
  text-transform: uppercase;
  letter-spacing: 2px;
  margin-bottom: 8px;
}

.step h3 {
  font-size: 1.1rem;
  margin-bottom: 8px;
  color: var(--text-primary);
}

.step p {
  font-size: 0.9rem;
  color: var(--text-secondary);
  line-height: 1.5;
}

/* SVG arrows between steps */
.step-arrow {
  width: 60px;
  flex-shrink: 0;
  align-self: center;
  margin-top: -20px;
}

.step-arrow line {
  stroke: rgba(255, 255, 255, 0.15);
  stroke-width: 2;
}

.step-arrow polygon {
  fill: rgba(255, 255, 255, 0.15);
}

@media (max-width: 768px) {
  .steps-flow {
    flex-direction: column;
    align-items: center;
    gap: 24px;
  }
  .step-arrow {
    transform: rotate(90deg);
    width: 40px;
    margin-top: 0;
  }
}

/* ============================================
   Reference Graph
   ============================================ */
.graph-container {
  width: 100%;
  max-width: 700px;
  margin: 0 auto 32px;
}

.graph-container svg {
  width: 100%;
  height: auto;
}

.graph-node {
  cursor: default;
}

.graph-node rect {
  rx: 8;
  ry: 8;
  fill: var(--bg-card);
  stroke: rgba(255, 255, 255, 0.1);
  stroke-width: 1;
  transition: filter 0.3s;
}

.graph-node text {
  font-family: var(--font-mono);
  font-size: 12px;
  fill: var(--text-primary);
  text-anchor: middle;
  dominant-baseline: central;
}

.graph-node.central rect {
  fill: rgba(0, 255, 136, 0.1);
  stroke: var(--accent-green);
  stroke-width: 1.5;
}

.graph-node.central text {
  fill: var(--accent-green);
  font-weight: 700;
}

.graph-edge {
  fill: none;
  stroke-width: 1.5;
  stroke-linecap: round;
}

.graph-edge.callers { stroke: var(--accent-blue); }
.graph-edge.callees { stroke: var(--accent-green); }
.graph-edge.types { stroke: var(--accent-purple); }

/* Edge labels */
.graph-label {
  font-family: var(--font-mono);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 1px;
}

.graph-label.callers { fill: var(--accent-blue); }
.graph-label.callees { fill: var(--accent-green); }
.graph-label.types { fill: var(--accent-purple); }

/* stroke-dashoffset entrance animation
   Default state: visible (works without JS). JS adds .graph-animate-ready
   to hide elements, then .shown/.drawn to reveal them on scroll. */
.graph-animate-ready .graph-edge-animated {
  transition: stroke-dashoffset 0.8s ease;
  /* stroke-dasharray/offset set by JS using getTotalLength() */
}

.graph-edge-animated.drawn {
  stroke-dashoffset: 0 !important;
}

.graph-animate-ready .graph-node-animated {
  opacity: 0;
  transition: opacity 0.4s ease;
}

.graph-node-animated.shown {
  opacity: 1;
}

@media (prefers-reduced-motion: reduce) {
  .graph-animate-ready .graph-edge-animated {
    stroke-dashoffset: 0 !important;
    transition: none;
  }
  .graph-animate-ready .graph-node-animated {
    opacity: 1;
    transition: none;
  }
}

/* Visually hidden for screen readers */
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  border: 0;
}
```

- [ ] **Step 4: Add Code Health, Embeddings, Tools, Languages, Performance, Installation, and Footer styles**

Append to `style.css`:

```css
/* ============================================
   Code Health — Risk labels
   ============================================ */
.risk-low { color: var(--accent-green); }
.risk-medium { color: var(--accent-yellow); }
.risk-high { color: var(--accent-red); }

/* ============================================
   Tools Showcase
   ============================================ */
.tool-card .tool-name {
  font-family: var(--font-mono);
  font-size: 1rem;
  font-weight: 700;
  color: var(--accent-green);
  margin-bottom: 4px;
}

.tool-card .tool-desc {
  font-size: 0.9rem;
  color: var(--text-secondary);
  margin-bottom: 16px;
}

.tool-card .terminal {
  font-size: 0.78rem;
  margin-bottom: 12px;
}

.token-badge {
  display: inline-block;
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--accent-green);
  background: rgba(0, 255, 136, 0.08);
  border: 1px solid rgba(0, 255, 136, 0.15);
  padding: 3px 10px;
  border-radius: 12px;
}

/* ============================================
   Language Badge Grid
   ============================================ */
.lang-category {
  margin-bottom: 24px;
}

.lang-category-label {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 1.5px;
  color: var(--text-muted);
  margin-bottom: 12px;
}

.lang-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.lang-badge {
  font-family: var(--font-mono);
  font-size: 0.82rem;
  padding: 6px 14px;
  background: var(--bg-card);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 6px;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.lang-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* ============================================
   Performance Stats
   ============================================ */
.perf-stats {
  display: flex;
  justify-content: center;
  gap: 80px;
  margin-bottom: 24px;
}

.perf-stat {
  text-align: center;
}

.perf-number {
  font-family: var(--font-mono);
  font-size: 3.5rem;
  font-weight: 700;
  color: var(--accent-green);
  line-height: 1;
  margin-bottom: 8px;
}

.perf-label {
  font-family: var(--font-mono);
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 2px;
  color: var(--text-muted);
}

.perf-subtitle {
  text-align: center;
  color: var(--text-secondary);
  font-size: 0.95rem;
}

@media (max-width: 768px) {
  .perf-stats { gap: 32px; }
  .perf-number { font-size: 2.2rem; }
}

/* ============================================
   Installation — Tabbed Code Blocks
   ============================================ */
.install-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  margin-bottom: 0;
}

.install-tab {
  font-family: var(--font-mono);
  font-size: 0.85rem;
  padding: 10px 20px;
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: color 0.2s, border-color 0.2s;
}

.install-tab:hover { color: var(--text-secondary); }

.install-tab.active {
  color: var(--accent-green);
  border-bottom-color: var(--accent-green);
}

.install-panel {
  display: none;
}

.install-panel.active {
  display: block;
}

.install-terminal {
  border-radius: 0 0 var(--terminal-radius) var(--terminal-radius);
  border-top: none;
}

/* Copy button */
.copy-btn {
  position: absolute;
  top: 12px;
  right: 12px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 4px 8px;
  font-family: var(--font-mono);
  font-size: 0.7rem;
  color: var(--text-muted);
  cursor: pointer;
  transition: color 0.2s, background 0.2s;
}

.copy-btn:hover {
  color: var(--text-primary);
  background: rgba(255, 255, 255, 0.1);
}

.copy-btn.copied {
  color: var(--accent-green);
}

/* ============================================
   Footer
   ============================================ */
.footer {
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  padding: 32px 24px;
  text-align: center;
}

.footer-inner {
  max-width: var(--max-width);
  margin: 0 auto;
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 24px;
  flex-wrap: wrap;
  font-size: 0.85rem;
  color: var(--text-muted);
}

.footer-inner a {
  color: var(--text-secondary);
}

.footer-sep {
  color: rgba(255, 255, 255, 0.1);
}
```

- [ ] **Step 5: Verify in browser**

Open `docs/site/index.html`. Page should render dark background, empty sections, and the styles should load without errors. Check the browser console for no 404s (CSS, font loading).

- [ ] **Step 6: Commit**

```bash
git add docs/site/style.css
git commit -m "feat(site): add all section-specific CSS styles"
```

---

### Task 3: Hero Section

**Files:**
- Modify: `docs/site/index.html` (hero section)
- Modify: `docs/site/script.js` (hero animation)

- [ ] **Step 1: Add hero HTML**

Replace the `<!-- Task 3 fills this -->` comment in the hero section with:

```html
  <h1 class="hero-title">Julie</h1>
  <p class="hero-tagline">Code intelligence that gives your AI agent its memory back.</p>

  <div class="context-bar-container">
    <div class="context-bar-label">
      <span>Context Window</span>
      <span id="token-counter">0 tokens</span>
    </div>
    <div class="context-bar">
      <div id="context-bar-fill" class="context-bar-fill"></div>
    </div>
  </div>

  <div id="file-reads" class="file-reads" aria-label="Animation showing token consumption comparison: without Julie vs with Julie"></div>

  <div id="hero-message" class="hero-message"></div>

  <div id="hero-ctas" class="hero-ctas">
    <a href="#install" class="btn btn-primary">Get Started</a>
    <a href="https://github.com/anortham/julie" class="btn btn-secondary" target="_blank" rel="noopener">View on GitHub</a>
  </div>
```

- [ ] **Step 2: Add hero animation JS**

Add the hero animation to `script.js`, inside the IIFE, before the `initScrollAnimations()` call:

```javascript
  // --- Hero animation ---
  function runHeroAnimation() {
    const bar = document.getElementById('context-bar-fill');
    const counter = document.getElementById('token-counter');
    const reads = document.getElementById('file-reads');
    const message = document.getElementById('hero-message');
    const ctas = document.getElementById('hero-ctas');

    if (!bar || !counter || !reads || !message || !ctas) return;

    // Hero animation runs once on page load (above fold, no Intersection Observer).
    // It's called exactly once by runHeroAnimation() below — no replay guard needed.

    // Skip animation for reduced motion
    if (prefersReducedMotion()) {
      bar.style.width = '5%';
      bar.className = 'context-bar-fill';
      counter.textContent = '~500 tokens';
      reads.innerHTML = [
        '<div class="file-read-line visible"><span class="julie-cmd">fast_search("UserService", definitions)</span> → <span class="julie-result">Found · 300 tokens</span></div>',
        '<div class="file-read-line visible"><span class="julie-cmd">deep_dive("UserService", overview)</span> → <span class="julie-result">Full picture · 200 tokens</span></div>',
      ].join('');
      message.textContent = 'Same understanding. 90% fewer tokens.';
      message.className = 'hero-message visible solution';
      ctas.classList.add('visible');
      return;
    }

    const fileReads = [
      { file: 'src/services/user.rs', tokens: 2847 },
      { file: 'src/services/auth.rs', tokens: 1923 },
      { file: 'src/handlers/api.rs', tokens: 3104 },
      { file: 'src/models/user.rs', tokens: 1456 },
      { file: 'src/database/queries.rs', tokens: 2231 },
      { file: 'src/middleware/session.rs', tokens: 1889 },
    ];
    const totalCapacity = 16000;
    let currentTokens = 0;
    let lineIndex = 0;

    // Phase 1: The Problem
    function addFileRead() {
      if (lineIndex >= fileReads.length) {
        // Session over
        setTimeout(() => {
          message.textContent = 'Session over. You read 6 files.';
          message.className = 'hero-message visible problem';
          // Pause, then Phase 2
          setTimeout(startPhase2, 2000);
        }, 500);
        return;
      }

      const fr = fileReads[lineIndex];
      currentTokens += fr.tokens;
      const pct = Math.min((currentTokens / totalCapacity) * 100, 95);

      const line = document.createElement('div');
      line.className = 'file-read-line';
      line.innerHTML = '<span class="filename">Reading ' + fr.file + '...</span> <span class="tokens">' + fr.tokens.toLocaleString() + ' tokens</span>';
      reads.appendChild(line);
      requestAnimationFrame(() => line.classList.add('visible'));

      bar.style.width = pct + '%';
      if (pct > 70) bar.className = 'context-bar-fill danger';
      else if (pct > 40) bar.className = 'context-bar-fill warning';
      counter.textContent = currentTokens.toLocaleString() + ' tokens';

      lineIndex++;
      setTimeout(addFileRead, 600);
    }

    // Phase 2: With Julie
    function startPhase2() {
      reads.innerHTML = '';
      bar.style.width = '0%';
      bar.className = 'context-bar-fill';
      currentTokens = 0;
      counter.textContent = '0 tokens';
      message.className = 'hero-message';

      const julieSteps = [
        { cmd: 'fast_search("UserService", definitions)', result: 'Found · 300 tokens', tokens: 300 },
        { cmd: 'deep_dive("UserService", overview)', result: 'Callers, callees, types · 200 tokens', tokens: 200 },
      ];

      let stepIdx = 0;

      function addJulieStep() {
        if (stepIdx >= julieSteps.length) {
          setTimeout(() => {
            message.textContent = 'Same understanding. 90% fewer tokens.';
            message.className = 'hero-message visible solution';
            setTimeout(() => ctas.classList.add('visible'), 600);
          }, 500);
          return;
        }

        const s = julieSteps[stepIdx];
        currentTokens += s.tokens;
        const pct = (currentTokens / totalCapacity) * 100;

        const line = document.createElement('div');
        line.className = 'file-read-line';
        line.innerHTML = '<span class="julie-cmd">' + s.cmd + '</span> → <span class="julie-result">' + s.result + '</span>';
        reads.appendChild(line);
        requestAnimationFrame(() => line.classList.add('visible'));

        bar.style.width = pct + '%';
        counter.textContent = currentTokens.toLocaleString() + ' tokens';

        stepIdx++;
        setTimeout(addJulieStep, 1200);
      }

      setTimeout(addJulieStep, 500);
    }

    // Start Phase 1 after a brief delay
    setTimeout(addFileRead, 800);
  }

  runHeroAnimation();
```

- [ ] **Step 3: Verify in browser**

Open the page. Verify:
- "Julie" title and tagline render centered
- Context drain animation plays: files appear one by one, bar fills green→yellow→red
- "Session over" message appears in red
- Phase 2 starts: bar resets, Julie commands appear, bar barely moves
- "Same understanding. 90% fewer tokens." appears in green
- CTA buttons fade in
- Animation plays once on page load, replays on page refresh (expected — no persistence needed)

- [ ] **Step 4: Commit**

```bash
git add docs/site/index.html docs/site/script.js
git commit -m "feat(site): add hero section with context drain animation"
```

---

## Chunk 2: Content Sections (Token Savings through Embeddings)

### Task 4: Token Savings Table

**Files:**
- Modify: `docs/site/index.html` (token-savings section)

- [ ] **Step 1: Add token savings HTML**

Replace the `<!-- Task 4 fills this -->` comment in `#token-savings` with:

```html
    <h2 class="section-title">Token Savings</h2>
    <p class="section-subtitle">Real numbers. Same tasks. Dramatically less context consumed.</p>

    <table class="savings-table">
      <thead>
        <tr>
          <th>Task</th>
          <th>Without Julie</th>
          <th>With Julie</th>
          <th>Savings</th>
        </tr>
      </thead>
      <tbody>
        <tr class="data-row animate-on-scroll" data-delay="0">
          <td>Understand a file's API</td>
          <td class="without"><span class="savings-icon" aria-hidden="true">✗</span>~2,000 tokens</td>
          <td class="with-julie"><span class="savings-icon" aria-hidden="true">✓</span>~200 tokens</td>
          <td class="savings-pct">~90%</td>
        </tr>
        <tr class="data-row animate-on-scroll" data-delay="100">
          <td>Find a function</td>
          <td class="without"><span class="savings-icon" aria-hidden="true">✗</span>~5,000+ tokens</td>
          <td class="with-julie"><span class="savings-icon" aria-hidden="true">✓</span>~300 tokens</td>
          <td class="savings-pct">~94%</td>
        </tr>
        <tr class="data-row animate-on-scroll" data-delay="200">
          <td>Investigate before modifying</td>
          <td class="without"><span class="savings-icon" aria-hidden="true">✗</span>~4,000+ tokens</td>
          <td class="with-julie"><span class="savings-icon" aria-hidden="true">✓</span>~200 tokens</td>
          <td class="savings-pct">~95%</td>
        </tr>
        <tr class="data-row animate-on-scroll" data-delay="300">
          <td>Orient on new area</td>
          <td class="without"><span class="savings-icon" aria-hidden="true">✗</span>~10,000+ tokens</td>
          <td class="with-julie"><span class="savings-icon" aria-hidden="true">✓</span>~2,000 tokens</td>
          <td class="savings-pct">~80%</td>
        </tr>
      </tbody>
    </table>
```

- [ ] **Step 2: Verify in browser**

Scroll to the Token Savings section. Verify rows slide in with staggered timing. ✗/✓ icons visible alongside the numbers. Red/green color coding present.

- [ ] **Step 3: Commit**

```bash
git add docs/site/index.html
git commit -m "feat(site): add token savings comparison table"
```

---

### Task 5: How It Works + Reference Graph

**Files:**
- Modify: `docs/site/index.html` (how-it-works and reference-graph sections)
- Modify: `docs/site/script.js` (graph entrance animation)

- [ ] **Step 1: Add How It Works HTML**

Replace the `<!-- Task 5 fills this -->` comment in `#how-it-works` with:

```html
    <h2 class="section-title">How It Works</h2>
    <p class="section-subtitle">Three steps. Zero configuration.</p>

    <div class="steps-flow">
      <div class="step animate-on-scroll" data-delay="0">
        <div class="step-icon">🌳</div>
        <div class="step-number">Step 1</div>
        <h3>Parse</h3>
        <p>Tree-sitter extracts symbols, relationships, and types from 31 languages</p>
      </div>

      <svg class="step-arrow animate-on-scroll" data-delay="150" viewBox="0 0 60 24" fill="none" aria-hidden="true">
        <line x1="0" y1="12" x2="48" y2="12" stroke-width="2" stroke="rgba(255,255,255,0.15)"/>
        <polygon points="48,6 60,12 48,18" fill="rgba(255,255,255,0.15)"/>
      </svg>

      <div class="step animate-on-scroll" data-delay="300">
        <div class="step-icon">📦</div>
        <div class="step-number">Step 2</div>
        <h3>Index</h3>
        <p>Tantivy full-text search + SQLite graph + KNN embeddings</p>
      </div>

      <svg class="step-arrow animate-on-scroll" data-delay="450" viewBox="0 0 60 24" fill="none" aria-hidden="true">
        <line x1="0" y1="12" x2="48" y2="12" stroke-width="2" stroke="rgba(255,255,255,0.15)"/>
        <polygon points="48,6 60,12 48,18" fill="rgba(255,255,255,0.15)"/>
      </svg>

      <div class="step animate-on-scroll" data-delay="600">
        <div class="step-icon">⚡</div>
        <div class="step-number">Step 3</div>
        <h3>Query</h3>
        <p>Agent asks a question, gets exactly what it needs — no file reads</p>
      </div>
    </div>
```

- [ ] **Step 2: Add Reference Graph HTML with inline SVG**

Replace the `<!-- Task 5 fills this -->` comment in `#reference-graph` with:

```html
    <h2 class="section-title">Reference Graph</h2>
    <p class="section-subtitle">Julie doesn't just find symbols — it understands how they connect.</p>

    <div class="graph-container" aria-describedby="graph-desc">
      <svg id="ref-graph" viewBox="0 0 700 360" fill="none" xmlns="http://www.w3.org/2000/svg">
        <!-- Edges (drawn behind nodes) -->
        <!-- Caller edges (blue) -->
        <path class="graph-edge callers graph-edge-animated" d="M140,60 Q250,60 280,155"/>
        <path class="graph-edge callers graph-edge-animated" d="M140,140 Q220,140 280,165"/>
        <path class="graph-edge callers graph-edge-animated" d="M140,220 Q220,220 280,175"/>
        <!-- Callee edges (green) -->
        <path class="graph-edge callees graph-edge-animated" d="M420,155 Q480,60 560,60"/>
        <path class="graph-edge callees graph-edge-animated" d="M420,165 Q480,140 560,140"/>
        <path class="graph-edge callees graph-edge-animated" d="M420,175 Q480,220 560,220"/>
        <!-- Type edge (purple) -->
        <path class="graph-edge types graph-edge-animated" d="M350,184 L350,300"/>

        <!-- Group labels -->
        <text class="graph-label callers" x="60" y="30">Callers</text>
        <text class="graph-label callees" x="580" y="30" text-anchor="middle">Callees</text>
        <text class="graph-label types" x="350" y="300" text-anchor="middle">Types</text>

        <!-- Caller nodes (left) -->
        <g class="graph-node graph-node-animated">
          <rect x="30" y="42" width="180" height="36"/>
          <text x="120" y="60">handle_checkout</text>
        </g>
        <g class="graph-node graph-node-animated">
          <rect x="30" y="122" width="180" height="36"/>
          <text x="120" y="140">retry_payment</text>
        </g>
        <g class="graph-node graph-node-animated">
          <rect x="30" y="202" width="180" height="36"/>
          <text x="120" y="220">batch_processor</text>
        </g>

        <!-- Central node -->
        <g class="graph-node central graph-node-animated">
          <rect x="260" y="140" width="180" height="44"/>
          <text x="350" y="162">process_payment</text>
        </g>

        <!-- Callee nodes (right) -->
        <g class="graph-node graph-node-animated">
          <rect x="490" y="42" width="180" height="36"/>
          <text x="580" y="60">validate_card</text>
        </g>
        <g class="graph-node graph-node-animated">
          <rect x="490" y="122" width="180" height="36"/>
          <text x="580" y="140">charge_gateway</text>
        </g>
        <g class="graph-node graph-node-animated">
          <rect x="490" y="202" width="180" height="36"/>
          <text x="580" y="220">emit_event</text>
        </g>

        <!-- Type nodes (bottom) -->
        <g class="graph-node graph-node-animated">
          <rect x="220" y="300" width="260" height="36"/>
          <text x="350" y="318">PaymentRequest → PaymentResult</text>
        </g>
      </svg>
      <p id="graph-desc" class="sr-only">Graph showing process_payment at center. Called by handle_checkout, retry_payment, and batch_processor. Calls validate_card, charge_gateway, and emit_event. Transforms PaymentRequest into PaymentResult.</p>
    </div>

    <div class="terminal animate-on-scroll" aria-label="Terminal showing deep_dive output for process_payment with callers, callees, and types in 200 tokens">
      <div class="terminal-header">
        <span class="terminal-dot red"></span>
        <span class="terminal-dot yellow"></span>
        <span class="terminal-dot green"></span>
      </div>
      <div><span class="prompt">$ </span><span class="command">deep_dive("process_payment", overview)</span></div>
      <div><span class="result">→ Callers: handle_checkout, retry_payment, batch_processor</span></div>
      <div><span class="result">→ Callees: validate_card, charge_gateway, emit_event</span></div>
      <div><span class="result">→ Types: PaymentRequest → PaymentResult</span></div>
      <div><span class="comment">→ ~200 tokens. Zero file reads.</span></div>
      <span class="cursor"></span>
    </div>

    <p class="section-subtitle" style="margin-top: 32px; text-align: center; max-width: 100%;"><em>One call. Full picture. Zero file reads.</em></p>
```

- [ ] **Step 3: Add graph entrance animation JS**

Add to `script.js`, inside the IIFE, before `initScrollAnimations()`:

```javascript
  // --- Reference graph entrance animation ---
  function initGraphAnimation() {
    const graph = document.getElementById('ref-graph');
    if (!graph) return;

    if (prefersReducedMotion()) {
      // Edges and nodes stay visible (CSS defaults)
      return;
    }

    // Mark SVG as animation-ready (hides nodes/edges via CSS)
    graph.classList.add('graph-animate-ready');

    // Pre-compute edge lengths and set dasharray/offset from JS
    const edges = graph.querySelectorAll('.graph-edge-animated');
    edges.forEach((edge) => {
      const length = edge.getTotalLength();
      edge.style.strokeDasharray = length;
      edge.style.strokeDashoffset = length;
    });

    const nodes = graph.querySelectorAll('.graph-node-animated');

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.unobserve(entry.target);

        // Nodes appear first, staggered
        nodes.forEach((node, i) => {
          setTimeout(() => node.classList.add('shown'), i * 120);
        });

        // Edges draw after nodes are visible
        const edgeDelay = nodes.length * 120 + 200;
        edges.forEach((edge, i) => {
          setTimeout(() => edge.classList.add('drawn'), edgeDelay + i * 100);
        });
      },
      { threshold: 0.3 }
    );

    observer.observe(graph);
  }

  initGraphAnimation();
```

- [ ] **Step 4: Verify in browser**

Scroll to How It Works: three steps with arrows render horizontally, fade in on scroll. Scroll to Reference Graph: nodes appear one by one, edges draw in. Terminal demo below shows deep_dive output with blinking cursor. Screen reader text is present (inspect with dev tools).

- [ ] **Step 5: Commit**

```bash
git add docs/site/index.html docs/site/script.js
git commit -m "feat(site): add How It Works and Reference Graph sections"
```

---

### Task 6: Code Health Intelligence + Semantic Embeddings

**Files:**
- Modify: `docs/site/index.html` (code-health and embeddings sections)

- [ ] **Step 1: Add Code Health HTML**

Replace the `<!-- Task 6 fills this -->` comment in `#code-health` with:

```html
    <h2 class="section-title">Code Health Intelligence</h2>
    <p class="section-subtitle">Risk scores and test quality — computed at index time, not runtime.</p>

    <div class="split-panels">
      <div class="terminal animate-on-scroll" data-delay="0" aria-label="Terminal showing test intelligence: 3 tests found with quality tiers ranging from thorough to thin">
        <div class="terminal-header">
          <span class="terminal-dot red"></span>
          <span class="terminal-dot yellow"></span>
          <span class="terminal-dot green"></span>
        </div>
        <div style="margin-bottom: 4px;"><span class="comment">// Test Intelligence</span></div>
        <div><span class="result">Tests: 3 found</span></div>
        <div>&nbsp; <span class="command">✓</span> <span class="result">test_process_payment</span> &nbsp;<span class="highlight">(thorough — 8 assertions, error paths)</span></div>
        <div>&nbsp; <span class="command">✓</span> <span class="result">test_payment_validation</span> &nbsp;<span class="highlight">(adequate — 4 assertions)</span></div>
        <div>&nbsp; <span class="warn">○</span> <span class="result">test_payment_retry</span> &nbsp;<span class="warn">(thin — 1 assertion, no error paths)</span></div>
        <span class="cursor"></span>
      </div>

      <div class="terminal animate-on-scroll" data-delay="200" aria-label="Terminal showing change risk medium at 0.66 and security risk high at 0.84 with factor breakdown">
        <div class="terminal-header">
          <span class="terminal-dot red"></span>
          <span class="terminal-dot yellow"></span>
          <span class="terminal-dot green"></span>
        </div>
        <div style="margin-bottom: 4px;"><span class="comment">// Risk Scoring</span></div>
        <div>Change Risk: <span class="risk-medium">MEDIUM</span> <span class="result">(0.66)</span></div>
        <div>&nbsp; <span class="result">→ 8 callers · public · thorough tests</span></div>
        <div style="margin-top: 8px;">Security Risk: <span class="risk-high">HIGH</span> <span class="result">(0.84)</span></div>
        <div>&nbsp; <span class="result">→ calls execute · public · accepts string params</span></div>
        <div>&nbsp; <span class="result">→ sink calls: </span><span class="error">execute</span></div>
        <div>&nbsp; <span class="result">→ untested: </span><span class="error">yes</span></div>
        <span class="cursor"></span>
      </div>
    </div>

    <p class="section-subtitle" style="margin-top: 32px; text-align: center; max-width: 100%;"><em>Every symbol scored. Every risk surfaced. Before you touch the code.</em></p>
```

- [ ] **Step 2: Add Semantic Embeddings HTML**

Replace the `<!-- Task 6 fills this -->` comment in `#embeddings` with:

```html
    <h2 class="section-title">Semantic Embeddings</h2>
    <p class="section-subtitle">Find what you mean, not just what you typed.</p>

    <div class="split-panels">
      <div class="terminal animate-on-scroll" data-delay="0" aria-label="Terminal showing keyword search finding handle_user_login and login_handler by exact text matching">
        <div class="terminal-header">
          <span class="terminal-dot red"></span>
          <span class="terminal-dot yellow"></span>
          <span class="terminal-dot green"></span>
        </div>
        <div style="margin-bottom: 4px;"><span class="comment">// Keyword Search</span></div>
        <div><span class="prompt">$ </span><span class="command">fast_search("handle user login")</span></div>
        <div><span class="result">→ handle_user_login</span> &nbsp;<span class="highlight">(exact match)</span></div>
        <div><span class="result">→ login_handler</span> &nbsp;<span class="comment">(partial match)</span></div>
        <span class="cursor"></span>
      </div>

      <div class="terminal animate-on-scroll" data-delay="200" aria-label="Terminal showing semantic similarity finding verify_credentials, validate_session, and check_permissions when searching for authenticate">
        <div class="terminal-header">
          <span class="terminal-dot red"></span>
          <span class="terminal-dot yellow"></span>
          <span class="terminal-dot green"></span>
        </div>
        <div style="margin-bottom: 4px;"><span class="comment">// Semantic Similarity</span></div>
        <div><span class="prompt">$ </span><span class="command">deep_dive("authenticate") → Related:</span></div>
        <div><span class="result">→ verify_credentials</span> &nbsp;<span class="highlight">(0.92 similarity)</span></div>
        <div><span class="result">→ validate_session</span> &nbsp;<span class="highlight">(0.87 similarity)</span></div>
        <div><span class="result">→ check_permissions</span> &nbsp;<span class="highlight">(0.81 similarity)</span></div>
        <span class="cursor"></span>
      </div>
    </div>

    <p class="section-subtitle" style="margin-top: 32px; text-align: center; max-width: 100%;"><em>GPU-accelerated embeddings. Works with CUDA, MPS, and DirectML. Falls back to CPU gracefully.</em></p>
```

- [ ] **Step 3: Verify in browser**

Scroll to Code Health: two panels render side by side. Risk labels are color-coded (MEDIUM=yellow, HIGH=red). Scroll to Embeddings: two panels render side by side. Both sections have blinking cursors and fade-in animation.

- [ ] **Step 4: Commit**

```bash
git add docs/site/index.html
git commit -m "feat(site): add Code Health and Semantic Embeddings sections"
```

---

## Chunk 3: Tools, Languages, Install, and Polish

### Task 7: Tools Showcase

**Files:**
- Modify: `docs/site/index.html` (tools section)

- [ ] **Step 1: Add Tools Showcase HTML**

Replace the `<!-- Task 7 fills this -->` comment in `#tools` with all 7 tool cards. Each card has: tool name, description, mini terminal demo, token badge. Here is the complete markup:

```html
    <h2 class="section-title">Tools</h2>
    <p class="section-subtitle">7 tools. Each one replaces dozens of file reads.</p>

    <div class="card-grid">
      <div class="card tool-card animate-on-scroll" data-delay="0">
        <div class="tool-name">fast_search</div>
        <div class="tool-desc">Full-text code search with code-aware tokenization</div>
        <div class="terminal" aria-label="fast_search finding UserService definition in 3 milliseconds">
          <div><span class="prompt">$ </span><span class="command">fast_search("UserService", definitions)</span></div>
          <div><span class="highlight">Definition found:</span> <span class="result">UserService</span></div>
          <div><span class="result">&nbsp; src/services.rs:42 (struct, public)</span></div>
          <div><span class="comment">&nbsp; 3ms</span></div>
        </div>
        <span class="token-badge">~300 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="100">
        <div class="tool-name">get_context</div>
        <div class="tool-desc">Token-budgeted context for a concept or task</div>
        <div class="terminal" aria-label="get_context returning pivots and neighbors for payment processing">
          <div><span class="prompt">$ </span><span class="command">get_context("payment processing")</span></div>
          <div><span class="highlight">Pivots (3):</span> <span class="result">process_payment, PaymentService, charge_gateway</span></div>
          <div><span class="highlight">Neighbors (8):</span> <span class="result">signatures + types</span></div>
          <div><span class="comment">&nbsp; Token budget: 2000 · Used: 1847</span></div>
        </div>
        <span class="token-badge">~2,000 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="200">
        <div class="tool-name">deep_dive</div>
        <div class="tool-desc">Progressive-depth symbol investigation</div>
        <div class="terminal" aria-label="deep_dive showing callers, callees, and types for process_payment">
          <div><span class="prompt">$ </span><span class="command">deep_dive("process_payment", overview)</span></div>
          <div><span class="result">Callers (3) · Callees (3) · Types (2)</span></div>
          <div><span class="result">Change Risk: </span><span class="risk-medium">MEDIUM</span></div>
          <div><span class="comment">&nbsp; ~200 tokens</span></div>
        </div>
        <span class="token-badge">~200 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="300">
        <div class="tool-name">fast_refs</div>
        <div class="tool-desc">Find all references to a symbol</div>
        <div class="terminal" aria-label="fast_refs finding 12 references to UserService across 8 files">
          <div><span class="prompt">$ </span><span class="command">fast_refs("UserService")</span></div>
          <div><span class="highlight">12 references</span> <span class="result">across 8 files</span></div>
          <div><span class="result">&nbsp; api.rs:15, handler.rs:42, tests.rs:8, ...</span></div>
        </div>
        <span class="token-badge">~400 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="400">
        <div class="tool-name">get_symbols</div>
        <div class="tool-desc">Smart file reading with 70-90% token savings</div>
        <div class="terminal" aria-label="get_symbols showing 20-line structure overview of a 500-line file">
          <div><span class="prompt">$ </span><span class="command">get_symbols("src/services.rs", structure)</span></div>
          <div><span class="result">struct UserService (pub)</span></div>
          <div><span class="result">&nbsp; fn new() → Self</span></div>
          <div><span class="result">&nbsp; fn authenticate(&self, ...) → Result</span></div>
          <div><span class="comment">&nbsp; 500 lines → 20 line overview</span></div>
        </div>
        <span class="token-badge">~200 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="500">
        <div class="tool-name">rename_symbol</div>
        <div class="tool-desc">Workspace-wide rename with dry-run preview</div>
        <div class="terminal" aria-label="rename_symbol previewing rename of UserService to AccountService across 15 files">
          <div><span class="prompt">$ </span><span class="command">rename_symbol("UserService", "AccountService", dry_run)</span></div>
          <div><span class="highlight">Preview:</span> <span class="result">15 files, 23 replacements</span></div>
          <div><span class="result">&nbsp; services.rs:42 &nbsp;UserService → AccountService</span></div>
          <div><span class="result">&nbsp; handler.rs:15 &nbsp;&nbsp;UserService → AccountService</span></div>
        </div>
        <span class="token-badge">~500 tokens</span>
      </div>

      <div class="card tool-card animate-on-scroll" data-delay="600">
        <div class="tool-name">manage_workspace</div>
        <div class="tool-desc">Index, add, remove, refresh workspaces</div>
        <div class="terminal" aria-label="manage_workspace indexing a project in under 2 seconds">
          <div><span class="prompt">$ </span><span class="command">manage_workspace(index)</span></div>
          <div><span class="result">Indexed: 1,247 symbols from 89 files</span></div>
          <div><span class="result">Languages: Rust, TypeScript, Python</span></div>
          <div><span class="comment">&nbsp; 1.8s</span></div>
        </div>
        <span class="token-badge">~100 tokens</span>
      </div>
    </div>
```

- [ ] **Step 2: Verify in browser**

Scroll to Tools: 7 cards in 2-column grid, each with terminal demo. Cards fade in staggered. On mobile, grid collapses to single column.

- [ ] **Step 3: Commit**

```bash
git add docs/site/index.html
git commit -m "feat(site): add Tools Showcase section with 7 tool cards"
```

---

### Task 8: Languages, Performance, Installation, Footer

**Files:**
- Modify: `docs/site/index.html` (languages, performance, install, footer sections)
- Modify: `docs/site/script.js` (performance count-up, tab switching, copy-to-clipboard)

- [ ] **Step 1: Add Languages HTML**

Replace the `<!-- Task 8 fills this -->` comment in `#languages` with:

```html
    <h2 class="section-title">31 Languages</h2>
    <p class="section-subtitle">Full symbol extraction, reference graphs, and code intelligence across all of them.</p>

    <div class="lang-category animate-on-scroll" data-delay="0">
      <div class="lang-category-label">Core (10)</div>
      <div class="lang-grid">
        <span class="lang-badge"><span class="lang-dot" style="background:#dea584"></span>Rust</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#3178c6"></span>TypeScript</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#f1e05a"></span>JavaScript</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#3572a5"></span>Python</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#b07219"></span>Java</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#178600"></span>C#</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#4f5d95"></span>PHP</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#701516"></span>Ruby</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#f05138"></span>Swift</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#a97bff"></span>Kotlin</span>
      </div>
    </div>

    <div class="lang-category animate-on-scroll" data-delay="100">
      <div class="lang-category-label">Systems (5)</div>
      <div class="lang-grid">
        <span class="lang-badge"><span class="lang-dot" style="background:#555555"></span>C</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#f34b7d"></span>C++</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#00add8"></span>Go</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#000080"></span>Lua</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#ec915c"></span>Zig</span>
      </div>
    </div>

    <div class="lang-category animate-on-scroll" data-delay="200">
      <div class="lang-category-label">Specialized (12)</div>
      <div class="lang-grid">
        <span class="lang-badge"><span class="lang-dot" style="background:#355570"></span>GDScript</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#41b883"></span>Vue</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#44a51c"></span>QML</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#276dc3"></span>R</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#512be4"></span>Razor</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#e38c00"></span>SQL</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#e34c26"></span>HTML</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#563d7c"></span>CSS</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#009926"></span>Regex</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#89e051"></span>Bash</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#012456"></span>PowerShell</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#00b4ab"></span>Dart</span>
      </div>
    </div>

    <div class="lang-category animate-on-scroll" data-delay="300">
      <div class="lang-category-label">Documentation (4)</div>
      <div class="lang-grid">
        <span class="lang-badge"><span class="lang-dot" style="background:#083fa1"></span>Markdown</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#a0a0a0"></span>JSON</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#9c4221"></span>TOML</span>
        <span class="lang-badge"><span class="lang-dot" style="background:#cb171e"></span>YAML</span>
      </div>
    </div>
```

- [ ] **Step 2: Add Performance Stats HTML**

Replace the `<!-- Task 8 fills this -->` comment in `#performance` with:

```html
    <div class="perf-stats">
      <div class="perf-stat animate-on-scroll" data-delay="0">
        <div class="perf-number" data-target="5" data-prefix="<" data-suffix="ms" id="perf-search">&lt;5ms</div>
        <div class="perf-label">Search</div>
      </div>
      <div class="perf-stat animate-on-scroll" data-delay="150">
        <div class="perf-number" data-target="100" data-prefix="<" data-suffix="MB" id="perf-memory">&lt;100MB</div>
        <div class="perf-label">Memory</div>
      </div>
      <div class="perf-stat animate-on-scroll" data-delay="300">
        <div class="perf-number" data-target="2" data-prefix="<" data-suffix="s" id="perf-startup">&lt;2s</div>
        <div class="perf-label">Startup</div>
      </div>
    </div>
    <p class="perf-subtitle animate-on-scroll" data-delay="400">Incremental updates: only changed files re-indexed. Typically 3-15 seconds.</p>
```

- [ ] **Step 3: Add Installation HTML**

Replace the `<!-- Task 8 fills this -->` comment in `#install` with:

```html
    <h2 class="section-title">Installation</h2>
    <p class="section-subtitle">Get started in 30 seconds.</p>

    <div class="animate-on-scroll">
      <div class="install-tabs" role="tablist">
        <button class="install-tab active" role="tab" aria-selected="true" data-tab="claude-code">Claude Code</button>
        <button class="install-tab" role="tab" aria-selected="false" data-tab="vscode">VS Code</button>
        <button class="install-tab" role="tab" aria-selected="false" data-tab="cursor">Cursor / Other</button>
      </div>

      <div id="panel-claude-code" class="install-panel active" role="tabpanel">
        <div class="terminal install-terminal" style="position:relative;" aria-label="Installation commands for Claude Code">
          <button class="copy-btn" data-copy="claude-code" aria-label="Copy to clipboard">Copy</button>
          <div><span class="prompt"># </span><span class="command">Build from source</span></div>
          <div><span class="prompt">$ </span><span class="result">git clone https://github.com/anortham/julie.git</span></div>
          <div><span class="prompt">$ </span><span class="result">cd julie && cargo build --release</span></div>
          <div style="margin-top: 8px;"><span class="prompt"># </span><span class="command">Connect to Claude Code</span></div>
          <div><span class="prompt">$ </span><span class="result">claude mcp add julie -- ./target/release/julie-server</span></div>
        </div>
      </div>

      <div id="panel-vscode" class="install-panel" role="tabpanel">
        <div class="terminal install-terminal" style="position:relative;" aria-label="Configuration for VS Code with GitHub Copilot">
          <button class="copy-btn" data-copy="vscode" aria-label="Copy to clipboard">Copy</button>
          <div><span class="comment">// .vscode/mcp.json</span></div>
          <div><span class="result">{</span></div>
          <div><span class="result">&nbsp; "servers": {</span></div>
          <div><span class="result">&nbsp; &nbsp; "Julie": {</span></div>
          <div><span class="result">&nbsp; &nbsp; &nbsp; "type": </span><span class="highlight">"stdio"</span><span class="result">,</span></div>
          <div><span class="result">&nbsp; &nbsp; &nbsp; "command": </span><span class="highlight">"/path/to/julie-server"</span></div>
          <div><span class="result">&nbsp; &nbsp; }</span></div>
          <div><span class="result">&nbsp; }</span></div>
          <div><span class="result">}</span></div>
        </div>
      </div>

      <div id="panel-cursor" class="install-panel" role="tabpanel">
        <div class="terminal install-terminal" style="position:relative;" aria-label="Configuration for Cursor, Windsurf, or other MCP clients">
          <button class="copy-btn" data-copy="cursor" aria-label="Copy to clipboard">Copy</button>
          <div><span class="comment">// MCP config (Cursor, Windsurf, etc.)</span></div>
          <div><span class="result">{</span></div>
          <div><span class="result">&nbsp; "mcpServers": {</span></div>
          <div><span class="result">&nbsp; &nbsp; "julie": {</span></div>
          <div><span class="result">&nbsp; &nbsp; &nbsp; "command": </span><span class="highlight">"/path/to/julie-server"</span></div>
          <div><span class="result">&nbsp; &nbsp; }</span></div>
          <div><span class="result">&nbsp; }</span></div>
          <div><span class="result">}</span></div>
        </div>
      </div>
    </div>

    <p class="section-subtitle" style="margin-top: 24px;">Julie indexes your workspace automatically on first connection (~2-5s for most projects).</p>
```

- [ ] **Step 4: Add Footer HTML**

Replace the `<!-- Task 8 fills this -->` comment in `#footer` with:

```html
    <div class="footer-inner">
      <a href="https://github.com/anortham/julie" target="_blank" rel="noopener">GitHub</a>
      <span class="footer-sep">·</span>
      <span>MIT License</span>
      <span class="footer-sep">·</span>
      <span>Built in Rust with tree-sitter and Tantivy</span>
      <span class="footer-sep">·</span>
      <span>v5.2.5</span>
    </div>
```

- [ ] **Step 5: Add Performance count-up, tab switching, and copy-to-clipboard JS**

Add to `script.js`, inside the IIFE, before `initScrollAnimations()`:

```javascript
  // --- Performance number count-up ---
  function initPerfCountUp() {
    const perfNumbers = document.querySelectorAll('.perf-number[data-target]');
    if (perfNumbers.length === 0) return;

    if (prefersReducedMotion()) return; // Numbers already show final values in HTML

    // Zero out for animation start (reduced-motion users keep the HTML values above)
    perfNumbers.forEach((el) => {
      const prefix = el.dataset.prefix || '';
      const suffix = el.dataset.suffix || '';
      el.textContent = prefix + '0' + suffix;
    });

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          observer.unobserve(entry.target);

          const el = entry.target;
          const target = parseInt(el.dataset.target, 10);
          const prefix = el.dataset.prefix || '';
          const suffix = el.dataset.suffix || '';
          const duration = 1000;
          const start = performance.now();

          function update(now) {
            const elapsed = now - start;
            const progress = Math.min(elapsed / duration, 1);
            // Ease out
            const eased = 1 - Math.pow(1 - progress, 3);
            const current = Math.round(eased * target);
            el.textContent = prefix + current + suffix;
            if (progress < 1) requestAnimationFrame(update);
          }

          requestAnimationFrame(update);
        });
      },
      { threshold: 0.5 }
    );

    perfNumbers.forEach((el) => observer.observe(el));
  }

  initPerfCountUp();

  // --- Installation tab switching ---
  const tabs = document.querySelectorAll('.install-tab');
  const panels = document.querySelectorAll('.install-panel');
  const tabList = [...tabs];

  function activateTab(tab) {
    const target = tab.dataset.tab;
    tabs.forEach((t) => {
      const isActive = t === tab;
      t.classList.toggle('active', isActive);
      t.setAttribute('aria-selected', isActive ? 'true' : 'false');
      t.setAttribute('tabindex', isActive ? '0' : '-1');
    });
    panels.forEach((p) => {
      p.classList.toggle('active', p.id === 'panel-' + target);
    });
  }

  tabs.forEach((tab) => {
    tab.addEventListener('click', () => activateTab(tab));

    // Keyboard navigation: Arrow Left/Right, Home/End
    tab.addEventListener('keydown', (e) => {
      const idx = tabList.indexOf(tab);
      let next = null;
      if (e.key === 'ArrowRight') next = tabList[(idx + 1) % tabList.length];
      else if (e.key === 'ArrowLeft') next = tabList[(idx - 1 + tabList.length) % tabList.length];
      else if (e.key === 'Home') next = tabList[0];
      else if (e.key === 'End') next = tabList[tabList.length - 1];
      if (next) {
        e.preventDefault();
        next.focus();
        activateTab(next);
      }
    });
  });

  // Set initial tabindex
  tabs.forEach((t) => t.setAttribute('tabindex', t.classList.contains('active') ? '0' : '-1'));

  // --- Copy to clipboard ---
  const copySnippets = {
    'claude-code': 'git clone https://github.com/anortham/julie.git\ncd julie && cargo build --release\nclaude mcp add julie -- ./target/release/julie-server',
    'vscode': '{\n  "servers": {\n    "Julie": {\n      "type": "stdio",\n      "command": "/path/to/julie-server"\n    }\n  }\n}',
    'cursor': '{\n  "mcpServers": {\n    "julie": {\n      "command": "/path/to/julie-server"\n    }\n  }\n}',
  };

  document.querySelectorAll('.copy-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const key = btn.dataset.copy;
      const text = copySnippets[key];
      if (!text) return;

      navigator.clipboard.writeText(text).then(() => {
        btn.textContent = 'Copied!';
        btn.classList.add('copied');
        setTimeout(() => {
          btn.textContent = 'Copy';
          btn.classList.remove('copied');
        }, 2000);
      }).catch(() => {
        // Fallback for non-secure contexts (file:// protocol, etc.)
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
        btn.textContent = 'Copied!';
        btn.classList.add('copied');
        setTimeout(() => {
          btn.textContent = 'Copy';
          btn.classList.remove('copied');
        }, 2000);
      });
    });
  });
```

- [ ] **Step 6: Verify in browser**

- Languages: 4 category groups, 31 badges with colored dots, fade in on scroll
- Performance: three numbers count up from 0 (0→5ms, 0→100MB, 0→2s)
- Installation: three tabs switch between Claude Code / VS Code / Cursor snippets. Copy button works.
- Footer: single row with GitHub, license, built-with, version

- [ ] **Step 7: Commit**

```bash
git add docs/site/index.html docs/site/script.js
git commit -m "feat(site): add Languages, Performance, Installation, and Footer sections"
```

---

### Task 9: 404 Page + Final Polish

**Files:**
- Create: `docs/site/404.html`
- Modify: `docs/site/style.css` (minor tweaks if needed)

- [ ] **Step 1: Create 404.html**

Create `docs/site/404.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>404 — Julie</title>
  <meta name="robots" content="noindex">
  <link rel="icon" type="image/svg+xml" href="favicon.svg">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <div style="min-height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; padding: 24px;">
    <h1 style="font-family: var(--font-mono); font-size: 4rem; color: var(--accent-green); margin-bottom: 16px;">404</h1>
    <p style="font-size: 1.2rem; color: var(--text-secondary); margin-bottom: 32px;">Page not found.</p>
    <a href="./" class="btn btn-primary">Back to Julie</a>
  </div>
</body>
</html>
```

- [ ] **Step 2: Browser testing checklist**

Open `docs/site/index.html` and verify end-to-end:

- [ ] Hero animation plays through both phases
- [ ] Sticky nav appears on scroll, active link tracks current section
- [ ] All sections render with correct content
- [ ] Scroll animations trigger once per section
- [ ] Terminal demos have blinking cursors
- [ ] Reference graph SVG renders with correct colors
- [ ] Performance numbers count up
- [ ] Installation tabs switch correctly
- [ ] Copy buttons work
- [ ] Footer renders
- [ ] Open DevTools → toggle "Prefers reduced motion" → reload → all animations skip, content shows immediately
- [ ] Resize to mobile width → layout stacks, nav scrolls horizontally, terminal blocks scroll
- [ ] Open 404.html → renders dark page with "Back to Julie" link
- [ ] Verify in both Chrome and Firefox (at minimum)
- [ ] Check total page weight in DevTools Network tab (target: <150KB)
- [ ] Verify all external links work (GitHub repo in footer, "View on GitHub" CTA in hero)
- [ ] Test tab switching with keyboard (Arrow Left/Right between install tabs)
- [ ] Verify JetBrains Mono font loads (terminal text should NOT be Courier)

- [ ] **Step 3: Commit**

```bash
git add docs/site/404.html
git commit -m "feat(site): add 404 page and finalize site"
```

---

## Summary

| Task | What it builds | Key files |
|------|---------------|-----------|
| 1 | Scaffolding: HTML skeleton, CSS foundation, nav JS, favicon, CI workflow | `index.html`, `style.css`, `script.js`, `favicon.svg`, `pages.yml` |
| 2 | All section-specific CSS (hero, table, graph, cards, badges, perf, install, footer) | `style.css` |
| 3 | Hero section: context drain animation + Julie terminal demo + CTAs | `index.html`, `script.js` |
| 4 | Token savings comparison table with scroll animation | `index.html` |
| 5 | How It Works 3-step flow + Reference Graph SVG with entrance animation | `index.html`, `script.js` |
| 6 | Code Health Intelligence + Semantic Embeddings panels | `index.html` |
| 7 | Tools Showcase: 7 tool cards with mini terminal demos | `index.html` |
| 8 | Languages grid, Performance count-up, Installation tabs + copy, Footer | `index.html`, `script.js` |
| 9 | 404 page + final browser testing | `404.html` |

**Total new files:** 5 (`index.html`, `style.css`, `script.js`, `favicon.svg`, `404.html`) + 1 workflow (`pages.yml`)
**Total commits:** 9
