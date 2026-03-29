# Dashboard Language Bar + Per-Workspace Metrics Link

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a multicolor stacked language bar to the Projects page (compact in table, detailed in expand panel) and a "View metrics" link from project detail.

**Architecture:** Language data comes from existing `SymbolDatabase::count_files_by_language()` accessed via `WorkspacePool`. Compact bars in table rows are updated via the existing `/projects/statuses` polling. Detail panel gets a full breakdown with legend. Pure HTML/CSS, no charting library.

**Tech Stack:** Rust (Axum routes), Tera templates, CSS custom properties, vanilla JS

**Spec:** `docs/superpowers/specs/2026-03-29-dashboard-language-bar-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `dashboard/static/app.css` | Language color vars + bar styles |
| Modify | `src/dashboard/routes/projects.rs` | Fetch language data, wire into endpoints |
| Create | `dashboard/templates/partials/language_detail.html` | Full bar + legend (detail panel) |
| Modify | `dashboard/templates/partials/project_row.html` | Add compact bar below workspace row |
| Modify | `dashboard/templates/partials/project_detail.html` | Add language detail + metrics link |
| Modify | `dashboard/templates/projects.html` | Update polling JS for language bars |
| Modify | `src/tests/dashboard/integration.rs` | Test detail endpoint returns language data |

---

### Task 1: CSS — Language Color Variables and Bar Styles

**Files:**
- Modify: `dashboard/static/app.css`

- [ ] **Step 1: Add language color custom properties**

Append to the `:root, [data-theme="dark"]` block (after line 27, before the closing `}`):

```css
  /* Language colors (GitHub linguist) */
  --lang-rust:       #dea584;
  --lang-typescript: #3178c6;
  --lang-javascript: #f1e05a;
  --lang-python:     #3572A5;
  --lang-java:       #b07219;
  --lang-csharp:     #178600;
  --lang-go:         #00ADD8;
  --lang-c:          #555555;
  --lang-cpp:        #555555;
  --lang-ruby:       #701516;
  --lang-swift:      #F05138;
  --lang-php:        #4F5D95;
  --lang-kotlin:     #A97BFF;
  --lang-html:       #e34c26;
  --lang-css:        #563d7c;
  --lang-scala:      #c22d40;
  --lang-elixir:     #6e4a7e;
  --lang-lua:        #000080;
  --lang-dart:       #00B4AB;
  --lang-zig:        #ec915c;
  --lang-r:          #198CE7;
  --lang-gdscript:   #355570;
  --lang-vue:        #41b883;
  --lang-other:      #8b8b8b;
```

Also append the same block to `[data-theme="light"]` (same colors work on both themes since they're used as bar fills against neutral tracks).

- [ ] **Step 2: Add language bar CSS rules**

Append after the "Metrics Volume Bars" section (after line 450):

```css
/* ---------- Language Bars ---------- */

.lang-bar-track {
  display: flex;
  height: 4px;
  border-radius: 2px;
  overflow: hidden;
  background: var(--julie-bg-inset);
}

.lang-bar-segment {
  height: 100%;
  min-width: 2px;
  transition: width 0.4s ease;
}

.lang-bar-segment:first-child {
  border-radius: 2px 0 0 2px;
}

.lang-bar-segment:last-child {
  border-radius: 0 2px 2px 0;
}

.lang-bar-segment:only-child {
  border-radius: 2px;
}

/* Larger variant for detail panel */
.lang-bar-track.lang-bar-lg {
  height: 8px;
  border-radius: 4px;
}

.lang-bar-lg .lang-bar-segment:first-child {
  border-radius: 4px 0 0 4px;
}

.lang-bar-lg .lang-bar-segment:last-child {
  border-radius: 0 4px 4px 0;
}

.lang-bar-lg .lang-bar-segment:only-child {
  border-radius: 4px;
}

/* Legend swatch */
.lang-swatch {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 2px;
  margin-right: 0.4rem;
  vertical-align: middle;
}
```

- [ ] **Step 3: Verify CSS is served**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles (rust-embed picks up the changed CSS at compile time)

- [ ] **Step 4: Commit**

```bash
git add dashboard/static/app.css
git commit -m "feat(dashboard): add language color variables and bar CSS"
```

---

### Task 2: Rust — Language Data Helper Function

**Files:**
- Modify: `src/dashboard/routes/projects.rs`

- [ ] **Step 1: Add the `LanguageEntry` struct, `lang_css_var`, and `fetch_language_data`**

Add these at the top of `projects.rs`, after the existing imports:

```rust
use serde::Serialize;

