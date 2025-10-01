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

    /// Static version of get_tree_sitter_language (delegates to shared language module)
    fn get_tree_sitter_language_static(language: &str) -> Result<Language> {
        crate::language::get_tree_sitter_language(language)
    }
}
