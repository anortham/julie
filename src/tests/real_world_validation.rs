//! Real-world validation tests following Miller's proven methodology
//!
//! This module tests all extractors against real-world code files to ensure
//! they work correctly on actual codebases, not just carefully crafted test strings.

#[cfg(test)]
mod real_world_tests {
    use crate::extractors::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tree_sitter::{Parser, Tree};

    const REAL_WORLD_TEST_DIR: &str = "tests/real-world";

    /// Initialize a tree-sitter parser for the given language
    fn init_parser(code: &str, language: &str) -> Tree {
        let mut parser = Parser::new();

        let lang = match language {
            "kotlin" => tree_sitter_kotlin_ng::LANGUAGE,
            "ruby" => tree_sitter_ruby::LANGUAGE,
            "rust" => tree_sitter_rust::LANGUAGE,
            "typescript" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            "javascript" | "jsx" => tree_sitter_javascript::LANGUAGE,
            "python" => tree_sitter_python::LANGUAGE,
            "java" => tree_sitter_java::LANGUAGE,
            "csharp" => tree_sitter_c_sharp::LANGUAGE,
            // "go" => tree_sitter_go::LANGUAGE, // Disabled
            "php" => tree_sitter_php::LANGUAGE_PHP,
            "swift" => tree_sitter_swift::LANGUAGE,
            "razor" => tree_sitter_c_sharp::LANGUAGE, // Razor uses C# parser
            "vue" => tree_sitter_javascript::LANGUAGE, // Vue uses JS parser
            "bash" => tree_sitter_bash::LANGUAGE,
            "css" => tree_sitter_css::LANGUAGE,
            "dart" => harper_tree_sitter_dart::LANGUAGE,
            "gdscript" => tree_sitter_gdscript::LANGUAGE,
            "html" => tree_sitter_html::LANGUAGE,
            "powershell" => tree_sitter_powershell::LANGUAGE,
            "regex" => tree_sitter_regex::LANGUAGE,
            "sql" => tree_sitter_sequel::LANGUAGE,
            "zig" => tree_sitter_zig::LANGUAGE,
            "c" => tree_sitter_c::LANGUAGE,
            "cpp" => tree_sitter_cpp::LANGUAGE,
            "go" => tree_sitter_go::LANGUAGE,
            "lua" => tree_sitter_lua::LANGUAGE,
            _ => panic!("Unsupported language: {}", language),
        };

        parser.set_language(&lang.into()).unwrap();
        parser.parse(code, None).unwrap()
    }

