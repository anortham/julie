//! Query Expansion for Multi-Word Search
//!
//! Converts multi-word queries like "user service" into multiple variants:
//! - CamelCase: "UserService"
//! - snake_case: "user_service"
//! - Wildcards: "user* AND service*"
//! - OR queries: "(user OR service)"
//! - Fuzzy: "user~1 service~1"
//!
//! This solves the #1 agent pain point: multi-word queries returning zero results.

use crate::utils::cross_language_intelligence;

/// Convert multi-word query to CamelCase
/// "user service" → "UserService"
/// "get user data" → "GetUserData"
pub fn to_camelcase(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Convert multi-word query to snake_case
/// "user service" → "user_service"
/// "get user data" → "get_user_data"
pub fn to_snake_case(query: &str) -> String {
    query.split_whitespace().collect::<Vec<&str>>().join("_")
}

/// Convert to lowercase camelCase (first word lowercase)
/// "user service" → "userService"
/// "get user data" → "getUserData"
pub fn to_lowercase_camelcase(query: &str) -> String {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let mut result = words[0].to_lowercase();
    for word in &words[1..] {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.push_str(&first.to_uppercase().chain(chars).collect::<String>());
        }
    }
    result
}

/// Convert to wildcard query with implicit AND (FTS5 compatible)
/// "user service" → "user* service*" (space = implicit AND)
pub fn to_wildcard_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("{}*", word))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Convert to OR query
/// "user service" → "(user OR service)"
pub fn to_or_query(query: &str) -> String {
    format!(
        "({})",
        query.split_whitespace().collect::<Vec<&str>>().join(" OR ")
    )
}

/// Convert to fuzzy query
/// "user service" → "user~1 service~1"
pub fn to_fuzzy_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("{}~1", word))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Expand query into all possible variants for Google-style search
/// Handles complex agent queries like "user auth controller post"
/// Returns a vector of query strings to try in sequence (most specific → most permissive)
pub fn expand_query(query: &str) -> Vec<String> {
    let mut variants = Vec::new();

    // 1. Original query (exact phrase match first)
    variants.push(query.to_string());

    // 2. For multi-word queries, try naming convention variants
    if query.contains(' ') {
        // CamelCase: "user auth post" → "UserAuthPost"
        let camelcase = to_camelcase(query);
        if camelcase != query {
            variants.push(camelcase);
        }

        // snake_case: "user auth post" → "user_auth_post"
        let snake_case = to_snake_case(query);
        if snake_case != query {
            variants.push(snake_case);
        }

        // camelCase: "user auth post" → "userAuthPost"
        let lowercase_camel = to_lowercase_camelcase(query);
        if lowercase_camel != query {
            variants.push(lowercase_camel.clone());
        }

        // 3. FTS5 implicit AND query (all terms must be present)
        // "user auth controller post" → "user auth controller post" (space = implicit AND)
        // This finds symbols containing ALL terms anywhere in the symbol
        // Note: Already added as variants[0] on line 92, so skip to avoid duplicate

        // 4. Wildcard query with implicit AND (more permissive matching)
        // "user auth post" → "user* auth* post*" (space = implicit AND)
        let wildcard_implicit_and = query
            .split_whitespace()
            .map(|word| format!("{}*", word))
            .collect::<Vec<String>>()
            .join(" ");
        variants.push(wildcard_implicit_and);

        // 5. OR query: Find symbols matching ANY term (most permissive)
        // "user auth post" → "(user OR auth OR post)"
        // Use this as last resort to ensure we return SOMETHING
        let or_query = to_or_query(query);
        if or_query != query {
            variants.push(or_query);
        }
    } else {
        // Single word queries

        // If it's CamelCase/PascalCase, generate naming convention variants
        // For single-word CamelCase, try EXACT match first (for structs like "SymbolDatabase")
        // Then try snake_case (for functions like "process_files_optimized")
        if query.chars().any(|c| c.is_uppercase()) {
            // variants[0] is already the exact query from line 90 - keep it!
            // "SymbolDatabase", "ProcessFilesOptimized", etc.

            // Add snake_case variant as fallback
            // "SymbolDatabase" → "symbol_database"
            // "ProcessFilesOptimized" → "process_files_optimized"
            let snake = cross_language_intelligence::to_snake_case(query);
            let snake_is_different = snake != query;

            // Add lowercase camelCase variant
            // "SymbolDatabase" → "symbolDatabase"
            // "ProcessFilesOptimized" → "processFilesOptimized"
            let lower_camel = to_lowercase_camelcase(query);
            let lower_camel_is_different = lower_camel != query && lower_camel != snake;

            // Now push them
            if snake_is_different {
                variants.push(snake);
            }
            if lower_camel_is_different {
                variants.push(lower_camel);
            }

            // Finally wildcards for partial matches
            variants.push(format!("{}*", query));
        } else {
            // Pure lowercase single word - just wildcards and fuzzy
            variants.push(format!("{}*", query));
            variants.push(format!("{}~1", query));
        }
    }

    variants
}