/// A single language in the distribution bar.
#[derive(Debug, Clone, Serialize)]
pub struct LanguageEntry {
    pub name: String,
    pub file_count: i64,
    pub percentage: f64,
    pub css_var: String,
}

/// Map a language name to its CSS custom property name.
fn lang_css_var(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "rust" => "var(--lang-rust)",
        "typescript" | "tsx" => "var(--lang-typescript)",
        "javascript" | "jsx" => "var(--lang-javascript)",
        "python" => "var(--lang-python)",
        "java" => "var(--lang-java)",
        "c_sharp" | "csharp" | "c#" => "var(--lang-csharp)",
        "go" => "var(--lang-go)",
        "c" => "var(--lang-c)",
        "cpp" | "c++" => "var(--lang-cpp)",
        "ruby" => "var(--lang-ruby)",
        "swift" => "var(--lang-swift)",
        "php" => "var(--lang-php)",
        "kotlin" => "var(--lang-kotlin)",
        "html" => "var(--lang-html)",
        "css" => "var(--lang-css)",
        "scala" => "var(--lang-scala)",
        "elixir" => "var(--lang-elixir)",
        "lua" => "var(--lang-lua)",
        "dart" => "var(--lang-dart)",
        "zig" => "var(--lang-zig)",
        "r" => "var(--lang-r)",
        "gdscript" => "var(--lang-gdscript)",
        "vue" => "var(--lang-vue)",
        _ => "var(--lang-other)",
    }
}

/// Fetch language distribution for a workspace via the WorkspacePool.
/// Returns up to `max_entries` named languages; the rest are grouped as "Other".
async fn fetch_language_data(
    state: &AppState,
    workspace_id: &str,
    max_entries: usize,
) -> Vec<LanguageEntry> {
    let pool = match state.dashboard.workspace_pool() {
        Some(p) => p,
        None => return vec![],
    };

    let workspace = match pool.get(workspace_id).await {
        Some(ws) => ws,
        None => return vec![],
    };

    let db = match &workspace.db {
        Some(db) => db,
        None => return vec![],
    };

    let counts = {
        let db_guard = match db.lock() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        match db_guard.count_files_by_language() {
            Ok(c) => c,
            Err(_) => return vec![],
        }
    };

    if counts.is_empty() {
        return vec![];
    }

    let total: i64 = counts.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return vec![];
    }

    let mut entries = Vec::new();
    let mut other_count: i64 = 0;

    for (i, (lang, count)) in counts.iter().enumerate() {
        if i < max_entries {
            entries.push(LanguageEntry {
                name: lang.clone(),
                file_count: *count,
                percentage: (*count as f64 / total as f64) * 100.0,
                css_var: lang_css_var(lang).to_string(),
            });
        } else {
            other_count += count;
        }
    }

    if other_count > 0 {
        entries.push(LanguageEntry {
            name: "Other".to_string(),
            file_count: other_count,
            percentage: (other_count as f64 / total as f64) * 100.0,
            css_var: lang_css_var("other").to_string(),
        });
    }

    entries
}
```

- [ ] **Step 2: Commit**

```bash
git add src/dashboard/routes/projects.rs
git commit -m "feat(dashboard): add language data helper for workspace language distribution"
```

---

### Task 3: Detail Endpoint — Language Data + Metrics Link

**Files:**
- Modify: `src/dashboard/routes/projects.rs`
- Create: `dashboard/templates/partials/language_detail.html`
- Modify: `dashboard/templates/partials/project_detail.html`

- [ ] **Step 1: Wire language data into the `detail` handler**

In `src/dashboard/routes/projects.rs`, in the `detail` function, add after the `index_duration_str` block and before the `let mut context = Context::new();` line:

```rust
    let languages = fetch_language_data(&state, &workspace_id, 8).await;
    let has_languages = !languages.is_empty();
```

Then add to the context (after the existing `context.insert` calls):

```rust
    context.insert("languages", &languages);
    context.insert("has_languages", &has_languages);
```

- [ ] **Step 2: Create the language detail partial**

Create `dashboard/templates/partials/language_detail.html`:

```html
{% if has_languages %}
<div style="margin-top: 0.75rem;">
  <p class="label-text" style="margin-bottom: 0.5rem;">Languages</p>

  <!-- Stacked bar -->
  <div class="lang-bar-track lang-bar-lg" style="margin-bottom: 0.6rem;">
    {% for lang in languages %}
      <div class="lang-bar-segment"
           style="width: {{ lang.percentage }}%; background: {{ lang.css_var }};"
           title="{{ lang.name }}: {{ lang.file_count }} files ({{ lang.percentage | round(precision=1) }}%)">
      </div>
    {% endfor %}
  </div>

  <!-- Legend -->
  <div style="display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; font-size: 0.8rem;">
    {% for lang in languages %}
      <span style="white-space: nowrap;">
        <span class="lang-swatch" style="background: {{ lang.css_var }};"></span>
        <span style="color: var(--julie-text-muted);">{{ lang.name }}</span>
        <span class="mono" style="color: var(--julie-text); margin-left: 0.15rem;">{{ lang.file_count }}</span>
        <span style="color: var(--julie-text-muted); font-size: 0.72rem;">({{ lang.percentage | round(precision=1) }}%)</span>
      </span>
    {% endfor %}
  </div>
