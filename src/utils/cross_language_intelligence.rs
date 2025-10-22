// Julie Intelligence Layer - Cross-Language Code Understanding
//!
//! This module implements Julie's core differentiation: intelligent cross-language
//! code navigation that goes beyond simple string matching. This is what makes Julie
//! more than just another code search tool.
//!
//! ## Architecture Philosophy
//!
//! Julie leverages three pillars of intelligence working together:
//!
//! 1. **Structural Intelligence** (Tree-sitter)
//!    - Understand code structure across 26 languages
//!    - Know that a Python class method ≈ C# instance method ≈ Rust impl function
//!    - Extract semantic meaning, not just text
//!
//! 2. **Fast Intelligence** (CASCADE: SQLite FTS5)
//!    - Naming convention variants (snake_case, camelCase, PascalCase)
//!    - Pattern matching across language conventions
//!    - Sub-10ms indexed searches
//!
//! 3. **Semantic Intelligence** (HNSW Embeddings)
//!    - Conceptual similarity (getUserData ≈ fetchUser ≈ loadUserProfile)
//!    - Find similar patterns even with different names
//!    - <50ms similarity searches
//!
//! ## The Problem We Solve
//!
//! Traditional code search tools fail in polyglot codebases:
//! - Search "getUserData" → miss Python's "get_user_data"
//! - Search "UserRepository" → miss Java's "UserDAO" (same concept!)
//! - Can't find where React calls C# API calls SQL (cross-language tracing)
//!
//! Julie solves this by understanding CODE, not just TEXT.
//!
//! ## Usage Example
//!
//! ```rust
//! use julie::utils::cross_language_intelligence::*;
//!
//! // Generate naming variants for cross-language search
//! let variants = generate_naming_variants("getUserData");
//! assert!(variants.contains(&"getUserData".to_string()));
//! assert!(variants.contains(&"get_user_data".to_string()));
//! assert!(variants.contains(&"GetUserData".to_string()));
//! // Returns: ["getUserData", "get_user_data", "GetUserData", "get-user-data", "GET_USER_DATA"]
//! ```
//!
//! For finding cross-language matches with search and embedding engines:
//! ```rust,ignore
//! use julie::utils::cross_language_intelligence::*;
//!
//! // Find symbols across languages (requires search and embedding engines)
//! let intelligence = CrossLanguageIntelligence::new();
//! let matches = intelligence.find_cross_language_matches(
//!     "getUserData",
//!     &search_engine,
//!     &embedding_engine,
//! ).await?;
//! // Finds: Python get_user_data(), JS getUserData(), C# GetUserData(),
//! //        and semantically similar: fetchUser(), loadUserProfile()
//! ```
//!
//! ## Design Principles
//!
//! 1. **Composable**: Each intelligence layer works independently
//! 2. **Fast-First**: Try cheap operations before expensive ones
//! 3. **Extensible**: Easy to add new language conventions or patterns
//! 4. **Transparent**: Log what intelligence is being applied
//! 5. **Tunable**: Configurable thresholds and strategies

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

use crate::extractors::SymbolKind;

//****************************//
// Naming Convention Variants //
//****************************//

/// Naming convention styles used across programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NamingConvention {
    /// snake_case: Python, Ruby, Rust (variables/functions)
    SnakeCase,
    /// camelCase: JavaScript, Java, TypeScript
    CamelCase,
    /// PascalCase: C#, Go, Java (classes)
    PascalCase,
    /// kebab-case: CSS, HTML attributes, some CLIs
    KebabCase,
    /// SCREAMING_SNAKE_CASE: Constants in most languages
    ScreamingSnakeCase,
}

/// Generate all naming convention variants of a symbol name
///
/// This is the foundation of Julie's cross-language naming intelligence.
/// Given a symbol name in any convention, generates all possible variants.
///
/// # Examples
///
/// ```
/// use julie::utils::cross_language_intelligence::generate_naming_variants;
///
/// let variants = generate_naming_variants("getUserData");
/// assert!(variants.contains(&"getUserData".to_string()));
/// assert!(variants.contains(&"get_user_data".to_string()));
/// // Returns: ["getUserData", "get_user_data", "GetUserData", "get-user-data", "GET_USER_DATA"]
/// ```
///
/// # Performance
///
/// O(n) where n is the length of the input string. Extremely fast.
/// These variants are then searched using indexed queries (SQLite FTS5).
pub fn generate_naming_variants(symbol: &str) -> Vec<String> {
    let mut variants = Vec::with_capacity(5);

    // Always include the original
    variants.push(symbol.to_string());

    // Generate each convention variant
    let snake = to_snake_case(symbol);
    let camel = to_camel_case(symbol);
    let pascal = to_pascal_case(symbol);
    let kebab = to_kebab_case(symbol);
    let screaming = to_screaming_snake_case(symbol);

    // Only add unique variants (avoid duplicates)
    for variant in [snake, camel, pascal, kebab, screaming] {
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }

    debug!(
        "🔄 Generated {} naming variants for '{}': {:?}",
        variants.len(),
        symbol,
        variants
    );

    variants
}