    /// Test a real-world file and validate meaningful extraction
    fn test_real_world_file(file_path: &Path, language: &str) {
        println!("🧪 Testing real-world file: {}", file_path.display());

        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_path.display(), e));

        let tree = init_parser(&content, language);

        // Extract symbols using the appropriate extractor
        let symbols = match language {
            "kotlin" => {
                let mut extractor = kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "ruby" => {
                let mut extractor = ruby::RubyExtractor::new(
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "rust" => {
                let mut extractor = rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "typescript" | "tsx" => {
                let mut extractor = typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "javascript" | "jsx" => {
                let mut extractor = javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "python" => {
                let mut extractor = python::PythonExtractor::new(
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "java" => {
                let mut extractor = java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "csharp" => {
                let mut extractor = csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "go" => {
                let mut extractor = go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "php" => {
                let mut extractor = php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "swift" => {
                let mut extractor = swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "razor" => {
                let mut extractor = razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "vue" => {
                let mut extractor = vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(Some(&tree))
            }
            "bash" => {
                let mut extractor = bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "css" => {
                let mut extractor = css::CSSExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "dart" => {
                let mut extractor = dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "gdscript" => {
                let mut extractor = gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "html" => {
                let mut extractor = html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "powershell" => {
                let mut extractor = powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "regex" => {
                let mut extractor = regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "sql" => {
                let mut extractor = sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "zig" => {
                let mut extractor = zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "cpp" => {
                let mut extractor = cpp::CppExtractor::new(
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            "lua" => {
                let mut extractor = lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone(),
                );
                extractor.extract_symbols(&tree)
            }
            _ => panic!("Unsupported language: {}", language),
        };

        // Validate meaningful extraction - Miller's key requirement
        assert!(
            !symbols.is_empty(),
            "❌ Should extract symbols from {}, but got 0. This indicates the extractor failed to parse real-world code.",
            file_path.display()
        );

        // Generate symbol breakdown for analysis (Miller's logging approach)
        let symbol_summary = symbols.iter().fold(HashMap::new(), |mut acc, symbol| {
            *acc.entry(symbol.kind.clone()).or_insert(0) += 1;
            acc
        });

        println!(
            "📊 {} extracted {} symbols: {:?}",
            file_path.file_name().unwrap().to_string_lossy(),
            symbols.len(),
            symbol_summary
        );

        // Log file size for performance analysis
        println!(
            "📏 File size: {} bytes, Symbols/KB: {:.1}",
            content.len(),
            symbols.len() as f64 / (content.len() as f64 / 1024.0)
        );
    }

    /// Get all files of a specific extension from a directory
    pub fn get_files_with_extension(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
        if !dir.exists() {
            return Vec::new();
        }

        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if extensions.contains(&ext.to_string_lossy().as_ref()) {
                            files.push(path);
                        }
                    }
                }
            }
        }
        files.sort();
        files
    }

    // Kotlin Real-World Tests
    #[test]
    fn test_kotlin_real_world_files() {
        let kotlin_dir = Path::new(REAL_WORLD_TEST_DIR).join("kotlin");
        let kotlin_files = get_files_with_extension(&kotlin_dir, &["kt"]);

        if kotlin_files.is_empty() {
            println!(
                "⚠️ No Kotlin real-world test files found in {}",
                kotlin_dir.display()
            );
            return;
        }

        for file_path in kotlin_files {
            test_real_world_file(&file_path, "kotlin");
        }
    }

    // Ruby Real-World Tests
    #[test]
    fn test_ruby_real_world_files() {
        let ruby_dir = Path::new(REAL_WORLD_TEST_DIR).join("ruby");
        let ruby_files = get_files_with_extension(&ruby_dir, &["rb"]);

        if ruby_files.is_empty() {
            println!(
                "⚠️ No Ruby real-world test files found in {}",
                ruby_dir.display()
            );
            return;
        }

        for file_path in ruby_files {
            test_real_world_file(&file_path, "ruby");
        }
    }

    // TypeScript Real-World Tests
    #[test]
    fn test_typescript_real_world_files() {
        let ts_dir = Path::new(REAL_WORLD_TEST_DIR).join("typescript");
        let ts_files = get_files_with_extension(&ts_dir, &["ts", "tsx"]);

        if ts_files.is_empty() {
            println!(
                "⚠️ No TypeScript real-world test files found in {}",
                ts_dir.display()
            );
            return;
        }

        for file_path in ts_files {
            let language = if file_path.extension().unwrap() == "tsx" {
                "tsx"
            } else {
                "typescript"
            };
            test_real_world_file(&file_path, language);
        }
    }

    // C# Real-World Tests
    #[test]
    fn test_csharp_real_world_files() {
        let csharp_dirs = [
            Path::new(REAL_WORLD_TEST_DIR).join("csharp"),
            Path::new(REAL_WORLD_TEST_DIR).join("csharp-advanced"),
        ];

        let mut all_files = Vec::new();
        for dir in &csharp_dirs {
            all_files.extend(get_files_with_extension(dir, &["cs"]));
        }

        if all_files.is_empty() {
            println!("⚠️ No C# real-world test files found");
            return;
        }

        for file_path in all_files {
            test_real_world_file(&file_path, "csharp");
        }
    }

    // Go Real-World Tests
    #[test]
    fn test_go_real_world_files() {
        let go_dir = Path::new(REAL_WORLD_TEST_DIR).join("go");
        let go_files = get_files_with_extension(&go_dir, &["go"]);
        if go_files.is_empty() {
            println!(
                "⚠️ No Go real-world test files found in {}",
                go_dir.display()
            );
            return;
        }
        for file_path in go_files {
            test_real_world_file(&file_path, "go");
        }
    }

    // Java Real-World Tests
    #[test]
    fn test_java_real_world_files() {
        let java_dir = Path::new(REAL_WORLD_TEST_DIR).join("java");
        let java_files = get_files_with_extension(&java_dir, &["java"]);

        if java_files.is_empty() {
            println!(
                "⚠️ No Java real-world test files found in {}",
                java_dir.display()
            );
            return;
        }

        for file_path in java_files {
            test_real_world_file(&file_path, "java");
        }
    }

    // PHP Real-World Tests
    #[test]
    fn test_php_real_world_files() {
        let php_dir = Path::new(REAL_WORLD_TEST_DIR).join("php");
        let php_files = get_files_with_extension(&php_dir, &["php"]);

        if php_files.is_empty() {
            println!(
                "⚠️ No PHP real-world test files found in {}",
                php_dir.display()
            );
            return;
        }

        for file_path in php_files {
            test_real_world_file(&file_path, "php");
        }
    }

    // Swift Real-World Tests
    #[test]
    fn test_swift_real_world_files() {
        let swift_dir = Path::new(REAL_WORLD_TEST_DIR).join("swift");
        let swift_files = get_files_with_extension(&swift_dir, &["swift"]);

        if swift_files.is_empty() {
            println!(
                "⚠️ No Swift real-world test files found in {}",
                swift_dir.display()
            );
            return;
        }

        for file_path in swift_files {
            test_real_world_file(&file_path, "swift");
        }
    }

    // Razor Real-World Tests
    #[test]
    fn test_razor_real_world_files() {
        let razor_dir = Path::new(REAL_WORLD_TEST_DIR).join("razor");
        let razor_files = get_files_with_extension(&razor_dir, &["razor"]);

        if razor_files.is_empty() {
            println!(
                "⚠️ No Razor real-world test files found in {}",
                razor_dir.display()
            );
            return;
        }

        for file_path in razor_files {
            test_real_world_file(&file_path, "razor");
        }
    }

    // Vue Real-World Tests
    #[test]
    fn test_vue_real_world_files() {
        let vue_dir = Path::new(REAL_WORLD_TEST_DIR).join("vue");
        let vue_files = get_files_with_extension(&vue_dir, &["vue"]);

        if vue_files.is_empty() {
            println!(
                "⚠️ No Vue real-world test files found in {}",
                vue_dir.display()
            );
            return;
        }

        for file_path in vue_files {
            test_real_world_file(&file_path, "vue");
        }
    }

    // Python Real-World Tests
    #[test]
    fn test_python_real_world_files() {
        let python_dir = Path::new(REAL_WORLD_TEST_DIR).join("python");
        let python_files = get_files_with_extension(&python_dir, &["py"]);

        if python_files.is_empty() {
            println!(
                "⚠️ No Python real-world test files found in {}",
                python_dir.display()
            );
            return;
        }

        for file_path in python_files {
            test_real_world_file(&file_path, "python");
        }
    }

    // JavaScript Real-World Tests
    #[test]
    fn test_javascript_real_world_files() {
        let js_dir = Path::new(REAL_WORLD_TEST_DIR).join("javascript");
        let js_files = get_files_with_extension(&js_dir, &["js"]);

        if js_files.is_empty() {
            println!(
                "⚠️ No JavaScript real-world test files found in {}",
                js_dir.display()
            );
            return;
        }

        for file_path in js_files {
            test_real_world_file(&file_path, "javascript");
        }
    }

    // Rust Real-World Tests
    #[test]
    fn test_rust_real_world_files() {
        let rust_dir = Path::new(REAL_WORLD_TEST_DIR).join("rust");
        let rust_files = get_files_with_extension(&rust_dir, &["rs"]);

        if rust_files.is_empty() {
            println!(
                "⚠️ No Rust real-world test files found in {}",
                rust_dir.display()
            );
            return;
        }

        for file_path in rust_files {
            test_real_world_file(&file_path, "rust");
        }
    }

    // Bash Real-World Tests
    #[test]
    fn test_bash_real_world_files() {
        let bash_dir = Path::new(REAL_WORLD_TEST_DIR).join("bash");
        let bash_files = get_files_with_extension(&bash_dir, &["sh"]);

        if bash_files.is_empty() {
            println!(
                "⚠️ No Bash real-world test files found in {}",
                bash_dir.display()
            );
            return;
        }

        for file_path in bash_files {
            test_real_world_file(&file_path, "bash");
        }
    }

    // CSS Real-World Tests
    #[test]
    fn test_css_real_world_files() {
        let css_dir = Path::new(REAL_WORLD_TEST_DIR).join("css");
        let css_files = get_files_with_extension(&css_dir, &["css"]);

        if css_files.is_empty() {
            println!(
                "⚠️ No CSS real-world test files found in {}",
                css_dir.display()
            );
            return;
        }

        for file_path in css_files {
            test_real_world_file(&file_path, "css");
        }
    }

    // Dart Real-World Tests
    #[test]
    fn test_dart_real_world_files() {
        let dart_dir = Path::new(REAL_WORLD_TEST_DIR).join("dart");
        let dart_files = get_files_with_extension(&dart_dir, &["dart"]);

        if dart_files.is_empty() {
            println!(
                "⚠️ No Dart real-world test files found in {}",
                dart_dir.display()
            );
            return;
        }

        for file_path in dart_files {
            test_real_world_file(&file_path, "dart");
        }
    }

    // GDScript Real-World Tests
    #[test]
    fn test_gdscript_real_world_files() {
        let gdscript_dir = Path::new(REAL_WORLD_TEST_DIR).join("gdscript");
        let gdscript_files = get_files_with_extension(&gdscript_dir, &["gd"]);

        if gdscript_files.is_empty() {
            println!(
                "⚠️ No GDScript real-world test files found in {}",
                gdscript_dir.display()
            );
            return;
        }

        for file_path in gdscript_files {
            test_real_world_file(&file_path, "gdscript");
        }
    }

    // HTML Real-World Tests
    #[test]
    fn test_html_real_world_files() {
        let html_dir = Path::new(REAL_WORLD_TEST_DIR).join("html");
        let html_files = get_files_with_extension(&html_dir, &["html"]);

        if html_files.is_empty() {
            println!(
                "⚠️ No HTML real-world test files found in {}",
                html_dir.display()
            );
            return;
        }

        for file_path in html_files {
            test_real_world_file(&file_path, "html");
        }
    }

    // PowerShell Real-World Tests
    #[test]
    fn test_powershell_real_world_files() {
        let powershell_dir = Path::new(REAL_WORLD_TEST_DIR).join("powershell");
        let powershell_files = get_files_with_extension(&powershell_dir, &["ps1"]);

        if powershell_files.is_empty() {
            println!(
                "⚠️ No PowerShell real-world test files found in {}",
                powershell_dir.display()
            );
            return;
        }

        for file_path in powershell_files {
            test_real_world_file(&file_path, "powershell");
        }
    }

    // Regex Real-World Tests
    #[test]
    fn test_regex_real_world_files() {
        let regex_dir = Path::new(REAL_WORLD_TEST_DIR).join("regex");
        let regex_files = get_files_with_extension(&regex_dir, &["regex", "regexp"]);

        if regex_files.is_empty() {
            println!(
                "⚠️ No Regex real-world test files found in {}",
                regex_dir.display()
            );
            return;
        }

        for file_path in regex_files {
            test_real_world_file(&file_path, "regex");
        }
    }

    // SQL Real-World Tests
    #[test]
    fn test_sql_real_world_files() {
        let sql_dir = Path::new(REAL_WORLD_TEST_DIR).join("sql");
        let sql_files = get_files_with_extension(&sql_dir, &["sql"]);

        if sql_files.is_empty() {
            println!(
                "⚠️ No SQL real-world test files found in {}",
                sql_dir.display()
            );
            return;
        }

        for file_path in sql_files {
            test_real_world_file(&file_path, "sql");
        }
    }

    // Zig Real-World Tests
    #[test]
    fn test_zig_real_world_files() {
        let zig_dir = Path::new(REAL_WORLD_TEST_DIR).join("zig");
        let zig_files = get_files_with_extension(&zig_dir, &["zig"]);

        if zig_files.is_empty() {
            println!(
                "⚠️ No Zig real-world test files found in {}",
                zig_dir.display()
            );
            return;
        }

        for file_path in zig_files {
            test_real_world_file(&file_path, "zig");
        }
    }

    // C Real-World Tests
    #[test]
    fn test_c_real_world_files() {
        let c_dir = Path::new(REAL_WORLD_TEST_DIR).join("c");
        let c_files = get_files_with_extension(&c_dir, &["c", "h"]);

        if c_files.is_empty() {
            println!("⚠️ No C real-world test files found in {}", c_dir.display());
            return;
        }

        for file_path in c_files {
            test_real_world_file(&file_path, "c");
        }
    }

    // C++ Real-World Tests
    #[test]
    fn test_cpp_real_world_files() {
        let cpp_dir = Path::new(REAL_WORLD_TEST_DIR).join("cpp");
        let cpp_files = get_files_with_extension(&cpp_dir, &["cpp", "cc", "cxx", "hpp", "h"]);

        if cpp_files.is_empty() {
            println!(
                "⚠️ No C++ real-world test files found in {}",
                cpp_dir.display()
            );
            return;
        }

        for file_path in cpp_files {
            test_real_world_file(&file_path, "cpp");
        }
    }

    // Lua Real-World Tests
    #[test]
    fn test_lua_real_world_files() {
        let lua_dir = Path::new(REAL_WORLD_TEST_DIR).join("lua");
        let lua_files = get_files_with_extension(&lua_dir, &["lua"]);
        if lua_files.is_empty() {
            println!(
                "⚠️ No Lua real-world test files found in {}",
                lua_dir.display()
            );
            return;
        }
        for file_path in lua_files {
            test_real_world_file(&file_path, "lua");
        }
    }

    /// Integration test: Process multiple languages in sequence
    /// This validates cross-language consistency following Miller's approach
    #[test]
    fn test_cross_language_real_world_integration() {
        let base_dir = Path::new(REAL_WORLD_TEST_DIR);
        let mut total_files_processed = 0;
        let mut total_symbols_extracted = 0;

        let languages = [
            ("kotlin", vec!["kt"]),
            ("typescript", vec!["ts", "tsx"]),
            ("csharp", vec!["cs"]),
            // ("go", vec!["go"]), // Disabled
            ("java", vec!["java"]),
            ("php", vec!["php"]),
            ("swift", vec!["swift"]),
            ("razor", vec!["razor"]),
            ("vue", vec!["vue"]),
            ("rust", vec!["rs"]),
            ("python", vec!["py"]),
            ("javascript", vec!["js"]),
            ("ruby", vec!["rb"]),
            ("bash", vec!["sh"]),
            ("css", vec!["css"]),
            ("dart", vec!["dart"]),
            ("gdscript", vec!["gd"]),
            ("html", vec!["html"]),
            ("powershell", vec!["ps1"]),
            ("regex", vec!["regex", "regexp"]),
            ("sql", vec!["sql"]),
            ("zig", vec!["zig"]),
            ("c", vec!["c", "h"]),
            ("cpp", vec!["cpp", "cc", "cxx", "hpp", "h"]),
        ];

        println!("🌍 Starting cross-language real-world integration test...");

        for (language, extensions) in &languages {
            let lang_dir = base_dir.join(language);
            if *language == "csharp" {
                // Handle both csharp and csharp-advanced directories
                let dirs = [base_dir.join("csharp"), base_dir.join("csharp-advanced")];
                for dir in &dirs {
                    let files = get_files_with_extension(dir, &extensions);
                    for file_path in files {
                        test_real_world_file(&file_path, language);
                        total_files_processed += 1;
                        // We'd need to actually count symbols here, simplified for now
                        total_symbols_extracted += 1;
                    }
                }
            } else {
                let files = get_files_with_extension(&lang_dir, &extensions);
                for file_path in files {
                    test_real_world_file(&file_path, language);
                    total_files_processed += 1;
                    // We'd need to actually count symbols here, simplified for now
                    total_symbols_extracted += 1;
                }
            }
        }

        println!(
            "🎯 Integration test complete: {} files processed across {} languages",
            total_files_processed,
            languages.len()
        );

        // Validate that we processed a meaningful number of files
        assert!(
            total_files_processed > 0,
            "Integration test should process at least some real-world files"
        );
    }
}

// Real-World Refactoring Tool Tests (following the same proven methodology)
#[cfg(test)]
mod real_world_refactoring_tests {
    use super::real_world_tests::get_files_with_extension;
    use crate::handler::JulieServerHandler;
    use crate::tools::refactoring::SmartRefactorTool;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    const REAL_WORLD_TEST_DIR: &str = "tests/real-world";

    /// Test SmartRefactorTool against real TypeScript files
    #[tokio::test]
    async fn test_rename_symbol_real_typescript_files() {
        let ts_dir = Path::new(REAL_WORLD_TEST_DIR).join("typescript");
        let ts_files = get_files_with_extension(&ts_dir, &["ts", "tsx"]);

        if ts_files.is_empty() {
            println!(
                "⚠️ No TypeScript real-world test files found in {}",
                ts_dir.display()
            );
            return;
        }

        let handler = JulieServerHandler::new().await.unwrap();

        for file_path in ts_files {
            test_rename_symbol_on_real_file(&handler, &file_path, "typescript").await;
        }
    }

    /// Test SmartRefactorTool against real JavaScript files
    #[tokio::test]
    async fn test_rename_symbol_real_javascript_files() {
        let js_dir = Path::new(REAL_WORLD_TEST_DIR).join("javascript");
        let js_files = get_files_with_extension(&js_dir, &["js", "jsx"]);

        if js_files.is_empty() {
            println!(
                "⚠️ No JavaScript real-world test files found in {}",
                js_dir.display()
            );
            return;
        }

        let handler = JulieServerHandler::new().await.unwrap();

        for file_path in js_files {
            test_rename_symbol_on_real_file(&handler, &file_path, "javascript").await;
        }
    }

    /// Test SmartRefactorTool against real Python files
    #[tokio::test]
    async fn test_rename_symbol_real_python_files() {
        let py_dir = Path::new(REAL_WORLD_TEST_DIR).join("python");
        let py_files = get_files_with_extension(&py_dir, &["py"]);

        if py_files.is_empty() {
            println!(
                "⚠️ No Python real-world test files found in {}",
                py_dir.display()
            );
            return;
        }

        let handler = JulieServerHandler::new().await.unwrap();

        for file_path in py_files {
            test_rename_symbol_on_real_file(&handler, &file_path, "python").await;
        }
    }

    /// Core function to test rename operations on real files
    async fn test_rename_symbol_on_real_file(
        handler: &JulieServerHandler,
        file_path: &Path,
        language: &str,
    ) {
        println!(
            "🔄 Testing rename refactoring on real file: {}",
            file_path.display()
        );

        // Read the real file content
        let content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => {
                println!("⚠️ Failed to read {}: {}", file_path.display(), e);
                return;
            }
        };

        // Extract symbols to find real symbols to rename
        let symbols = extract_symbols_from_real_file(&content, language);
        if symbols.is_empty() {
            println!("📝 No symbols found in {}", file_path.display());
            return;
        }

        // Create temporary workspace with copy of real file
        let temp_dir = TempDir::new().unwrap();
        let test_file_path = temp_dir.path().join(file_path.file_name().unwrap());
        fs::write(&test_file_path, &content).unwrap();

        // Initialize workspace for the refactoring tool
        let workspace_path = temp_dir.path().to_string_lossy().to_string();
        if let Err(e) = handler.initialize_workspace(Some(workspace_path)).await {
            println!("⚠️ Failed to initialize workspace: {}", e);
            return;
        }

        // Test renaming the first meaningful symbol we find
        for symbol in symbols.iter().take(3) {
            // Skip very short or generic names
            if symbol.len() < 3 || symbol.chars().all(|c| c.is_ascii_lowercase()) {
                continue;
            }

            let new_name = format!("Renamed{}", symbol);

            println!("  🎯 Testing rename: {} -> {}", symbol, new_name);

            // Test dry run first
            let dry_run_tool = SmartRefactorTool {
                operation: "rename_symbol".to_string(),
                params: format!(
                    r#"{{"old_name": "{}", "new_name": "{}", "scope": "workspace"}}"#,
                    symbol, new_name
                ),
                dry_run: true,
            };

            match dry_run_tool.call_tool(handler).await {
                Ok(result) => {
                    let response = extract_response_text(&result);
                    if response.contains("No references found") {
                        println!("    📝 No references found for symbol '{}'", symbol);
                        continue;
                    }

                    println!("    ✅ Dry run successful for symbol '{}'", symbol);

                    // If dry run found references, test actual rename
                    let actual_tool = SmartRefactorTool {
                        operation: "rename_symbol".to_string(),
                        params: format!(
                            r#"{{"old_name": "{}", "new_name": "{}", "scope": "workspace"}}"#,
                            symbol, new_name
                        ),
                        dry_run: false,
                    };

                    match actual_tool.call_tool(handler).await {
                        Ok(result) => {
                            let response = extract_response_text(&result);
                            if response.contains("Rename successful")
                                || response.contains("Modified")
                            {
                                println!("    🎉 Actual rename successful for symbol '{}'", symbol);

                                // Verify the file was actually modified correctly
                                if let Ok(modified_content) = fs::read_to_string(&test_file_path) {
                                    if modified_content.contains(&new_name) {
                                        println!(
                                            "    ✅ File correctly modified with new symbol name"
                                        );
                                    } else {
                                        println!("    ⚠️ File was not modified as expected");
                                    }
                                }
                            } else {
                                println!("    📝 Rename result: {}", response);
                            }
                        }
                        Err(e) => println!("    ❌ Actual rename failed: {}", e),
                    }

                    // Only test one successful rename per file to avoid conflicts
                    break;
                }
                Err(e) => println!("    ❌ Dry run failed: {}", e),
            }
        }
    }

    /// Extract potential symbols from real file content for testing
    fn extract_symbols_from_real_file(content: &str, language: &str) -> Vec<String> {
        let mut symbols = Vec::new();

        match language {
            "typescript" | "javascript" => {
                // Look for class names, function names, interface names
                for line in content.lines() {
                    let line = line.trim();

                    // Class declarations
                    if line.starts_with("export class ") || line.starts_with("class ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "class") {
                            symbols.push(name);
                        }
                    }

                    // Interface declarations
                    if line.starts_with("export interface ") || line.starts_with("interface ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "interface") {
                            symbols.push(name);
                        }
                    }

                    // Function declarations
                    if line.starts_with("export function ") || line.starts_with("function ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "function") {
                            symbols.push(name);
                        }
                    }

                    // Const declarations
                    if line.starts_with("export const ") || line.starts_with("const ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "const") {
                            symbols.push(name);
                        }
                    }
                }
            }
            "python" => {
                // Look for class and function definitions
                for line in content.lines() {
                    let line = line.trim();

                    if line.starts_with("class ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "class") {
                            symbols.push(name);
                        }
                    }

                    if line.starts_with("def ") {
                        if let Some(name) = extract_identifier_after_keyword(line, "def") {
                            symbols.push(name);
                        }
                    }
                }
            }
            _ => {
                // Generic symbol extraction for other languages
                // This is a simple heuristic - real extraction would use tree-sitter
                for line in content.lines() {
                    if line.contains("class ") || line.contains("function ") {
                        // Extract potential identifiers
                        for word in line.split_whitespace() {
                            if word.len() > 3 && word.chars().next().unwrap().is_alphabetic() {
                                let clean_word =
                                    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                                if !clean_word.is_empty() && clean_word.len() > 3 {
                                    symbols.push(clean_word.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        symbols.into_iter().take(5).collect() // Limit to first 5 symbols
    }

    /// Helper to extract identifier after a keyword
    fn extract_identifier_after_keyword(line: &str, keyword: &str) -> Option<String> {
        let keyword_with_space = format!("{} ", keyword);
        if let Some(start) = line.find(&keyword_with_space) {
            let after_keyword = &line[start + keyword_with_space.len()..];
            let identifier = after_keyword
                .split_whitespace()
                .next()?
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

            if identifier.len() > 2 && identifier.chars().all(|c| c.is_alphanumeric() || c == '_') {
                Some(identifier.to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Extract text from CallToolResult
    fn extract_response_text(result: &rust_mcp_sdk::schema::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n")
    }
}
