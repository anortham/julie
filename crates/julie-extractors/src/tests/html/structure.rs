use super::{SymbolKind, extract_symbols};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_document_structure_semantic_elements_and_attributes() {
        let html_code = r###"<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="description" content="A comprehensive web application for project management">
  <meta name="keywords" content="project, management, productivity, collaboration">
  <meta name="author" content="Development Team">
  <meta property="og:title" content="Project Manager Pro">
  <meta property="og:description" content="Streamline your project workflow">
  <meta property="og:image" content="/images/og-image.jpg">
  <meta property="og:url" content="https://projectmanager.example.com">
  <meta name="twitter:card" content="summary_large_image">

  <title>Project Manager Pro - Streamline Your Workflow</title>

  <link rel="stylesheet" href="/css/main.css">
  <link rel="stylesheet" href="/css/themes.css">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preload" href="/fonts/inter.woff2" as="font" type="font/woff2" crossorigin>
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
  <link rel="manifest" href="/site.webmanifest">

  <script type="application/ld+json">
  {
    "@context": "https://schema.org",
    "@type": "WebApplication",
    "name": "Project Manager Pro",
    "description": "A comprehensive project management tool",
    "url": "https://projectmanager.example.com",
    "applicationCategory": "ProductivityApplication",
    "operatingSystem": "Web Browser"
  }
  </script>
</head>

<body class="theme-light" data-env="production">
  <div id="app" class="app-container">
    <a href="#main-content" class="skip-link">Skip to main content</a>

    <header class="header" role="banner">
      <div class="container">
        <div class="header-content">
          <a href="/" class="logo" aria-label="Project Manager Pro Homepage">
            <img src="/images/logo.svg" alt="Project Manager Pro" width="120" height="40">
          </a>

          <nav class="navigation" role="navigation" aria-label="Main navigation">
            <ul class="nav-list">
              <li class="nav-item">
                <a href="/dashboard" class="nav-link" aria-current="page">Dashboard</a>
              </li>
              <li class="nav-item">
                <a href="/projects" class="nav-link">Projects</a>
              </li>
              <li class="nav-item">
                <a href="/team" class="nav-link">Team</a>
              </li>
              <li class="nav-item">
                <a href="/analytics" class="nav-link">Analytics</a>
              </li>
            </ul>
          </nav>

          <div class="header-actions">
            <button type="button" class="btn btn-icon" aria-label="Toggle theme" data-action="toggle-theme">
              <span class="icon-theme" aria-hidden="true"></span>
            </button>

            <button type="button" class="btn btn-icon" aria-label="Notifications" data-badge="3">
              <span class="icon-bell" aria-hidden="true"></span>
            </button>

            <div class="user-menu" data-component="dropdown">
              <button type="button" class="user-avatar" aria-label="User menu" aria-expanded="false" aria-haspopup="true">
                <img src="/images/avatars/user-123.jpg" alt="John Doe" width="32" height="32">
              </button>

              <div class="dropdown-menu" role="menu" aria-hidden="true">
                <a href="/profile" class="dropdown-item" role="menuitem">Profile</a>
                <a href="/settings" class="dropdown-item" role="menuitem">Settings</a>
                <hr class="dropdown-divider">
                <button type="button" class="dropdown-item" role="menuitem" data-action="logout">
                  Sign Out
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </header>

    <main id="main-content" class="main" role="main">
      <div class="container">
        <div class="page-header">
          <nav aria-label="Breadcrumb">
            <ol class="breadcrumb">
              <li class="breadcrumb-item">
                <a href="/dashboard">Dashboard</a>
              </li>
              <li class="breadcrumb-item active" aria-current="page">
                Projects
              </li>
            </ol>
          </nav>

          <div class="page-title-section">
            <h1 class="page-title">Projects Overview</h1>
            <p class="page-description">
              Manage and track all your active projects in one place
            </p>
          </div>

          <div class="page-actions">
            <button type="button" class="btn btn-secondary" data-action="export">
              <span class="icon-download" aria-hidden="true"></span>
              Export Data
            </button>
            <button type="button" class="btn btn-primary" data-action="create-project">
              <span class="icon-plus" aria-hidden="true"></span>
              New Project
            </button>
          </div>
        </div>

        <section class="filters-section" aria-label="Project filters">
          <div class="filters-header">
            <h2 class="filters-title">Filter Projects</h2>
            <button type="button" class="btn btn-ghost btn-sm" data-action="clear-filters">
              Clear All
            </button>
          </div>

          <div class="filters-grid">
            <div class="filter-group">
              <label for="search-projects" class="filter-label">Search</label>
              <div class="search-input-wrapper">
                <input
                  type="search"
                  id="search-projects"
                  class="search-input"
                  placeholder="Search by name, description, or tags..."
                  aria-describedby="search-help"
                  autocomplete="off"
                  spellcheck="false"
                >
                <button type="button" class="search-clear" aria-label="Clear search" hidden>
                  <span class="icon-x" aria-hidden="true"></span>
                </button>
              </div>
              <div id="search-help" class="filter-help">
                Use keywords to find specific projects
              </div>
            </div>

            <div class="filter-group">
              <label for="status-filter" class="filter-label">Status</label>
              <select id="status-filter" class="filter-select" aria-describedby="status-help">
                <option value="">All Statuses</option>
                <option value="planning">Planning</option>
                <option value="active">Active</option>
                <option value="on-hold">On Hold</option>
                <option value="completed">Completed</option>
                <option value="cancelled">Cancelled</option>
              </select>
              <div id="status-help" class="filter-help">
                Filter by project status
              </div>
            </div>

            <div class="filter-group">
              <label for="team-filter" class="filter-label">Team</label>
              <select id="team-filter" class="filter-select" multiple aria-describedby="team-help">
                <option value="frontend">Frontend Team</option>
                <option value="backend">Backend Team</option>
                <option value="design">Design Team</option>
                <option value="qa">QA Team</option>
                <option value="devops">DevOps Team</option>
              </select>
              <div id="team-help" class="filter-help">
                Select one or more teams
              </div>
            </div>

            <div class="filter-group">
              <fieldset class="priority-fieldset">
                <legend class="filter-label">Priority</legend>
                <div class="checkbox-group">
                  <label class="checkbox-label">
                    <input type="checkbox" class="checkbox-input" name="priority" value="low">
                    <span class="checkbox-text">Low</span>
                  </label>
                  <label class="checkbox-label">
                    <input type="checkbox" class="checkbox-input" name="priority" value="medium">
                    <span class="checkbox-text">Medium</span>
                  </label>
                  <label class="checkbox-label">
                    <input type="checkbox" class="checkbox-input" name="priority" value="high">
                    <span class="checkbox-text">High</span>
                  </label>
                  <label class="checkbox-label">
                    <input type="checkbox" class="checkbox-input" name="priority" value="critical">
                    <span class="checkbox-text">Critical</span>
                  </label>
                </div>
              </fieldset>
            </div>
          </div>
        </section>
      </div>
    </main>

    <aside class="sidebar" role="complementary" aria-label="Additional information">
      <div class="sidebar-content">
        <section class="sidebar-section">
          <h3 class="sidebar-title">Quick Stats</h3>
          <div class="stats-grid">
            <div class="stat-item">
              <div class="stat-value" data-value="24">24</div>
              <div class="stat-label">Active Projects</div>
            </div>
            <div class="stat-item">
              <div class="stat-value" data-value="156">156</div>
              <div class="stat-label">Total Tasks</div>
            </div>
            <div class="stat-item">
              <div class="stat-value" data-value="8">8</div>
              <div class="stat-label">Team Members</div>
            </div>
          </div>
        </section>

        <section class="sidebar-section">
          <h3 class="sidebar-title">Recent Activity</h3>
          <div class="activity-list">
            <article class="activity-item">
              <time class="activity-time" datetime="2024-01-15T14:30:00Z">
                2 hours ago
              </time>
              <div class="activity-content">
                <strong>Sarah Chen</strong> completed task "Design mockups"
              </div>
            </article>

            <article class="activity-item">
              <time class="activity-time" datetime="2024-01-15T12:15:00Z">
                4 hours ago
              </time>
              <div class="activity-content">
                <strong>Mike Johnson</strong> created new project "Mobile App Redesign"
              </div>
            </article>
          </div>
        </section>
      </div>
    </aside>

    <footer class="footer" role="contentinfo">
      <div class="container">
        <div class="footer-content">
          <div class="footer-section">
            <h4 class="footer-title">Product</h4>
            <ul class="footer-links">
              <li><a href="/features">Features</a></li>
              <li><a href="/pricing">Pricing</a></li>
              <li><a href="/integrations">Integrations</a></li>
            </ul>
          </div>

          <div class="footer-section">
            <h4 class="footer-title">Support</h4>
            <ul class="footer-links">
              <li><a href="/help">Help Center</a></li>
              <li><a href="/contact">Contact Us</a></li>
              <li><a href="/status">System Status</a></li>
            </ul>
          </div>

          <div class="footer-section">
            <h4 class="footer-title">Legal</h4>
            <ul class="footer-links">
              <li><a href="/privacy">Privacy Policy</a></li>
              <li><a href="/terms">Terms of Service</a></li>
              <li><a href="/cookies">Cookie Policy</a></li>
            </ul>
          </div>
        </div>

        <div class="footer-bottom">
          <p class="copyright">
            &copy; 2024 Project Manager Pro. All rights reserved.
          </p>

          <div class="social-links">
            <a href="https://twitter.com/projectmanagerpro" class="social-link" aria-label="Follow us on Twitter">
              <span class="icon-twitter" aria-hidden="true"></span>
            </a>
            <a href="https://github.com/projectmanagerpro" class="social-link" aria-label="View our code on GitHub">
              <span class="icon-github" aria-hidden="true"></span>
            </a>
            <a href="https://linkedin.com/company/projectmanagerpro" class="social-link" aria-label="Connect with us on LinkedIn">
              <span class="icon-linkedin" aria-hidden="true"></span>
            </a>
          </div>
        </div>
      </div>
    </footer>
  </div>

  <script src="/js/vendor/polyfills.js" defer></script>
  <script src="/js/main.js" type="module" defer></script>
</body>
</html>"###;
        let symbols = extract_symbols(html_code);

        // Document structure
        let html_element = symbols.iter().find(|s| s.name == "html");
        assert!(html_element.is_some());
        assert_eq!(html_element.unwrap().kind, SymbolKind::Class);
        assert!(
            html_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"lang="en""#)
        );
        assert!(
            html_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"data-theme="light""#)
        );

        let head_element = symbols.iter().find(|s| s.name == "head");
        assert!(head_element.is_some());

        let body_element = symbols.iter().find(|s| s.name == "body");
        assert!(body_element.is_some());
        assert!(
            body_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"class="theme-light""#)
        );

        // Meta elements
        let charset_meta = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"charset="UTF-8""#))
        });
        assert!(charset_meta.is_some());
        assert_eq!(charset_meta.unwrap().kind, SymbolKind::Property);

        let viewport_meta = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"name="viewport""#))
        });
        assert!(viewport_meta.is_some());

        let description_meta = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"name="description""#))
        });
        assert!(description_meta.is_some());

        let og_title_meta = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"property="og:title""#))
        });
        assert!(og_title_meta.is_some());

        let twitter_meta = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"name="twitter:card""#))
        });
        assert!(twitter_meta.is_some());

        // Title
        let title_element = symbols.iter().find(|s| s.name == "title");
        assert!(title_element.is_some());
        assert!(
            title_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Project Manager Pro")
        );

        // Link elements
        let stylesheet_link = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"rel="stylesheet""#) && sig.contains("main.css")
            })
        });
        assert!(stylesheet_link.is_some());
        assert_eq!(stylesheet_link.unwrap().kind, SymbolKind::Import);

        let preconnect_link = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"rel="preconnect""#))
        });
        assert!(preconnect_link.is_some());

        let preload_link = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"rel="preload""#))
        });
        assert!(preload_link.is_some());

        let icon_link = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"rel="icon""#))
        });
        assert!(icon_link.is_some());

        let manifest_link = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"rel="manifest""#))
        });
        assert!(manifest_link.is_some());

        // Structured data script
        let structured_data_script = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("application/ld+json"))
        });
        assert!(structured_data_script.is_some());
        assert_eq!(structured_data_script.unwrap().kind, SymbolKind::Variable);

        // Semantic elements
        let header_element = symbols.iter().find(|s| s.name == "header");
        assert!(header_element.is_some());
        assert!(
            header_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"role="banner""#)
        );

        let nav_element = symbols.iter().find(|s| s.name == "nav");
        assert!(nav_element.is_some());
        assert!(
            nav_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"role="navigation""#)
        );

        let main_element = symbols.iter().find(|s| s.name == "main");
        assert!(main_element.is_some());
        assert!(
            main_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"id="main-content""#)
        );
        assert!(
            main_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"role="main""#)
        );

        let aside_element = symbols.iter().find(|s| s.name == "aside");
        assert!(aside_element.is_some());
        assert!(
            aside_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"role="complementary""#)
        );

        let footer_element = symbols.iter().find(|s| s.name == "footer");
        assert!(footer_element.is_some());
        assert!(
            footer_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"role="contentinfo""#)
        );

        // Accessibility elements
        let skip_link = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("Skip to main content"))
        });
        assert!(skip_link.is_some());

        let logo_img = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"alt="Project Manager Pro""#))
        });
        assert!(logo_img.is_some());

        // Interactive elements
        let theme_button = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"data-action="toggle-theme""#))
        });
        assert!(theme_button.is_some());

        let notification_button = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"data-badge="3""#))
        });
        assert!(notification_button.is_some());

        let user_menu_button = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"aria-expanded="false""#) && sig.contains(r#"aria-haspopup="true""#)
            })
        });
        assert!(user_menu_button.is_some());

        // Form elements
        let search_input = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"type="search""#) && sig.contains(r#"id="search-projects""#)
            })
        });
        assert!(search_input.is_some());
        assert_eq!(search_input.unwrap().kind, SymbolKind::Field);

        let status_select = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"id="status-filter""#))
        });
        assert!(status_select.is_some());

        let team_select = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"id="team-filter""#) && sig.contains("multiple")
            })
        });
        assert!(team_select.is_some());

        let fieldset_element = symbols.iter().find(|s| s.name == "fieldset");
        assert!(fieldset_element.is_some());

        let legend_element = symbols.iter().find(|s| s.name == "legend");
        assert!(legend_element.is_some());

        // Checkbox inputs
        let checkbox_inputs: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"type="checkbox""#))
            })
            .collect();
        assert!(checkbox_inputs.len() >= 4);

        // Data attributes
        let component_dropdown = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"data-component="dropdown""#))
        });
        assert!(component_dropdown.is_some());

        let env_attribute = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"data-env="production""#))
        });
        assert!(env_attribute.is_some());

        // ARIA attributes
        let aria_current_page = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"aria-current="page""#))
        });
        assert!(aria_current_page.is_some());

        let aria_hidden: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"aria-hidden="true""#))
            })
            .collect();
        assert!(aria_hidden.len() > 5);

        let aria_label: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("aria-label="))
            })
            .collect();
        assert!(aria_label.len() > 8);

        // Breadcrumbs
        let breadcrumb_nav = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"aria-label="Breadcrumb""#))
        });
        assert!(breadcrumb_nav.is_some());

        let breadcrumb_list = symbols.iter().find(|s| {
            s.name == "ol"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("breadcrumb"))
        });
        assert!(breadcrumb_list.is_some());

        // Time elements
        let time_elements: Vec<_> = symbols.iter().filter(|s| s.name == "time").collect();
        assert!(time_elements.len() >= 2);

        let datetime_attribute = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"datetime="2024-01-15T14:30:00Z""#)
            })
        });
        assert!(datetime_attribute.is_some());

        // Article elements
        let article_elements: Vec<_> = symbols.iter().filter(|s| s.name == "article").collect();
        assert!(article_elements.len() >= 2);

        // Script elements
        let polyfills_script = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"src="/js/vendor/polyfills.js""#)
            })
        });
        assert!(polyfills_script.is_some());
        assert_eq!(polyfills_script.unwrap().kind, SymbolKind::Import);

        let module_script = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"type="module""#))
        });
        assert!(module_script.is_some());
    }
}