/// Convert string to snake_case
///
/// Used by: Python, Ruby, Rust (functions, variables, modules)
///
/// # Algorithm
/// - Split on uppercase letters and special characters
/// - Handle acronyms correctly (XMLParser → xml_parser)
/// - Join with underscores
/// - Lowercase everything
///
/// # Examples
/// - "getUserData" → "get_user_data"
/// - "HTTPServer" → "http_server"
/// - "parseXMLFile" → "parse_xml_file"
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        // Skip existing underscores, hyphens (collapse multiple into one)
        if ch == '_' || ch == '-' {
            if !result.is_empty() && !result.ends_with('_') {
                result.push('_');
            }
            continue;
        }

        if ch.is_uppercase() {
            // Add underscore before uppercase letter if:
            // 1. Not at the start AND
            // 2. (Previous char was lowercase OR next char is lowercase)
            let prev_is_lower =
                i > 0 && chars.get(i - 1).map(|c| c.is_lowercase()).unwrap_or(false);
            let next_is_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);

            if i > 0 && (prev_is_lower || next_is_lower) && !result.ends_with('_') {
                result.push('_');
            }

            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

/// Convert string to camelCase
///
/// Used by: JavaScript, TypeScript, Java (variables, methods)
///
/// # Algorithm
/// - Remove underscores/hyphens and capitalize next letter
/// - Keep first letter lowercase
///
/// # Examples
/// - "get_user_data" → "getUserData"
/// - "http-server" → "httpServer"
/// - "ParseXMLFile" → "parseXMLFile"
pub fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    let mut first_char = true;

    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
            first_char = false;
        } else if first_char {
            result.push(ch.to_lowercase().next().unwrap());
            first_char = false;
        } else {
            result.push(ch);
        }
    }

    result
}

