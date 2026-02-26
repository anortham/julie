//! Content extraction and token-budget truncation utilities for get_context.

/// Abbreviate a code body: first 5 lines + "..." + last 5 lines.
/// Returns the full code if it has 12 or fewer lines.
pub(crate) fn abbreviate_code(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= 12 {
        return code.to_string();
    }
    let mut out = String::new();
    for line in &lines[..5] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    // ... (abbreviated)\n");
    for (i, line) in lines[lines.len() - 5..].iter().enumerate() {
        out.push_str(line);
        if i < 4 {
            out.push('\n');
        }
    }
    out
}

/// Truncate code content to fit within a token budget.
/// Returns content unchanged if within budget.
/// Uses head-biased truncation (2/3 top, 1/3 bottom).
pub(crate) fn truncate_to_token_budget(code: &str, max_tokens: usize) -> String {
    use crate::utils::token_estimation::TokenEstimator;

    let estimator = TokenEstimator::new();
    let estimated = estimator.estimate_string(code);

    if estimated <= max_tokens {
        return code.to_string();
    }

    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= 5 {
        return code.to_string();
    }

    let target_lines = (lines.len() * max_tokens / estimated).max(5);
    if lines.len() <= target_lines {
        return code.to_string();
    }

    let head = (target_lines * 2 / 3).max(3);
    let tail = (target_lines - head).max(2);

    let mut out = String::new();
    for line in &lines[..head] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!(
        "    // ... ({} lines omitted to fit token budget)\n",
        lines.len() - head - tail
    ));
    for (i, line) in lines[lines.len() - tail..].iter().enumerate() {
        out.push_str(line);
        if i < tail - 1 {
            out.push('\n');
        }
    }
    out
}