/// Expand query for Google-style "any term" matching
/// Returns OR-based queries for maximum recall
pub fn expand_query_permissive(query: &str) -> Vec<String> {
    let mut variants = Vec::new();

    // Start with most permissive: OR query
    variants.push(to_or_query(query));

    // Add fuzzy OR for typo tolerance
    let terms: Vec<&str> = query.split_whitespace().collect();
    let fuzzy_or = format!(
        "({})",
        terms
            .iter()
            .map(|term| format!("{}~1", term))
            .collect::<Vec<String>>()
            .join(" OR ")
    );
    variants.push(fuzzy_or);

    variants
}

/// Check if a symbol's name is actually relevant to the search query
///
/// Prevents spurious matches where the query appears only in comments/docs
/// but the symbol name is completely unrelated.
///
/// # Arguments
/// * `query` - Original user query (e.g., "ProcessFilesOptimized")
/// * `symbol_name` - Actual symbol name from results (e.g., "expand_query" or "process_files_optimized")
/// * `variant` - Query variant that produced this match (e.g., "ProcessFilesOptimized" or "process_files_optimized")
///
/// # Returns
/// * `true` - Symbol name is relevant (matches query intent)
/// * `false` - Symbol name is NOT relevant (spurious match via comments)
///
/// # Examples
/// ```
/// use julie::utils::query_expansion::is_symbol_name_relevant;
///
/// // Exact match (different casing)
/// assert!(is_symbol_name_relevant(
///     "ProcessFilesOptimized",
///     "process_files_optimized",
///     "process_files_optimized"
/// ));
///
/// // Spurious match via comment
/// assert!(!is_symbol_name_relevant(
///     "ProcessFilesOptimized",
///     "expand_query",
///     "ProcessFilesOptimized"
/// ));
/// ```
pub fn is_symbol_name_relevant(query: &str, symbol_name: &str, variant: &str) -> bool {
    // Normalize all inputs to snake_case for comparison
    let normalized_query = cross_language_intelligence::to_snake_case(query);
    let normalized_symbol = cross_language_intelligence::to_snake_case(symbol_name);
    let normalized_variant = cross_language_intelligence::to_snake_case(variant);

    // Strip wildcards from variant for comparison
    let variant_clean = normalized_variant.trim_end_matches('*');

    // Check 1: Does the symbol name match the variant?
    if normalized_symbol == variant_clean {
        return true;
    }

    // Check 2: Does the symbol name match the original query?
    if normalized_symbol == normalized_query {
        return true;
    }

    // Check 3: Substring match (one contains the other)
    // This handles method names like "UserService.getData" → "get_data"
    if normalized_symbol.contains(variant_clean) || variant_clean.contains(&normalized_symbol) {
        return true;
    }

    if normalized_symbol.contains(&normalized_query)
        || normalized_query.contains(&normalized_symbol)
    {
        return true;
    }

    // No match found - this is a spurious result
    false
}
