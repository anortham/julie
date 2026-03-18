# Launch Readiness: Language Fixes, Documentation, and Skill Validation

**Date:** 2026-03-18
**Status:** Approved
**Goal:** Prepare Julie for its first public release — fix credibility-threatening language bugs, document skills and installation for all major AI coding harnesses, and validate skill quality.

---

## Context

Julie v5.3.0 supports 33 languages, 8 MCP tools, 10 skills, and has been verified against real-world projects for 17/19 full-tier languages. The project is functionally complete for a public launch, but three gaps remain:

1. **Language bugs** — 4 extractor/resolver bugs that produce incorrect results for common projects (Python/Flask centrality theft is the worst)
2. **Documentation** — README is stale (says 31 languages, 7 tools, no skills section), GH Pages site has no skills section, no installation guides for Gemini/Codex/other harnesses, no skill installation instructions
3. **Skill quality** — 10 skills shipped but never validated through the skill-creator reviewer

## Approach: Fixes-First

**Order:** Language fixes → Documentation → Skill validation

Rationale: Language fixes change the verification results matrix. Documentation should reference the final state, not an intermediate one. The Python centrality theft bug is embarrassing if someone tries Julie on Flask/Django — fix before driving traffic.

---

## Workstream 1: Language Fixes

### 1A. Python Centrality Theft (Systemic — HIGH priority)

