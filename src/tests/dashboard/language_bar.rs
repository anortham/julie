use crate::dashboard::routes::projects::{LanguageEntry, lang_css_var, render_compact_lang_bar};

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
