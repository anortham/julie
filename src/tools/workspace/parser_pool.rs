use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;
use tree_sitter::{Language, Parser};

//******************//
// Parser Pool for Performance Optimization //
//******************//

/// PERFORMANCE OPTIMIZATION: Reusable parser pool to avoid expensive parser creation per file
/// This provides 10-50x speedup by reusing tree-sitter parsers across files of the same language
pub struct LanguageParserPool {
    parsers: HashMap<String, Parser>,
}

impl LanguageParserPool {
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Get or create a parser for the specified language
    pub fn get_parser(&mut self, language: &str) -> Result<&mut Parser> {
        if !self.parsers.contains_key(language) {
            let mut parser = Parser::new();
            let tree_sitter_language = Self::get_tree_sitter_language_static(language)?;
            parser.set_language(&tree_sitter_language).map_err(|e| {
                anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
            })?;
            self.parsers.insert(language.to_string(), parser);
            debug!("🔧 Created new parser for language: {}", language);
        }
        Ok(self.parsers.get_mut(language).unwrap())
    }

    /// Static version of get_tree_sitter_language for parser pool
    fn get_tree_sitter_language_static(language: &str) -> Result<Language> {
        match language {
            "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
            "typescript" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "javascript" => Ok(tree_sitter_javascript::LANGUAGE.into()),
            "python" => Ok(tree_sitter_python::LANGUAGE.into()),
            "java" => Ok(tree_sitter_java::LANGUAGE.into()),
            "csharp" => Ok(tree_sitter_c_sharp::LANGUAGE.into()),
            "ruby" => Ok(tree_sitter_ruby::LANGUAGE.into()),
            "swift" => Ok(tree_sitter_swift::LANGUAGE.into()),
            "kotlin" => Ok(tree_sitter_kotlin_ng::LANGUAGE.into()),
            "go" => Ok(tree_sitter_go::LANGUAGE.into()),
            "c" => Ok(tree_sitter_c::LANGUAGE.into()),
            "cpp" => Ok(tree_sitter_cpp::LANGUAGE.into()),
            "lua" => Ok(tree_sitter_lua::LANGUAGE.into()),
            "sql" => Ok(tree_sitter_sequel::LANGUAGE.into()),
            "html" => Ok(tree_sitter_html::LANGUAGE.into()),
            "css" => Ok(tree_sitter_css::LANGUAGE.into()),
            "vue" => Ok(tree_sitter_javascript::LANGUAGE.into()), // Vue SFCs use JS parser for script sections
            "razor" => Ok(tree_sitter_razor::LANGUAGE.into()),
            "bash" => Ok(tree_sitter_bash::LANGUAGE.into()),
            "powershell" => Ok(tree_sitter_powershell::LANGUAGE.into()),
            "gdscript" => Ok(tree_sitter_gdscript::LANGUAGE.into()),
            "zig" => Ok(tree_sitter_zig::LANGUAGE.into()),
            "dart" => Ok(harper_tree_sitter_dart::LANGUAGE.into()),
            "regex" => Ok(tree_sitter_regex::LANGUAGE.into()),
            _ => Err(anyhow::anyhow!("Unsupported language: {}", language)),
        }
    }
}