</div>
{% endif %}
```

- [ ] **Step 3: Update project_detail.html — add language bar and metrics link**

The existing `project_detail.html` structure is:

```
<div class="columns" style="margin: 0;">   ← line 1
  <div class="column">...</div>            ← col 1: Index Stats
  {% if health %}<div class="column">...</div>{% endif %}  ← col 2: Symbol Stats
  <div class="column">...</div>            ← col 3: References
</div>                                     ← line 78
```

Append AFTER line 78 (the closing `</div>` of the columns wrapper), making the language bar and metrics link a full-width section below the three columns:

```html

<!-- Language distribution -->
{% include "partials/language_detail.html" %}

<!-- Workspace metrics link -->
<div style="margin-top: 0.75rem;">
  <a href="/metrics?workspace={{ workspace.workspace_id }}"
     class="button is-dark is-small"
     style="font-size: 0.78rem;">
    View Metrics &rarr;
  </a>
</div>
```

- [ ] **Step 4: Build and verify templates compile**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src/dashboard/routes/projects.rs dashboard/templates/partials/language_detail.html dashboard/templates/partials/project_detail.html
git commit -m "feat(dashboard): add language breakdown and metrics link to project detail"
```

---

### Task 4: Compact Language Bar in Table Rows

**Files:**
- Modify: `dashboard/templates/partials/project_row.html`
- Modify: `src/dashboard/routes/projects.rs` (statuses endpoint)
- Modify: `dashboard/templates/projects.html` (polling JS)

- [ ] **Step 1: Add the compact bar to project_row.html**

In `dashboard/templates/partials/project_row.html`, the first `<tr>` contains the main row. We want the language bar to appear under the "Path" cell content (the most natural place since it spans visual width).

Modify the Path `<td>` (currently line 10-12) from:

```html
    <td>
      <span class="mono" style="color: var(--julie-text-muted);">{{ ws.path }}</span>
    </td>
```

to:

```html
    <td>
      <span class="mono" style="color: var(--julie-text-muted);">{{ ws.path }}</span>
      <div class="lang-bar-track" id="langbar-{{ ws.workspace_id }}" style="margin-top: 0.3rem;"></div>
    </td>
```

- [ ] **Step 2: Add language bar HTML generation to the `statuses` endpoint**

In `src/dashboard/routes/projects.rs`, in the `statuses` function, we need to generate compact bar HTML for each workspace and include it in the JSON response.

Add a helper function (can go near `lang_css_var`):

```rust
/// Render a compact language bar as an HTML string for the statuses JSON response.
fn render_compact_lang_bar(languages: &[LanguageEntry]) -> String {
    if languages.is_empty() {
        return String::new();
    }
    let mut html = String::new();
    for lang in languages {
        html.push_str(&format!(
            r#"<div class="lang-bar-segment" style="width: {pct}%; background: {color};" title="{name}: {count} files ({pct_r}%)"></div>"#,
            pct = lang.percentage,
            color = lang.css_var,
            name = lang.name,
            count = lang.file_count,
            pct_r = format!("{:.1}", lang.percentage),
        ));
    }
    html
}
```

Then in the `statuses` function, inside the `for ws in &workspaces` loop, fetch language data and add it to the JSON. Add this before the existing `map.insert` calls for each workspace.