**Bug:** When a test file defines `class Flask(flask.Flask)` (a test subclass), pending relationship resolution picks the test subclass over the real class. The test subclass accumulates more raw incoming references because tests import from it. This cascades into wrong centrality (real Flask gets ~0) and wrong definition search ranking (#1 is the test class).

**Fix:** During pending relationship resolution, add a test-file de-preference. When multiple candidates match a pending relationship target name, penalize candidates whose definition lives in a test file (detected via `is_test_path()`). This preserves correct behavior when the only definition IS in a test file, but prevents test subclasses from stealing centrality from production code.

**Scope:** The resolver's `score_candidate` function in `src/tools/workspace/indexing/resolver.rs` already has a scoring structure with additive bonuses (language match +100, path proximity +50/+25, kind match +10, parent reference +200). Adding a test-path penalty here is the first approach to try. However, the verification data shows the test Flask gets raw centrality score 213 vs real Flask's 1.4 — these are centrality scores from `compute_reference_scores()`, not resolver scores. The problem may be twofold: (1) the resolver picks the test class as the target for pending relationships, and (2) centrality then accumulates on the wrong target. A simple penalty in `score_candidate` may not override a +200 parent-reference bonus. TDD will reveal which layer needs the fix — this may require changes in both the resolver and centrality computation.

**Verification:** Re-run Python verification against pallets/flask. Flask class should have centrality >0.5 with 125+ dependents. Test subclass should have lower centrality.

### 1B. Swift `Session` Class Extraction

**Bug:** `open class Session: @unchecked Sendable` at `Source/Core/Session.swift:30` is missing from the symbol table. The extractor's `extract_class()` in `swift/types.rs` looks for `type_identifier`/`user_type` child but returns `None` for this syntax. ~80 methods become orphaned top-level functions.

**Fix:** Investigate tree-sitter AST for this declaration pattern. The `@unchecked` attribute wrapper likely changes the node structure. Fix the child-type lookup.

**Scope:** `crates/julie-extractors/src/swift/types.rs`

**Verification:** Re-run Swift verification against Alamofire. Session class should appear with ~80 child methods.

### 1C. Kotlin Sealed Class Extraction

**Bug:** `sealed class JsonReader` is absent from symbols while `JsonWriter` (also sealed) works fine. Member functions become orphaned top-level symbols.

**Fix:** Investigate the AST difference between working `JsonWriter` and broken `JsonReader`. Likely a tree-sitter grammar edge case in how the sealed modifier interacts with the class body.

**Scope:** `crates/julie-extractors/src/kotlin/`

**Verification:** Re-run Kotlin verification against square/moshi. JsonReader should appear as a class with correct child methods.

### 1D. Dart 3 Class Modifiers

**Bug:** `base class`, `sealed class`, `final class`, `interface class` (Dart 3 modifiers) produce different AST node types than plain `class_definition`. The extractor only matches `class_definition`, so ProviderContainer (`base class`) and AsyncValue (`sealed class`) are missing.

**Fix:** Add handlers for Dart 3 modifier node types in the extractor.

**Scope:** `crates/julie-extractors/src/dart/`

**Verification:** Re-run Dart verification against rrousselGit/riverpod. ProviderContainer and AsyncValue should appear as class symbols.

### 1E. Language Verification Runs

Run verification against real-world projects following `docs/LANGUAGE_VERIFICATION_CHECKLIST.md`:

**Full Tier (2 remaining):**
- **Rust** — Julie itself or ripgrep
- **JavaScript** — a focused JS project (not a TS project with JS)

**Specialized Tier (7 remaining):**
- **Bash** — ohmyzsh/ohmyzsh
- **PowerShell** — PowerShell/PSScriptAnalyzer
- **Vue** — vuejs/pinia
- **R** — tidyverse/ggplot2
- **SQL** — flyway/flyway migration files
- **HTML** — any web project's templates
- **CSS** — any web project's stylesheets

Each verification may surface new bugs. Those get fixed via TDD inline during the verification session. Results are recorded in `docs/LANGUAGE_VERIFICATION_RESULTS.md`.

**Data/Docs Tier (5 remaining):**
- **Markdown** — Julie's own `docs/` directory
- **JSON** — Julie's own config files
- **TOML** — Julie's `Cargo.toml` files
- **YAML** — GitHub Actions CI config
- **Regex** — verify extractor doesn't crash

These are trivial (Check 1 only: symbol extraction) and take ~15 minutes total. Fills in the last blank rows in the matrix.

**Parallelization:** The 4 bug fixes (1A-1D) are independent and can be dispatched to parallel agents. Verification runs split into two groups:
- **1E-reverify:** Python re-verification — depends on 1A completion
- **1E-new:** All other verifications (Rust, JS, Specialized, Data/Docs) — independent of 1A-1D, can start immediately

**Verification threshold for Specialized/Data tiers:** A language is considered "verified" if all applicable checks PASS or have documented, language-inherent limitations (e.g., Lua's lack of class keyword). A check that FAILs due to a Julie bug blocks the language — either fix the bug or document it as a known limitation with a tracking note.

---

## Workstream 2: Documentation

### 2A. README Updates

**Quick fixes (standalone commit):**
- "31 languages" → "33 languages" (all occurrences)
- "Tools (7)" → "Tools (8)" — add `query_metrics` tool entry with description
- "Supported Languages (31)" → "Supported Languages (33)" — add Scala to Core, Elixir to a new Functional category
- Project structure `extractors/` comment — update to 33

**New: Skills section** (after Code Health Intelligence):

Table of all 10 skills grouped by category:
| Category | Skills |
|----------|--------|
| Reports | `/codehealth`, `/security-audit`, `/architecture` |
| Navigation | `/explore-area`, `/call-trace`, `/logic-flow` |
| Analysis | `/impact-analysis`, `/dependency-graph`, `/type-flow` |
| Debugging | `/search-debug` |

One-line description per skill. Note that skills ship in `.claude/skills/` with a pointer to the installation guide.

**New: Skill installation guide** (subsection of Skills or Installation):

Per-harness instructions for copying skills to the correct location. Web research required for current paths:
- **Claude Code** — already in `.claude/skills/`, works out of the box when Julie repo is cloned
- **VS Code / GitHub Copilot** — copy to `.github/copilot-instructions.md` or equivalent
- **Cursor** — copy to `.cursor/rules/`
- **Windsurf** — copy to `.windsurfrules` or equivalent
- **Gemini CLI** — copy to `GEMINI.md` or `.gemini/` config
- **Codex CLI** — copy to `AGENTS.md` or equivalent
- **OpenCode** — research needed

### 2B. GitHub Pages Site Updates

- **New Skills section** — between Code Health and Tools. Showcase the 3 report skills with terminal mockups (most visually impressive). Mention the 7 navigation/analysis skills as a list.
- **Tool count and cards** — update "7 tools" to "8 tools" (line 327) and add a `query_metrics` tool card to the card grid
- **Install tabs** — add Gemini CLI and Codex CLI tabs with correct config format
- **Footer** — update version number
- **JSONL resolution** — JSONL is a file extension alias for JSON (no separate extractor directory). Remove JSONL from README's Documentation line. Keep Documentation at 4 (Markdown, JSON, TOML, YAML). Total remains 33 languages.

### 2C. Harness Research

Web research to compile correct, current skill installation paths for each harness. This is critical — wrong paths mean broken installs and frustrated users.

---

## Workstream 3: Skill Validation

Run all 10 skills through `skill-creator:skill-creator` to audit:
- **Description quality** — does the description trigger correctly?
- **Allowed-tools declarations** — are all needed tools listed?
- **Query pattern correctness** — do the documented query patterns match tool APIs?
- **Output format clarity** — is the report format well-structured?

Fix anything flagged. Light pass, not a rewrite.

---

## Known Limitations (Accepted for Launch)

These are unfixed issues from verification that are either language-inherent or low-severity. They will be documented in the verification results matrix and optionally in a "Known Limitations" section of the README.

| Language | Issue | Why Deferred |
|----------|-------|-------------|
| **C++** | Zero cross-file references; deep_dive can't disambiguate constructor overloads | Header-only architecture (nlohmann/json) is an unusual pattern; most C++ projects have separate .cpp files |
| **C** | Centrality split between header and implementation | Inherent to C's header/implementation model; header gets refs, implementation gets none |
| **PHP** | Class-level relationship tracking weak; `reference_kind` filter ignored; `get_symbols` target filter duplicates methods | Namespace-heavy architecture limits direct class refs; the duplication bug is cosmetic |
| **Ruby** | Centrality accumulates on `constant` instead of `class` symbol | Ruby's dual symbol emission (class + constant at same line); needs deeper investigation |
| **Lua** | Class-like tables stored as `variable` kind, excluded from centrality boost | Inherent to Lua — no `class` keyword; metatables are semantically classes but syntactically variables |
| **Go** | Markdown headings outrank Go struct in def search without `language` filter | Works correctly with `language="go"` filter; cross-language ranking is a general issue |

---

## Ordering and Dependencies

```
Phase 1: Language Fixes (1-2 sessions)
├── 1A: Python centrality theft       ← highest priority, systemic
├── 1B: Swift Session extraction      ← can parallel with 1A
├── 1C: Kotlin sealed classes         ← can parallel with 1A
├── 1D: Dart 3 class modifiers        ← can parallel with 1A
├── 1E-reverify: Python re-verify     ← depends on 1A
└── 1E-new: All other verifications   ← independent, can start immediately
    ├── Rust + JS (Full Tier)
    ├── 7 Specialized Tier languages
    └── 5 Data/Docs Tier languages

Phase 2: Documentation (1 session)      ← depends on Phase 1
├── 2A: README updates
├── 2B: GH Pages site updates          ← depends on 2A
└── 2C: Harness research               ← independent, can start during Phase 1

Phase 3: Skill Validation (0.5 session)  ← independent of Phase 2, can run in parallel
└── 3A: Run all 10 skills through skill-creator reviewer
```

**Quick win:** The README "31→33" and "7→8 tools" fix can be committed immediately as a trivial update, independent of the larger docs work.

---

## Success Criteria

### Language Fixes
- [ ] Python/Flask verification: real Flask class has centrality >0.5
- [ ] Swift/Alamofire: Session class extracted with child methods
- [ ] Kotlin/moshi: JsonReader class extracted
- [ ] Dart/riverpod: ProviderContainer and AsyncValue extracted

### Verification Coverage
- [ ] Rust and JavaScript Full Tier verification PASS
- [ ] 7 Specialized Tier languages verified (PASS or documented language-inherent limitations)
- [ ] 5 Data/Docs Tier languages verified (Check 1 PASS)
- [ ] Verification results matrix fully populated (no blank `—` rows)

### Documentation
- [ ] README accurate: 33 languages, 8 tools, skills section, skill installation guide
- [ ] GH Pages site: skills section, 8 tool cards, Gemini/Codex install tabs, current version
- [ ] JSONL removed from Documentation language list (alias for JSON, not a separate language)
- [ ] Known limitations documented for deferred bugs (C++, C, PHP, Ruby, Lua, Go)

### Skill Quality
- [ ] All 10 skills pass skill-creator review

### Artifacts
- [ ] Verification results matrix updated in `docs/LANGUAGE_VERIFICATION_RESULTS.md`
- [ ] Design spec updated to final status
