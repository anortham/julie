// HTML Extractor Tests
//
// Direct port of Miller's HTML extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/html-extractor.test.ts

use crate::extractors::base::{Symbol, SymbolKind};
// use crate::extractors::html::HTMLExtractor; // TEMPORARILY DISABLED
use tree_sitter::Parser;

/// Initialize HTML parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_html::LANGUAGE.into()).expect("Error loading HTML grammar");
    parser
}

#[cfg(test)]
mod html_extractor_tests {
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

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "index.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Document structure
        let html_element = symbols.iter().find(|s| s.name == "html");
        assert!(html_element.is_some());
        assert_eq!(html_element.unwrap().kind, SymbolKind::Class);
        assert!(html_element.unwrap().signature.as_ref().unwrap().contains(r#"lang="en""#));
        assert!(html_element.unwrap().signature.as_ref().unwrap().contains(r#"data-theme="light""#));

        let head_element = symbols.iter().find(|s| s.name == "head");
        assert!(head_element.is_some());

        let body_element = symbols.iter().find(|s| s.name == "body");
        assert!(body_element.is_some());
        assert!(body_element.unwrap().signature.as_ref().unwrap().contains(r#"class="theme-light""#));

        // Meta elements
        let charset_meta = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"charset="UTF-8""#))
        );
        assert!(charset_meta.is_some());
        assert_eq!(charset_meta.unwrap().kind, SymbolKind::Property);

        let viewport_meta = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"name="viewport""#))
        );
        assert!(viewport_meta.is_some());

        let description_meta = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"name="description""#))
        );
        assert!(description_meta.is_some());

        let og_title_meta = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"property="og:title""#))
        );
        assert!(og_title_meta.is_some());

        let twitter_meta = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"name="twitter:card""#))
        );
        assert!(twitter_meta.is_some());

        // Title
        let title_element = symbols.iter().find(|s| s.name == "title");
        assert!(title_element.is_some());
        assert!(title_element.unwrap().signature.as_ref().unwrap().contains("Project Manager Pro"));

        // Link elements
        let stylesheet_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"rel="stylesheet""#) && sig.contains("main.css")
            )
        );
        assert!(stylesheet_link.is_some());
        assert_eq!(stylesheet_link.unwrap().kind, SymbolKind::Import);

        let preconnect_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"rel="preconnect""#))
        );
        assert!(preconnect_link.is_some());

        let preload_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"rel="preload""#))
        );
        assert!(preload_link.is_some());

        let icon_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"rel="icon""#))
        );
        assert!(icon_link.is_some());

        let manifest_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"rel="manifest""#))
        );
        assert!(manifest_link.is_some());

        // Structured data script
        let structured_data_script = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("application/ld+json"))
        );
        assert!(structured_data_script.is_some());
        assert_eq!(structured_data_script.unwrap().kind, SymbolKind::Variable);

        // Semantic elements
        let header_element = symbols.iter().find(|s| s.name == "header");
        assert!(header_element.is_some());
        assert!(header_element.unwrap().signature.as_ref().unwrap().contains(r#"role="banner""#));

        let nav_element = symbols.iter().find(|s| s.name == "nav");
        assert!(nav_element.is_some());
        assert!(nav_element.unwrap().signature.as_ref().unwrap().contains(r#"role="navigation""#));

        let main_element = symbols.iter().find(|s| s.name == "main");
        assert!(main_element.is_some());
        assert!(main_element.unwrap().signature.as_ref().unwrap().contains(r#"id="main-content""#));
        assert!(main_element.unwrap().signature.as_ref().unwrap().contains(r#"role="main""#));

        let aside_element = symbols.iter().find(|s| s.name == "aside");
        assert!(aside_element.is_some());
        assert!(aside_element.unwrap().signature.as_ref().unwrap().contains(r#"role="complementary""#));

        let footer_element = symbols.iter().find(|s| s.name == "footer");
        assert!(footer_element.is_some());
        assert!(footer_element.unwrap().signature.as_ref().unwrap().contains(r#"role="contentinfo""#));

        // Accessibility elements
        let skip_link = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("Skip to main content"))
        );
        assert!(skip_link.is_some());

        let logo_img = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"alt="Project Manager Pro""#))
        );
        assert!(logo_img.is_some());

        // Interactive elements
        let theme_button = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"data-action="toggle-theme""#))
        );
        assert!(theme_button.is_some());

        let notification_button = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"data-badge="3""#))
        );
        assert!(notification_button.is_some());

        let user_menu_button = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"aria-expanded="false""#) && sig.contains(r#"aria-haspopup="true""#)
            )
        );
        assert!(user_menu_button.is_some());

        // Form elements
        let search_input = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"type="search""#) && sig.contains(r#"id="search-projects""#)
            )
        );
        assert!(search_input.is_some());
        assert_eq!(search_input.unwrap().kind, SymbolKind::Field);

        let status_select = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"id="status-filter""#))
        );
        assert!(status_select.is_some());

        let team_select = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"id="team-filter""#) && sig.contains("multiple")
            )
        );
        assert!(team_select.is_some());

        let fieldset_element = symbols.iter().find(|s| s.name == "fieldset");
        assert!(fieldset_element.is_some());

        let legend_element = symbols.iter().find(|s| s.name == "legend");
        assert!(legend_element.is_some());

        // Checkbox inputs
        let checkbox_inputs: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="checkbox""#)))
            .collect();
        assert!(checkbox_inputs.len() >= 4);

        // Data attributes
        let component_dropdown = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"data-component="dropdown""#))
        );
        assert!(component_dropdown.is_some());

        let env_attribute = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"data-env="production""#))
        );
        assert!(env_attribute.is_some());

        // ARIA attributes
        let aria_current_page = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"aria-current="page""#))
        );
        assert!(aria_current_page.is_some());

        let aria_hidden: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"aria-hidden="true""#)))
            .collect();
        assert!(aria_hidden.len() > 5);

        let aria_label: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("aria-label=")))
            .collect();
        assert!(aria_label.len() > 8);

        // Breadcrumbs
        let breadcrumb_nav = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"aria-label="Breadcrumb""#))
        );
        assert!(breadcrumb_nav.is_some());

        let breadcrumb_list = symbols.iter().find(|s|
            s.name == "ol" && s.signature.as_ref().map_or(false, |sig| sig.contains("breadcrumb"))
        );
        assert!(breadcrumb_list.is_some());

        // Time elements
        let time_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "time")
            .collect();
        assert!(time_elements.len() >= 2);

        let datetime_attribute = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"datetime="2024-01-15T14:30:00Z""#))
        );
        assert!(datetime_attribute.is_some());

        // Article elements
        let article_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "article")
            .collect();
        assert!(article_elements.len() >= 2);

        // Script elements
        let polyfills_script = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"src="/js/vendor/polyfills.js""#))
        );
        assert!(polyfills_script.is_some());
        assert_eq!(polyfills_script.unwrap().kind, SymbolKind::Import);

        let module_script = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="module""#))
        );
        assert!(module_script.is_some());
    }

    #[test]
    fn test_extract_complex_forms_validation_and_interactive_elements() {
        let html_code = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Advanced Form Example</title>
</head>
<body>
  <section class="form-section">
    <h2>Contact Information</h2>

    <form id="contact-form" class="contact-form" novalidate aria-label="Contact form">
      <div class="form-row">
        <div class="form-group">
          <label for="first-name" class="form-label required">
            First Name
            <span class="required-indicator" aria-label="required">*</span>
          </label>
          <input
            type="text"
            id="first-name"
            name="firstName"
            class="form-input"
            required
            autocomplete="given-name"
            aria-describedby="first-name-error"
            minlength="2"
            maxlength="50"
            pattern="[A-Za-z\\s]+"
            placeholder="Enter your first name"
          >
          <div id="first-name-error" class="error-message" role="alert" aria-live="polite"></div>
        </div>

        <div class="form-group">
          <label for="last-name" class="form-label required">
            Last Name
            <span class="required-indicator" aria-label="required">*</span>
          </label>
          <input
            type="text"
            id="last-name"
            name="lastName"
            class="form-input"
            required
            autocomplete="family-name"
            aria-describedby="last-name-error"
            minlength="2"
            maxlength="50"
            pattern="[A-Za-z\\s]+"
            placeholder="Enter your last name"
          >
          <div id="last-name-error" class="error-message" role="alert" aria-live="polite"></div>
        </div>
      </div>

      <div class="form-group">
        <label for="email" class="form-label required">
          Email Address
          <span class="required-indicator" aria-label="required">*</span>
        </label>
        <input
          type="email"
          id="email"
          name="email"
          class="form-input"
          required
          autocomplete="email"
          aria-describedby="email-help email-error"
          placeholder="your.email@example.com"
        >
        <div id="email-help" class="form-help">
          We will never share your email with anyone else.
        </div>
        <div id="email-error" class="error-message" role="alert" aria-live="polite"></div>
      </div>

      <div class="form-group">
        <fieldset class="form-fieldset">
          <legend class="form-legend required">
            Preferred Contact Method
            <span class="required-indicator" aria-label="required">*</span>
          </legend>
          <div class="radio-group" role="radiogroup" aria-describedby="contact-method-error">
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="email" class="radio-input" required>
              <span class="radio-text">Email</span>
            </label>
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="phone" class="radio-input" required>
              <span class="radio-text">Phone</span>
            </label>
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="both" class="radio-input" required>
              <span class="radio-text">Both Email and Phone</span>
            </label>
          </div>
          <div id="contact-method-error" class="error-message" role="alert" aria-live="polite"></div>
        </fieldset>
      </div>

      <div class="form-group">
        <fieldset class="form-fieldset">
          <legend class="form-legend">Interests</legend>
          <div class="checkbox-group" aria-describedby="interests-help">
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="web-development" class="checkbox-input">
              <span class="checkbox-text">Web Development</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="mobile-apps" class="checkbox-input">
              <span class="checkbox-text">Mobile Apps</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="ui-design" class="checkbox-input">
              <span class="checkbox-text">UI/UX Design</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="data-science" class="checkbox-input">
              <span class="checkbox-text">Data Science</span>
            </label>
          </div>
          <div id="interests-help" class="form-help">
            Select all that apply to your interests.
          </div>
        </fieldset>
      </div>

      <div class="form-actions">
        <button type="button" class="btn btn-secondary" data-action="save-draft">
          Save as Draft
        </button>
        <button type="reset" class="btn btn-ghost">
          Clear Form
        </button>
        <button type="submit" class="btn btn-primary">
          <span class="btn-text">Send Message</span>
          <span class="btn-loading" aria-hidden="true">Sending...</span>
        </button>
      </div>
    </form>
  </section>

  <dialog id="confirmation-modal" class="modal" aria-labelledby="modal-title" aria-describedby="modal-description">
    <div class="modal-content">
      <header class="modal-header">
        <h3 id="modal-title" class="modal-title">Confirm Submission</h3>
        <button type="button" class="modal-close" aria-label="Close dialog" data-action="close-modal">
          <span aria-hidden="true">&times;</span>
        </button>
      </header>

      <div class="modal-body">
        <p id="modal-description">
          Are you sure you want to submit this form? Please review your information before proceeding.
        </p>
      </div>

      <footer class="modal-footer">
        <button type="button" class="btn btn-secondary" data-action="cancel">
          Cancel
        </button>
        <button type="button" class="btn btn-primary" data-action="confirm-submit">
          Confirm & Submit
        </button>
      </footer>
    </div>
  </dialog>

  <details class="disclosure" open>
    <summary class="disclosure-summary">
      <span class="summary-text">Advanced Options</span>
      <span class="summary-icon" aria-hidden="true">▼</span>
    </summary>

    <div class="disclosure-content">
      <div class="form-group">
        <label for="timezone" class="form-label">Timezone</label>
        <select id="timezone" name="timezone" class="form-select">
          <option value="">Auto-detect</option>
          <option value="UTC">UTC</option>
          <option value="EST">Eastern Standard Time</option>
          <option value="PST">Pacific Standard Time</option>
          <option value="GMT">Greenwich Mean Time</option>
        </select>
      </div>
    </div>
  </details>
</body>
</html>"###;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "form.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Form element
        let contact_form = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"id="contact-form""#))
        );
        assert!(contact_form.is_some());
        assert!(contact_form.unwrap().signature.as_ref().unwrap().contains("novalidate"));
        assert!(contact_form.unwrap().signature.as_ref().unwrap().contains(r#"aria-label="Contact form""#));

        // Input elements with validation
        let first_name_input = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"id="first-name""#))
        );
        assert!(first_name_input.is_some());
        assert_eq!(first_name_input.unwrap().kind, SymbolKind::Field);
        assert!(first_name_input.unwrap().signature.as_ref().unwrap().contains("required"));
        assert!(first_name_input.unwrap().signature.as_ref().unwrap().contains(r#"autocomplete="given-name""#));
        assert!(first_name_input.unwrap().signature.as_ref().unwrap().contains(r#"pattern="[A-Za-z\\s]+""#));

        let email_input = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="email""#))
        );
        assert!(email_input.is_some());
        assert!(email_input.unwrap().signature.as_ref().unwrap().contains(r#"autocomplete="email""#));

        // Radio buttons
        let radio_inputs: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="radio""#)))
            .collect();
        assert_eq!(radio_inputs.len(), 3);

        let email_radio = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"name="contactMethod""#) && sig.contains(r#"value="email""#)
            )
        );
        assert!(email_radio.is_some());

        // Checkboxes
        let checkbox_inputs: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="checkbox""#)))
            .collect();
        assert!(checkbox_inputs.len() >= 4);

        let web_dev_checkbox = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"value="web-development""#))
        );
        assert!(web_dev_checkbox.is_some());

        // Modal dialog
        let dialog_element = symbols.iter().find(|s| s.name == "dialog");
        assert!(dialog_element.is_some());
        assert!(dialog_element.unwrap().signature.as_ref().unwrap().contains(r#"aria-labelledby="modal-title""#));
        assert!(dialog_element.unwrap().signature.as_ref().unwrap().contains(r#"aria-describedby="modal-description""#));

        // Details/Summary
        let details_element = symbols.iter().find(|s| s.name == "details");
        assert!(details_element.is_some());
        assert!(details_element.unwrap().signature.as_ref().unwrap().contains("open"));

        let summary_element = symbols.iter().find(|s| s.name == "summary");
        assert!(summary_element.is_some());
    }

    #[test]
    fn test_extract_multimedia_elements_svg_canvas_and_embedded_content() {
        let html_code = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Media and Embedded Content</title>
</head>
<body>
  <section class="gallery" aria-label="Photo gallery">
    <h2>Image Gallery</h2>

    <figure class="featured-image">
      <img
        src="/images/hero-image.jpg"
        alt="Beautiful sunset over mountains with vibrant orange and pink colors"
        width="800"
        height="600"
        loading="lazy"
        decoding="async"
        sizes="(max-width: 768px) 100vw, (max-width: 1200px) 50vw, 33vw"
        srcset="
          /images/hero-image-400.jpg 400w,
          /images/hero-image-800.jpg 800w,
          /images/hero-image-1200.jpg 1200w,
          /images/hero-image-1600.jpg 1600w
        "
      >
      <figcaption class="image-caption">
        Sunset over the Rocky Mountains -
        <cite>Photo by Jane Photographer</cite>
        <time datetime="2024-01-15">January 15, 2024</time>
      </figcaption>
    </figure>

    <div class="image-grid">
      <picture class="responsive-image">
        <source
          media="(min-width: 1200px)"
          srcset="/images/gallery-1-large.webp"
          type="image/webp"
        >
        <source
          media="(min-width: 768px)"
          srcset="/images/gallery-1-medium.webp"
          type="image/webp"
        >
        <source
          srcset="/images/gallery-1-small.webp"
          type="image/webp"
        >
        <img
          src="/images/gallery-1-medium.jpg"
          alt="Abstract art piece with geometric patterns"
          loading="lazy"
          decoding="async"
        >
      </picture>

      <img
        src="/images/gallery-2.jpg"
        alt="Modern architecture with glass and steel elements"
        width="400"
        height="300"
        loading="lazy"
        decoding="async"
      >

      <img
        src="data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAwIiBoZWlnaHQ9IjIwMCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KICA8cmVjdCB3aWR0aD0iMTAwJSIgaGVpZ2h0PSIxMDAlIiBmaWxsPSIjZGRkIi8+CiAgPHRleHQgeD0iNTAlIiB5PSI1MCUiIGZvbnQtZmFtaWx5PSJBcmlhbCwgc2Fucy1zZXJpZiIgZm9udC1zaXplPSIxNnB4IiBmaWxsPSIjOTk5IiB0ZXh0LWFuY2hvcj0ibWlkZGxlIiBkeT0iLjNlbSI+UGxhY2Vob2xkZXI8L3RleHQ+Cjwvc3ZnPgo="
        alt="Placeholder image"
        width="200"
        height="200"
        loading="lazy"
      >
    </div>
  </section>

  <section class="video-section" aria-label="Video content">
    <h2>Video Content</h2>

    <div class="video-wrapper">
      <video
        id="main-video"
        class="main-video"
        width="800"
        height="450"
        controls
        preload="metadata"
        poster="/images/video-poster.jpg"
        aria-describedby="video-description"
      >
        <source src="/videos/demo.mp4" type="video/mp4">
        <source src="/videos/demo.webm" type="video/webm">
        <source src="/videos/demo.ogv" type="video/ogg">

        <track
          kind="subtitles"
          src="/videos/demo-en.vtt"
          srclang="en"
          label="English"
          default
        >
        <track
          kind="subtitles"
          src="/videos/demo-es.vtt"
          srclang="es"
          label="Español"
        >
        <track
          kind="captions"
          src="/videos/demo-captions.vtt"
          srclang="en"
          label="English Captions"
        >
        <track
          kind="descriptions"
          src="/videos/demo-descriptions.vtt"
          srclang="en"
          label="Audio Descriptions"
        >

        <p class="video-fallback">
          Your browser doesn't support HTML5 video.
          <a href="/videos/demo.mp4">Download the video</a> instead.
        </p>
      </video>

      <div id="video-description" class="video-description">
        Product demonstration showing key features and user interface walkthrough.
      </div>
    </div>
  </section>

  <section class="audio-section" aria-label="Audio content">
    <h2>Audio Content</h2>

    <div class="audio-player">
      <audio
        id="podcast-player"
        class="audio-element"
        preload="none"
        aria-describedby="audio-description"
      >
        <source src="/audio/podcast-episode-1.mp3" type="audio/mpeg">
        <source src="/audio/podcast-episode-1.ogg" type="audio/ogg">
        <source src="/audio/podcast-episode-1.wav" type="audio/wav">

        <p class="audio-fallback">
          Your browser doesn't support HTML5 audio.
          <a href="/audio/podcast-episode-1.mp3">Download the audio file</a> instead.
        </p>
      </audio>

      <div class="audio-info">
        <h3 class="audio-title">Tech Talk Episode 1: Web Accessibility</h3>
        <p id="audio-description" class="audio-description">
          In this episode, we discuss the importance of web accessibility and practical tips for developers.
        </p>
        <div class="audio-meta">
          <span class="duration">Duration: 45 minutes</span>
          <span class="file-size">Size: 32.5 MB</span>
        </div>
      </div>
    </div>
  </section>

  <section class="graphics-section" aria-label="Vector graphics">
    <h2>SVG Graphics</h2>

    <div class="svg-container">
      <svg
        width="300"
        height="200"
        viewBox="0 0 300 200"
        xmlns="http://www.w3.org/2000/svg"
        role="img"
        aria-labelledby="chart-title chart-desc"
      >
        <title id="chart-title">Sales Data Chart</title>
        <desc id="chart-desc">
          Bar chart showing quarterly sales data with values for Q1: 100, Q2: 150, Q3: 200, Q4: 175
        </desc>

        <defs>
          <linearGradient id="barGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" style="stop-color:#4285f4;stop-opacity:1" />
            <stop offset="100%" style="stop-color:#1a73e8;stop-opacity:1" />
          </linearGradient>
        </defs>

        <rect x="0" y="0" width="300" height="200" fill="#fafafa" stroke="#ddd" stroke-width="1"/>

        <rect x="40" y="120" width="40" height="60" fill="url(#barGradient)" aria-label="Q1: 100">
          <title>Q1: $100k</title>
        </rect>
        <rect x="100" y="95" width="40" height="85" fill="url(#barGradient)" aria-label="Q2: 150">
          <title>Q2: $150k</title>
        </rect>
        <rect x="160" y="70" width="40" height="110" fill="url(#barGradient)" aria-label="Q3: 200">
          <title>Q3: $200k</title>
        </rect>
        <rect x="220" y="82" width="40" height="98" fill="url(#barGradient)" aria-label="Q4: 175">
          <title>Q4: $175k</title>
        </rect>

        <text x="60" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q1</text>
        <text x="120" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q2</text>
        <text x="180" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q3</text>
        <text x="240" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q4</text>

        <circle cx="150" cy="50" r="20" fill="#ff6b6b" opacity="0.8">
          <animate attributeName="r" values="15;25;15" dur="2s" repeatCount="indefinite"/>
        </circle>
      </svg>

      <img src="/images/logo.svg" alt="Company logo" width="150" height="75" class="svg-logo">

      <object data="/images/infographic.svg" type="image/svg+xml" width="400" height="300" aria-label="Data infographic">
        <img src="/images/infographic-fallback.png" alt="Data infographic showing key statistics">
      </object>
    </div>
  </section>

  <section class="canvas-section" aria-label="Interactive graphics">
    <h2>Canvas Graphics</h2>

    <div class="canvas-container">
      <canvas
        id="interactive-chart"
        class="chart-canvas"
        width="600"
        height="400"
        role="img"
        aria-label="Interactive data visualization"
        aria-describedby="canvas-description"
      >
        <p id="canvas-description">
          Interactive chart showing real-time data. Canvas is not supported in your browser.
          <a href="/data.csv">Download the raw data</a> instead.
        </p>
      </canvas>
    </div>
  </section>

  <section class="embed-section" aria-label="Embedded content">
    <h2>Embedded Content</h2>

    <div class="video-embed">
      <iframe
        width="560"
        height="315"
        src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"
        title="Sample Video"
        frameborder="0"
        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
        allowfullscreen
        loading="lazy"
        aria-describedby="embed-description"
      ></iframe>
      <div id="embed-description" class="embed-description">
        Educational video about web development best practices.
      </div>
    </div>

    <div class="map-embed">
      <iframe
        src="https://www.openstreetmap.org/export/embed.html?bbox=-0.004017949104309083%2C51.47612752641776%2C0.00030577182769775396%2C51.478569861898606&layer=mapnik"
        width="400"
        height="300"
        frameborder="0"
        title="Office location map"
        aria-label="Interactive map showing office location"
        loading="lazy"
      ></iframe>
    </div>

    <embed
      src="/documents/presentation.pdf"
      type="application/pdf"
      width="600"
      height="400"
      aria-label="Product presentation PDF"
    >
  </section>

  <section class="components-section" aria-label="Custom web components">
    <h2>Custom Elements</h2>

    <custom-video-player
      src="/videos/demo.mp4"
      poster="/images/video-poster.jpg"
      controls="true"
      autoplay="false"
      aria-label="Custom video player component"
    >
      <p slot="fallback">Video player not supported in your browser.</p>
    </custom-video-player>

    <data-visualization
      type="chart"
      data-source="/api/analytics"
      refresh-interval="30000"
      aria-label="Real-time analytics dashboard"
    ></data-visualization>

    <image-gallery
      images='[
        {"src": "/images/1.jpg", "alt": "Image 1", "caption": "First image"},
        {"src": "/images/2.jpg", "alt": "Image 2", "caption": "Second image"}
      ]'
      layout="grid"
      lazy-loading="true"
    ></image-gallery>
  </section>
</body>
</html>"###;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "media.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Image elements
        let hero_image = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"src="/images/hero-image.jpg""#))
        );
        assert!(hero_image.is_some());
        assert_eq!(hero_image.unwrap().kind, SymbolKind::Variable); // Media elements as variables
        assert!(hero_image.unwrap().signature.as_ref().unwrap().contains(r#"loading="lazy""#));
        assert!(hero_image.unwrap().signature.as_ref().unwrap().contains(r#"decoding="async""#));
        assert!(hero_image.unwrap().signature.as_ref().unwrap().contains("srcset="));
        assert!(hero_image.unwrap().signature.as_ref().unwrap().contains("sizes="));

        let data_uri_image = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("data:image/svg+xml;base64"))
        );
        assert!(data_uri_image.is_some());

        // Figure and figcaption
        let figure_element = symbols.iter().find(|s| s.name == "figure");
        assert!(figure_element.is_some());

        let figcaption_element = symbols.iter().find(|s| s.name == "figcaption");
        assert!(figcaption_element.is_some());

        let cite_element = symbols.iter().find(|s| s.name == "cite");
        assert!(cite_element.is_some());

        // Picture and source elements
        let picture_element = symbols.iter().find(|s| s.name == "picture");
        assert!(picture_element.is_some());

        let source_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "source")
            .collect();
        assert!(source_elements.len() >= 5); // Picture sources + video/audio sources

        let webp_source = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="image/webp""#))
        );
        assert!(webp_source.is_some());

        let media_source = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"media="(min-width: 1200px)""#))
        );
        assert!(media_source.is_some());

        // Video element
        let video_element = symbols.iter().find(|s| s.name == "video");
        assert!(video_element.is_some());
        assert!(video_element.unwrap().signature.as_ref().unwrap().contains("controls"));
        assert!(video_element.unwrap().signature.as_ref().unwrap().contains(r#"preload="metadata""#));
        assert!(video_element.unwrap().signature.as_ref().unwrap().contains(r#"poster="/images/video-poster.jpg""#));

        // Track elements
        let track_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "track")
            .collect();
        assert_eq!(track_elements.len(), 4);

        let subtitles_track = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig|
                sig.contains(r#"kind="subtitles""#) && sig.contains(r#"srclang="en""#)
            )
        );
        assert!(subtitles_track.is_some());

        let captions_track = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"kind="captions""#))
        );
        assert!(captions_track.is_some());

        let descriptions_track = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"kind="descriptions""#))
        );
        assert!(descriptions_track.is_some());

        let default_track = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("default")) && s.name == "track"
        );
        assert!(default_track.is_some());

        // Audio element
        let audio_element = symbols.iter().find(|s| s.name == "audio");
        assert!(audio_element.is_some());
        assert!(audio_element.unwrap().signature.as_ref().unwrap().contains(r#"preload="none""#));

        let mp3_source = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="audio/mpeg""#))
        );
        assert!(mp3_source.is_some());

        let ogg_source = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"type="audio/ogg""#))
        );
        assert!(ogg_source.is_some());

        // SVG element
        let svg_element = symbols.iter().find(|s| s.name == "svg");
        assert!(svg_element.is_some());
        assert!(svg_element.unwrap().signature.as_ref().unwrap().contains(r#"role="img""#));
        assert!(svg_element.unwrap().signature.as_ref().unwrap().contains(r#"aria-labelledby="chart-title chart-desc""#));

        let title_element = symbols.iter().find(|s|
            s.name == "title" && s.signature.as_ref().map_or(false, |sig| sig.contains("Sales Data Chart"))
        );
        assert!(title_element.is_some());

        let desc_element = symbols.iter().find(|s| s.name == "desc");
        assert!(desc_element.is_some());

        // SVG shapes
        let rect_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "rect")
            .collect();
        assert!(rect_elements.len() >= 5);

        let circle_element = symbols.iter().find(|s| s.name == "circle");
        assert!(circle_element.is_some());

        let text_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "text")
            .collect();
        assert!(text_elements.len() >= 4);

        // SVG animation
        let animate_element = symbols.iter().find(|s| s.name == "animate");
        assert!(animate_element.is_some());
        assert!(animate_element.unwrap().signature.as_ref().unwrap().contains(r#"attributeName="r""#));
        assert!(animate_element.unwrap().signature.as_ref().unwrap().contains(r#"repeatCount="indefinite""#));

        // Object element
        let object_element = symbols.iter().find(|s| s.name == "object");
        assert!(object_element.is_some());
        assert!(object_element.unwrap().signature.as_ref().unwrap().contains(r#"type="image/svg+xml""#));

        // Canvas element
        let canvas_element = symbols.iter().find(|s| s.name == "canvas");
        assert!(canvas_element.is_some());
        assert!(canvas_element.unwrap().signature.as_ref().unwrap().contains(r#"role="img""#));
        assert!(canvas_element.unwrap().signature.as_ref().unwrap().contains(r#"aria-describedby="canvas-description""#));

        // Iframe elements
        let iframe_elements: Vec<_> = symbols.iter()
            .filter(|s| s.name == "iframe")
            .collect();
        assert_eq!(iframe_elements.len(), 2);

        let youtube_iframe = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("youtube-nocookie.com"))
        );
        assert!(youtube_iframe.is_some());
        assert!(youtube_iframe.unwrap().signature.as_ref().unwrap().contains("allowfullscreen"));
        assert!(youtube_iframe.unwrap().signature.as_ref().unwrap().contains(r#"loading="lazy""#));

        let map_iframe = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("openstreetmap.org"))
        );
        assert!(map_iframe.is_some());

        // Embed element
        let embed_element = symbols.iter().find(|s| s.name == "embed");
        assert!(embed_element.is_some());
        assert!(embed_element.unwrap().signature.as_ref().unwrap().contains(r#"type="application/pdf""#));

        // Custom elements
        let custom_video_player = symbols.iter().find(|s| s.name == "custom-video-player");
        assert!(custom_video_player.is_some());
        assert_eq!(custom_video_player.unwrap().kind, SymbolKind::Class);
        assert!(custom_video_player.unwrap().signature.as_ref().unwrap().contains(r#"controls="true""#));

        let data_visualization = symbols.iter().find(|s| s.name == "data-visualization");
        assert!(data_visualization.is_some());
        assert!(data_visualization.unwrap().signature.as_ref().unwrap().contains(r#"data-source="/api/analytics""#));

        let image_gallery = symbols.iter().find(|s| s.name == "image-gallery");
        assert!(image_gallery.is_some());
        assert!(image_gallery.unwrap().signature.as_ref().unwrap().contains(r#"lazy-loading="true""#));

        // Slot element
        let slot_element = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains(r#"slot="fallback""#))
        );
        assert!(slot_element.is_some());

        // Media-specific attributes
        let loading_lazy: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"loading="lazy""#)))
            .collect();
        assert!(loading_lazy.len() > 5);

        let decoding_async: Vec<_> = symbols.iter()
            .filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains(r#"decoding="async""#)))
            .collect();
        assert!(decoding_async.len() >= 3);

        let allow_attribute = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("allow=\"accelerometer; autoplay"))
        );
        assert!(allow_attribute.is_some());

        // Fallback content
        let video_fallback = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("Your browser doesn't support HTML5 video"))
        );
        assert!(video_fallback.is_some());

        let audio_fallback = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("Your browser doesn't support HTML5 audio"))
        );
        assert!(audio_fallback.is_some());

        let canvas_fallback = symbols.iter().find(|s|
            s.signature.as_ref().map_or(false, |sig| sig.contains("Canvas is not supported in your browser"))
        );
        assert!(canvas_fallback.is_some());
    }
}