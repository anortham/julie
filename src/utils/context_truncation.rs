/// Context truncation utilities for limiting code context while preserving essential structure
/// Based on battle-tested patterns from COA.CodeSearch.McpServer
pub struct ContextTruncator {
    // Placeholder - implement after tests
}

impl Default for ContextTruncator {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextTruncator {
    pub fn new() -> Self {
        Self {}
    }

    pub fn truncate_lines(&self, lines: &[String], max_lines: usize) -> Vec<String> {
        // Minimal implementation: if within limit, return as-is
        if lines.len() <= max_lines {
            lines.to_vec()
        } else {
            // For now, just take first max_lines
            lines.iter().take(max_lines).cloned().collect()
        }
    }

    /// Smart truncation that preserves essential code structure
    /// Returns a String representation with intelligent truncation and ellipsis indicators
    pub fn smart_truncate(&self, lines: &[String], max_lines: usize) -> String {
        if lines.is_empty() {
            return String::new();
        }

        if max_lines == 0 {
            return String::new();
        }

        if lines.len() <= max_lines {
            // No truncation needed
            return lines.join("\n");
        }

        // Identify essential lines
        let essential_lines = self.identify_essential_lines(lines);

        // Collect essential lines and their indices
        let mut essential_indices = Vec::new();
        for (i, is_essential) in essential_lines.iter().enumerate() {
            if *is_essential {
                essential_indices.push(i);
            }
        }

        // Ensure we have first and last lines if possible
        if !essential_indices.contains(&0) {
            essential_indices.insert(0, 0);
        }
        let last_index = lines.len() - 1;
        if !essential_indices.contains(&last_index) {
            essential_indices.push(last_index);
        }

        // Sort and deduplicate
        essential_indices.sort();
        essential_indices.dedup();

        // Take only the first max_lines essential indices
        if essential_indices.len() > max_lines {
            // Prioritize keeping first and last
            let mut final_indices = vec![essential_indices[0]]; // Always keep first

            // Take middle essential lines up to max_lines - 2 (reserving space for first and last)
            let middle_count = max_lines.saturating_sub(2);
            for &idx in essential_indices.iter().skip(1).take(middle_count) {
                if idx != last_index {
                    final_indices.push(idx);
                }
            }

            // Always try to keep last if we have room
            if final_indices.len() < max_lines && !final_indices.contains(&last_index) {
                final_indices.push(last_index);
            }

            essential_indices = final_indices;
            essential_indices.sort();
        }

        // Build result with ellipsis markers
        let mut result = Vec::new();
        let mut last_included = None;

        for &idx in &essential_indices {
            // Add ellipsis if we skipped lines
            if let Some(last_idx) = last_included {
                if idx > last_idx + 1 {
                    let skipped_count = idx - last_idx - 1;
                    result.push(format!("... ({} lines truncated) ...", skipped_count));
                }
            }

            result.push(lines[idx].clone());
            last_included = Some(idx);
        }

        result.join("\n")
    }

    /// Identify lines that should be preserved during smart truncation
    fn identify_essential_lines(&self, lines: &[String]) -> Vec<bool> {
        let mut essential = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Doc comments and regular comments at the start
            if trimmed.starts_with("///") || trimmed.starts_with("/**") || trimmed.starts_with("//")
            {
                essential[i] = true;
            }

            // Function signatures
            if trimmed.contains("fn ")
                || trimmed.contains("function ")
                || trimmed.contains("def ")
                || trimmed.contains("public ")
                || trimmed.contains("private ")
                || trimmed.contains("protected ")
            {
                essential[i] = true;
            }

            // Class/struct/interface definitions
            if trimmed.contains("class ")
                || trimmed.contains("struct ")
                || trimmed.contains("interface ")
                || trimmed.contains("enum ")
            {
                essential[i] = true;
            }

            // Attributes and decorators
            if trimmed.starts_with("#[") || trimmed.starts_with("@") {
                essential[i] = true;
            }

            // Return statements
            if trimmed.starts_with("return ")
                || trimmed.starts_with("Ok(")
                || trimmed.starts_with("Err(")
                || trimmed == "}"
            {
                essential[i] = true;
            }

            // Closing braces or brackets (end of blocks)
            if trimmed == "}" || trimmed == "};" || trimmed == "});" {
                essential[i] = true;
            }
        }

        essential
    }
}
