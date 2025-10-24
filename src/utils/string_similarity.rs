/// Simple Levenshtein distance calculation for fuzzy string matching
///
/// Computes the minimum number of single-character edits (insertions, deletions, substitutions)
/// required to change one string into another.
///
/// # Examples
///
/// ```
/// use julie::utils::string_similarity::levenshtein_distance;
///
/// assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
/// assert_eq!(levenshtein_distance("hello", "hello"), 0);
/// ```
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    // Create a matrix to store distances
    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    // Initialize first row and column
    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    // Compute distances
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for (i, &char_a) in a_chars.iter().enumerate() {
        for (j, &char_b) in b_chars.iter().enumerate() {
            let cost = if char_a == char_b { 0 } else { 1 };

            matrix[i + 1][j + 1] = std::cmp::min(
                std::cmp::min(
                    matrix[i][j + 1] + 1, // deletion
                    matrix[i + 1][j] + 1, // insertion
                ),
                matrix[i][j] + cost, // substitution
            );
        }
    }

    matrix[a_len][b_len]
}

/// Find the closest match from a list of candidates
///
/// Returns (best_match, distance) tuple, or None if candidates is empty
///
/// # Examples
///
/// ```
/// use julie::utils::string_similarity::find_closest_match;
///
/// let candidates = vec!["apple", "application", "apply"];
/// let (best, distance) = find_closest_match("aplication", &candidates).unwrap();
/// assert_eq!(best, "application");
/// ```
pub fn find_closest_match<'a>(query: &str, candidates: &'a [&'a str]) -> Option<(&'a str, usize)> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_match = candidates[0];
    let mut best_distance = levenshtein_distance(query, best_match);

    for &candidate in candidates.iter().skip(1) {
        let distance = levenshtein_distance(query, candidate);
        if distance < best_distance {
            best_distance = distance;
            best_match = candidate;
        }
    }

    Some((best_match, best_distance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
        assert_eq!(levenshtein_distance("", "hello"), 5);
        assert_eq!(levenshtein_distance("hello", ""), 5);
    }

    #[test]
    fn test_find_closest_match() {
        let candidates = vec!["apple", "application", "apply"];
        let (best, distance) = find_closest_match("aplication", &candidates).unwrap();
        assert_eq!(best, "application");
        assert_eq!(distance, 1); // "aplication" → "application" requires inserting one 'p'
    }

    #[test]
    fn test_find_closest_match_workspace_ids() {
        // Simulate real workspace ID typos
        let candidates = vec![
            "coa-codesearch-mcp_9037416c",
            "coa-intranet_cdcd7a9d",
            "julie_316c0b08",
        ];

        // Test: completely wrong workspace name - algorithm finds closest match
        // "coa-mcp-framework_c77f81e4" is closer to "coa-intranet_cdcd7a9d"
        // in terms of edit distance (both start with "coa-" and similar length)
        let (best, _distance) =
            find_closest_match("coa-mcp-framework_c77f81e4", &candidates).unwrap();
        assert_eq!(best, "coa-intranet_cdcd7a9d");

        // Test: wrong hash with correct prefix - should match exact prefix
        let (best, distance) =
            find_closest_match("coa-codesearch-mcp_wronghash", &candidates).unwrap();
        assert_eq!(best, "coa-codesearch-mcp_9037416c");
        // This should have a reasonable distance since only the hash is wrong
        assert!(distance < "coa-codesearch-mcp_wronghash".len() / 2);
    }

    #[test]
    fn test_workspace_with_spaces() {
        // Test workspace names with spaces (if we support them)
        let candidates = vec!["my workspace_abc123", "your workspace_def456"];
        let (best, _) = find_closest_match("my workspce_abc123", &candidates).unwrap();
        assert_eq!(best, "my workspace_abc123");
    }
}