/// Convert string to PascalCase
///
/// Used by: C#, Go, Java (classes, interfaces), Rust (types, traits)
///
/// # Algorithm
/// - Same as camelCase but first letter capitalized
///
/// # Examples
/// - "get_user_data" → "GetUserData"
/// - "http-server" → "HttpServer"
/// - "parseXMLFile" → "ParseXMLFile"
pub fn to_pascal_case(s: &str) -> String {
    let camel = to_camel_case(s);
    if camel.is_empty() {
        return camel;
    }

    let mut chars = camel.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Convert string to kebab-case
///
/// Used by: CSS, HTML, CLI arguments, URLs
///
/// # Examples
/// - "getUserData" → "get-user-data"
/// - "HTTPServer" → "http-server"
pub fn to_kebab_case(s: &str) -> String {
    to_snake_case(s).replace('_', "-")
}

/// Convert string to SCREAMING_SNAKE_CASE
///
/// Used by: Constants in most languages (JavaScript, Python, Java, C#)
///
/// # Examples
/// - "getUserData" → "GET_USER_DATA"
/// - "maxConnections" → "MAX_CONNECTIONS"
pub fn to_screaming_snake_case(s: &str) -> String {
    to_snake_case(s).to_uppercase()
}

//*****************************//
// Symbol Kind Equivalence     //
//*****************************//

/// Cross-language symbol kind equivalence
///
/// Different languages use different terminology for similar concepts:
/// - Python "class" ≈ Rust "struct" ≈ C# "class" ≈ TypeScript "interface"
/// - Java "interface" ≈ Rust "trait" ≈ TypeScript "interface"
///
/// This mapping enables intelligent cross-language navigation.
#[derive(Debug, Clone)]
pub struct SymbolKindEquivalence {
    equivalence_groups: HashMap<SymbolKind, Vec<SymbolKind>>,
}

impl SymbolKindEquivalence {
    /// Create new symbol kind equivalence mapper with default mappings
    pub fn new() -> Self {
        let mut equivalence_groups = HashMap::new();

        // Group 1: Class-like types (data containers with methods)
        let class_group = vec![SymbolKind::Class, SymbolKind::Struct, SymbolKind::Interface];
        for kind in &class_group {
            equivalence_groups.insert(kind.clone(), class_group.clone());
        }

        // Group 2: Function-like callables
        let function_group = vec![SymbolKind::Function, SymbolKind::Method];
        for kind in &function_group {
            equivalence_groups.insert(kind.clone(), function_group.clone());
        }

        // Group 3: Module/namespace organization
        let module_group = vec![SymbolKind::Module, SymbolKind::Namespace];
        for kind in &module_group {
            equivalence_groups.insert(kind.clone(), module_group.clone());
        }

        // Group 4: Type definitions
        let type_group = vec![SymbolKind::Type, SymbolKind::Interface];
        for kind in &type_group {
            equivalence_groups.insert(kind.clone(), type_group.clone());
        }

        Self { equivalence_groups }
    }

    /// Check if two symbol kinds are equivalent across languages
    ///
    /// # Examples
    ///
    /// ```
    /// use julie::utils::cross_language_intelligence::SymbolKindEquivalence;
    /// use julie::extractors::base::SymbolKind;
    ///
    /// let eq = SymbolKindEquivalence::new();
    /// assert!(eq.are_equivalent(SymbolKind::Class, SymbolKind::Struct)); // true
    /// assert!(eq.are_equivalent(SymbolKind::Function, SymbolKind::Method)); // true
    /// assert!(!eq.are_equivalent(SymbolKind::Class, SymbolKind::Function)); // false
    /// ```
    pub fn are_equivalent(&self, kind1: SymbolKind, kind2: SymbolKind) -> bool {
        if kind1 == kind2 {
            return true;
        }

        if let Some(equiv_group) = self.equivalence_groups.get(&kind1) {
            equiv_group.contains(&kind2)
        } else {
            false
        }
    }

    /// Get all equivalent symbol kinds for a given kind
    ///
    /// # Examples
    ///
    /// ```
    /// use julie::utils::cross_language_intelligence::SymbolKindEquivalence;
    /// use julie::extractors::base::SymbolKind;
    ///
    /// let eq = SymbolKindEquivalence::new();
    /// let equivalents = eq.get_equivalents(SymbolKind::Class);
    /// assert!(equivalents.contains(&SymbolKind::Class));
    /// assert!(equivalents.contains(&SymbolKind::Struct));
    /// // Returns: [Class, Struct, Interface]
    /// ```
    pub fn get_equivalents(&self, kind: SymbolKind) -> Vec<SymbolKind> {
        self.equivalence_groups
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| vec![kind])
    }
}

impl Default for SymbolKindEquivalence {
    fn default() -> Self {
        Self::new()
    }
}

//*****************************//
// Intelligence Configuration  //
//*****************************//

/// Configuration for cross-language intelligence strategies
///
/// Allows tuning the balance between precision and recall, speed vs accuracy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceConfig {
    /// Enable naming convention variants (fast, high precision)
    pub enable_naming_variants: bool,

    /// Enable symbol kind equivalence (medium precision, low cost)
    pub enable_kind_equivalence: bool,

    /// Enable semantic similarity (slower, high recall, lower precision)
    pub enable_semantic_similarity: bool,

    /// Minimum similarity score for semantic matches (0.0 to 1.0)
    /// Higher = more precise but fewer results
    /// Recommended: 0.7 for definitions, 0.85 for references
    pub semantic_similarity_threshold: f32,

    /// Maximum number of variants to search per symbol
    /// Prevents explosion with very long symbol names
    pub max_variants: usize,

    /// Enable debug logging for intelligence decisions
    pub debug_logging: bool,
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            enable_naming_variants: true,
            enable_kind_equivalence: true,
            enable_semantic_similarity: true,
            semantic_similarity_threshold: 0.7,
            max_variants: 10,
            debug_logging: true,
        }
    }
}

impl IntelligenceConfig {
    /// Strict configuration for reference finding (high precision)
    ///
    /// Disables semantic similarity to avoid false positives.
    /// Only finds exact matches and naming variants.
    pub fn strict() -> Self {
        Self {
            enable_naming_variants: true,
            enable_kind_equivalence: false,
            enable_semantic_similarity: false,
            semantic_similarity_threshold: 0.9,
            max_variants: 5,
            debug_logging: true,
        }
    }

    /// Relaxed configuration for exploration (high recall)
    ///
    /// Enables all intelligence features for maximum discovery.
    pub fn relaxed() -> Self {
        Self {
            enable_naming_variants: true,
            enable_kind_equivalence: true,
            enable_semantic_similarity: true,
            semantic_similarity_threshold: 0.6,
            max_variants: 15,
            debug_logging: true,
        }
    }
}
