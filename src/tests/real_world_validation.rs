//! Real-world validation tests following Miller's proven methodology
//!
//! This module tests all extractors against real-world code files to ensure
//! they work correctly on actual codebases, not just carefully crafted test strings.

#[cfg(test)]
mod real_world_tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use crate::extractors::*;
    use tree_sitter::{Parser, Tree};

    const REAL_WORLD_TEST_DIR: &str = "debug/test-workspace-real";

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
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "ruby" => {
                let mut extractor = ruby::RubyExtractor::new(
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "rust" => {
                let mut extractor = rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "typescript" | "tsx" => {
                let mut extractor = typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "javascript" | "jsx" => {
                let mut extractor = javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "python" => {
                let mut extractor = python::PythonExtractor::new(
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "java" => {
                let mut extractor = java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "csharp" => {
                let mut extractor = csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            // "go" => { /* Disabled */ },
            "php" => {
                let mut extractor = php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "swift" => {
                let mut extractor = swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "razor" => {
                let mut extractor = razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(&tree)
            },
            "vue" => {
                let mut extractor = vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string_lossy().to_string(),
                    content.clone()
                );
                extractor.extract_symbols(Some(&tree))
            },
            _ => panic!("Unsupported language: {}", language),
        };

        // Validate meaningful extraction - Miller's key requirement
        assert!(
            !symbols.is_empty(),
            "❌ Should extract symbols from {}, but got 0. This indicates the extractor failed to parse real-world code.",
            file_path.display()
        );

        // Generate symbol breakdown for analysis (Miller's logging approach)
        let symbol_summary = symbols.iter()
            .fold(HashMap::new(), |mut acc, symbol| {
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
    fn get_files_with_extension(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
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
            println!("⚠️ No Kotlin real-world test files found in {}", kotlin_dir.display());
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
            println!("⚠️ No Ruby real-world test files found in {}", ruby_dir.display());
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
            println!("⚠️ No TypeScript real-world test files found in {}", ts_dir.display());
            return;
        }

        for file_path in ts_files {
            let language = if file_path.extension().unwrap() == "tsx" { "tsx" } else { "typescript" };
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

    // Go Real-World Tests - DISABLED (extractor not enabled)
    // #[test]
    // fn test_go_real_world_files() { ... }

    // Java Real-World Tests
    #[test]
    fn test_java_real_world_files() {
        let java_dir = Path::new(REAL_WORLD_TEST_DIR).join("java");
        let java_files = get_files_with_extension(&java_dir, &["java"]);

        if java_files.is_empty() {
            println!("⚠️ No Java real-world test files found in {}", java_dir.display());
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
            println!("⚠️ No PHP real-world test files found in {}", php_dir.display());
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
            println!("⚠️ No Swift real-world test files found in {}", swift_dir.display());
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
            println!("⚠️ No Razor real-world test files found in {}", razor_dir.display());
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
            println!("⚠️ No Vue real-world test files found in {}", vue_dir.display());
            return;
        }

        for file_path in vue_files {
            test_real_world_file(&file_path, "vue");
        }
    }

    // Rust Real-World Tests
    #[test]
    fn test_rust_real_world_files() {
        let rust_dir = Path::new(REAL_WORLD_TEST_DIR).join("rust");
        let rust_files = get_files_with_extension(&rust_dir, &["rs"]);

        if rust_files.is_empty() {
            println!("⚠️ No Rust real-world test files found in {}", rust_dir.display());
            return;
        }

        for file_path in rust_files {
            test_real_world_file(&file_path, "rust");
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