use super::types::LineMatch;

pub(crate) fn format_grouped_line_matches(matches: &[LineMatch]) -> Vec<String> {
    let mut groups: Vec<LineMatchGroup<'_>> = Vec::new();

    for line_match in matches {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.file_path == line_match.file_path)
        {
            group.matches.push(line_match);
        } else {
            groups.push(LineMatchGroup {
                file_path: &line_match.file_path,
                matches: vec![line_match],
            });
        }
    }

    let mut lines = Vec::new();
    for group in groups {
        lines.push(format!(
            "{} ({} {})",
            group.file_path,
            group.matches.len(),
            pluralize_line(group.matches.len())
        ));
        for line_match in group.matches {
            lines.push(format!(
                "  {}: {}",
                line_match.line_number, line_match.line_content
            ));
        }
    }

    lines
}

struct LineMatchGroup<'a> {
    file_path: &'a str,
    matches: Vec<&'a LineMatch>,
}

fn pluralize_line(count: usize) -> &'static str {
    if count == 1 { "line" } else { "lines" }
}
