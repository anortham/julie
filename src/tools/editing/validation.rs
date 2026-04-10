//! Shared validation and formatting utilities for editing tools.

/// Count the net bracket balance in content (open minus close for each type).
/// Counts raw characters without skipping strings or comments.
fn count_bracket_balance(content: &str) -> (i32, i32, i32) {
    let (mut braces, mut brackets, mut parens) = (0i32, 0i32, 0i32);
    for ch in content.chars() {
        match ch {
            '{' => braces += 1,
            '}' => braces -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '(' => parens += 1,
            ')' => parens -= 1,
            _ => {}
        }
    }
    (braces, brackets, parens)
}

/// Check if an edit changes bracket balance. Returns a warning string if
/// the balance changed (possible syntax issue), or None if balanced.
/// This is advisory, not a hard reject, because the check cannot distinguish
/// brackets in code from brackets in strings/comments.
pub fn check_bracket_balance(before: &str, after: &str) -> Option<String> {
    let (bb, kb, pb) = count_bracket_balance(before);
    let (ba, ka, pa) = count_bracket_balance(after);

    if bb != ba || kb != ka || pb != pa {
        let mut issues = Vec::new();
        if bb != ba {
            issues.push(format!("braces {{}} changed by {}", ba - bb));
        }
        if kb != ka {
            issues.push(format!("brackets [] changed by {}", ka - kb));
        }
        if pb != pa {
            issues.push(format!("parens () changed by {}", pa - pb));
        }
        return Some(format!(
            "Warning: edit changes bracket balance ({}) -- verify this is intentional",
            issues.join(", ")
        ));
    }

    None
}

/// Determine if a file should have bracket balance checked based on extension.
/// Non-code files (markdown, yaml, json, toml) skip the check.
pub fn should_check_balance(file_path: &str) -> bool {
    let skip_extensions = [
        ".md", ".yaml", ".yml", ".json", ".toml", ".txt", ".csv", ".xml", ".html",
    ];
    !skip_extensions.iter().any(|ext| file_path.ends_with(ext))
}

/// Format a unified diff between before and after content using LCS alignment.
/// Produces compact output with 3 lines of context around changes.
pub fn format_unified_diff(before: &str, after: &str, file_path: &str) -> String {
    use std::fmt::Write;

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut output = String::new();

    writeln!(output, "--- {}", file_path).unwrap();
    writeln!(output, "+++ {}", file_path).unwrap();

    // Trim common prefix and suffix to minimize the LCS work
    let mut prefix_len = 0;
    while prefix_len < before_lines.len()
        && prefix_len < after_lines.len()
        && before_lines[prefix_len] == after_lines[prefix_len]
    {
        prefix_len += 1;
    }

    let mut suffix_len = 0;
    while suffix_len < before_lines.len() - prefix_len
        && suffix_len < after_lines.len() - prefix_len
        && before_lines[before_lines.len() - 1 - suffix_len]
            == after_lines[after_lines.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let before_mid = &before_lines[prefix_len..before_lines.len() - suffix_len];
    let after_mid = &after_lines[prefix_len..after_lines.len() - suffix_len];

    // If nothing changed, return just the header
    if before_mid.is_empty() && after_mid.is_empty() {
        return output;
    }

    // Compute LCS on the differing middle section
    let lcs_pairs = compute_lcs(before_mid, after_mid);

    // Build edit operations from LCS
    let mut ops: Vec<(char, &str)> = Vec::new();

    // Common prefix (context only)
    for line in &before_lines[..prefix_len] {
        ops.push((' ', line));
    }

    // Middle section: walk both sequences using LCS matches as anchors
    let mut ai = 0;
    let mut bi = 0;
    for &(la, lb) in &lcs_pairs {
        while ai < la {
            ops.push(('-', before_mid[ai]));
            ai += 1;
        }
        while bi < lb {
            ops.push(('+', after_mid[bi]));
            bi += 1;
        }
        ops.push((' ', before_mid[ai]));
        ai += 1;
        bi += 1;
    }
    while ai < before_mid.len() {
        ops.push(('-', before_mid[ai]));
        ai += 1;
    }
    while bi < after_mid.len() {
        ops.push(('+', after_mid[bi]));
        bi += 1;
    }

    // Common suffix (context only)
    for line in &before_lines[before_lines.len() - suffix_len..] {
        ops.push((' ', line));
    }

    // Format with context: only show regions around changes
    let context = 3;
    let change_indices: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (op, _))| *op != ' ')
        .map(|(i, _)| i)
        .collect();

    if change_indices.is_empty() {
        return output;
    }

    // Build visible ranges (change positions +/- context, merged when overlapping)
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &ci in &change_indices {
        let start = ci.saturating_sub(context);
        let end = (ci + context + 1).min(ops.len());
        if let Some(last) = ranges.last_mut() {
            if start <= last.1 {
                last.1 = end;
                continue;
            }
        }
        ranges.push((start, end));
    }

    // Precompute line numbers: (old_lineno, new_lineno) at each op position
    let mut line_nos: Vec<(usize, usize)> = Vec::with_capacity(ops.len());
    let mut old_no = 1usize;
    let mut new_no = 1usize;
    for &(op, _) in &ops {
        line_nos.push((old_no, new_no));
        match op {
            '-' => old_no += 1,
            '+' => new_no += 1,
            _ => {
                old_no += 1;
                new_no += 1;
            }
        }
    }

    for (start, end) in ranges {
        // Hunk header: count old-side lines (context + removed) and new-side (context + added)
        let old_start = line_nos[start].0;
        let new_start = line_nos[start].1;
        let old_count = ops[start..end].iter().filter(|(op, _)| *op != '+').count();
        let new_count = ops[start..end].iter().filter(|(op, _)| *op != '-').count();
        writeln!(
            output,
            "@@ -{},{} +{},{} @@",
            old_start, old_count, new_start, new_count
        )
        .unwrap();

        for i in start..end {
            let (op, line) = ops[i];
            writeln!(output, "{}{}", op, line).unwrap();
        }
    }

    output
}

/// Compute the Longest Common Subsequence between two line sequences.
/// Returns pairs of (index_in_a, index_in_b) for matching lines.
fn compute_lcs<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(usize, usize)> {
    let n = a.len();
    let m = b.len();

    if n == 0 || m == 0 {
        return Vec::new();
    }

    // O(n*m) DP table. Fine for code files (typically <1000 lines in the diff region
    // after prefix/suffix trimming).
    let mut dp = vec![vec![0u32; m + 1]; n + 1];

    for i in 1..=n {
        for j in 1..=m {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find matching pairs
    let mut pairs = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}
