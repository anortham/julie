//! Shared validation and formatting utilities for editing tools.

use anyhow::Result;

/// Check that all brackets, braces, and parentheses are matched in the content.
///
/// Returns Ok(()) if balanced, Err with details if unmatched.
pub fn check_bracket_balance(content: &str) -> Result<()> {
    let mut stack: Vec<char> = Vec::new();

    for ch in content.chars() {
        match ch {
            '{' | '[' | '(' => stack.push(ch),
            '}' => {
                if stack.last() == Some(&'{') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing brace '}}' -- edit would create invalid syntax"
                    ));
                }
            }
            ']' => {
                if stack.last() == Some(&'[') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing bracket ']' -- edit would create invalid syntax"
                    ));
                }
            }
            ')' => {
                if stack.last() == Some(&'(') {
                    stack.pop();
                } else {
                    return Err(anyhow::anyhow!(
                        "Unmatched closing paren ')' -- edit would create invalid syntax"
                    ));
                }
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        let unmatched: String = stack.iter().collect();
        return Err(anyhow::anyhow!(
            "Unmatched opening bracket(s): '{}' -- edit would create invalid syntax",
            unmatched
        ));
    }

    Ok(())
}

/// Determine if a file should have bracket balance checked based on extension.
/// Non-code files (markdown, yaml, json, toml) skip the check.
pub fn should_check_balance(file_path: &str) -> bool {
    let skip_extensions = [
        ".md", ".yaml", ".yml", ".json", ".toml", ".txt", ".csv", ".xml", ".html",
    ];
    !skip_extensions
        .iter()
        .any(|ext| file_path.ends_with(ext))
}

/// Format a unified diff between before and after content.
/// Returns a compact diff string with context around changes.
pub fn format_unified_diff(before: &str, after: &str, file_path: &str) -> String {
    use std::fmt::Write;

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut output = String::new();

    writeln!(output, "--- {}", file_path).unwrap();
    writeln!(output, "+++ {}", file_path).unwrap();

    let max_len = before_lines.len().max(after_lines.len());
    let context = 3;
    let mut last_change: Option<usize> = None;

    for i in 0..max_len {
        let b = before_lines.get(i).copied();
        let a = after_lines.get(i).copied();

        if b != a {
            // Print context lines before the change (if we haven't recently)
            let context_start = i.saturating_sub(context);
            let print_from = match last_change {
                Some(lc) if context_start <= lc + context => lc + context + 1,
                _ => context_start,
            };
            for j in print_from..i {
                if let Some(line) = before_lines.get(j) {
                    writeln!(output, " {}", line).unwrap();
                }
            }

            if let Some(line) = b {
                writeln!(output, "-{}", line).unwrap();
            }
            if let Some(line) = a {
                writeln!(output, "+{}", line).unwrap();
            }
            last_change = Some(i);
        } else if let Some(lc) = last_change {
            // Print trailing context after a change
            if i <= lc + context {
                if let Some(line) = b {
                    writeln!(output, " {}", line).unwrap();
                }
            }
        }
    }

    output
}