The `statuses` function is not async-iterator friendly for calling `fetch_language_data` inside the loop (it's async). Since `statuses` is already async, this works fine. Add the language fetch inside the loop:

After the match on `ws.status` (the badge assignment), and before `map.insert`, add:

```rust
        let languages = fetch_language_data(&state, &ws.workspace_id, 5).await;
        let lang_bar_html = render_compact_lang_bar(&languages);
```

Then add `"lang_bar"` to the JSON object for each workspace. Update both the standard and non-standard status branches. The standard branch becomes:

```rust
        map.insert(
            ws.workspace_id.clone(),
            serde_json::json!({
                "badge": badge,
                "symbols": ws.symbol_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "files": ws.file_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "vectors": ws.vector_count.map(|n| n.to_string()).unwrap_or_else(|| "\u{2014}".into()),
                "lang_bar": lang_bar_html,
            }),
        );
```

Apply the same change to the non-standard status branch (inside the `other =>` arm).

- [ ] **Step 3: Update the polling JS to render language bars**

In `dashboard/templates/projects.html`, inside the `setInterval` callback, after the existing DOM updates (after the `vecs` update), add:

```javascript
      const lb = document.getElementById('langbar-' + id);
      if (lb && ws.lang_bar !== undefined) lb.innerHTML = ws.lang_bar;
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add dashboard/templates/partials/project_row.html src/dashboard/routes/projects.rs dashboard/templates/projects.html
git commit -m "feat(dashboard): add compact language bar to project table rows"
```

---

### Task 5: Integration Test

**Files:**
- Modify: `src/tests/dashboard/integration.rs`

- [ ] **Step 1: Write a test for the detail endpoint**

The existing tests create a `test_state()` with `None` for workspace_pool, so language data will be empty (graceful fallback). We can test that the detail endpoint doesn't crash when there's no workspace pool, and that the template renders without language data.

However, a more useful test verifies the `LanguageEntry` struct and helpers work correctly. Add a unit test in the projects route module.

Add a test at the bottom of `src/tests/dashboard/integration.rs`:

```rust
#[tokio::test]
async fn test_project_detail_returns_200_without_workspace_pool() {
    // With no daemon_db, the detail endpoint should return 404
    // (daemon_db is None, so get_workspace returns NotFound)
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/projects/test_workspace/detail")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    // 404 because daemon_db is None
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}
```

And add a unit test for the helper functions in `projects.rs`. Add a `#[cfg(test)]` block at the bottom of `src/dashboard/routes/projects.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_css_var_known_languages() {
        assert_eq!(lang_css_var("rust"), "var(--lang-rust)");
        assert_eq!(lang_css_var("TypeScript"), "var(--lang-typescript)");
        assert_eq!(lang_css_var("tsx"), "var(--lang-typescript)");
        assert_eq!(lang_css_var("python"), "var(--lang-python)");
        assert_eq!(lang_css_var("c_sharp"), "var(--lang-csharp)");
    }

    #[test]
    fn test_lang_css_var_unknown_falls_back_to_other() {
        assert_eq!(lang_css_var("brainfuck"), "var(--lang-other)");
        assert_eq!(lang_css_var(""), "var(--lang-other)");
    }

    #[test]
    fn test_render_compact_lang_bar_empty() {
        assert_eq!(render_compact_lang_bar(&[]), "");
    }

    #[test]
    fn test_render_compact_lang_bar_single_language() {
        let entries = vec![LanguageEntry {
            name: "Rust".to_string(),
            file_count: 100,
            percentage: 100.0,
            css_var: "var(--lang-rust)".to_string(),
        }];
        let html = render_compact_lang_bar(&entries);
        assert!(html.contains("lang-bar-segment"));
        assert!(html.contains("--lang-rust"));
        assert!(html.contains("Rust: 100 files"));
    }

    #[test]
    fn test_render_compact_lang_bar_multiple_languages() {
        let entries = vec![
            LanguageEntry {
                name: "Rust".to_string(),
                file_count: 70,
                percentage: 70.0,
                css_var: "var(--lang-rust)".to_string(),
            },
            LanguageEntry {
                name: "Python".to_string(),
                file_count: 30,
                percentage: 30.0,
                css_var: "var(--lang-python)".to_string(),
            },
        ];
        let html = render_compact_lang_bar(&entries);
        assert!(html.contains("--lang-rust"));
        assert!(html.contains("--lang-python"));
        assert!(html.contains("width: 70%"));
        assert!(html.contains("width: 30%"));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --lib test_lang_css_var 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_render_compact_lang_bar 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_project_detail_returns_200 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Run xtask dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All buckets pass

- [ ] **Step 4: Commit**

```bash
git add src/dashboard/routes/projects.rs src/tests/dashboard/integration.rs
git commit -m "test(dashboard): add tests for language bar helpers and detail endpoint"
```

---

## Summary

| Task | What | Files |
|------|------|-------|
| 1 | CSS colors + bar styles | `app.css` |
| 2 | `LanguageEntry` struct + `fetch_language_data` + `lang_css_var` helpers | `projects.rs` |
| 3 | Detail endpoint: language breakdown + metrics link | `projects.rs`, `language_detail.html`, `project_detail.html` |
| 4 | Table rows: compact bar + statuses JSON + polling JS | `project_row.html`, `projects.rs`, `projects.html` |
| 5 | Unit + integration tests | `projects.rs`, `integration.rs` |
