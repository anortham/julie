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

/// Convert to wildcard query
/// "user service" → "user* AND service*"
pub fn to_wildcard_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("{}*", word))
        .collect::<Vec<String>>()
        .join(" AND ")
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

        // 3. GOOGLE-STYLE: AND query (all terms must be present)
        // "user auth controller post" → "user AND auth AND controller AND post"
        // This finds symbols containing ALL terms anywhere in the symbol
        let and_query = query
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" AND ");
        if and_query != query {
            variants.push(and_query);
        }

        // 4. Wildcard AND: More permissive matching
        // "user auth post" → "user* AND auth* AND post*"
        let wildcard_and = query
            .split_whitespace()
            .map(|word| format!("{}*", word))
            .collect::<Vec<String>>()
            .join(" AND ");
        variants.push(wildcard_and);

        // 5. OR query: Find symbols matching ANY term (most permissive)
        // "user auth post" → "(user OR auth OR post)"
        // Use this as last resort to ensure we return SOMETHING
        let or_query = to_or_query(query);
        if or_query != query {
            variants.push(or_query);
        }
    } else {
        // Single word queries: just add wildcards and fuzzy
        variants.push(format!("{}*", query));
        variants.push(format!("{}~1", query));
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
