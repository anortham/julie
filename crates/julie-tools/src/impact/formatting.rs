use crate::impact::LikelyTests;
use crate::impact::ranking::RankedImpact;
use crate::impact::seed::SeedContext;
use crate::shared::truncation_line;

/// Output layout for blast-radius text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlastRadiusFormat {
    Readable,
    Compact,
}

impl BlastRadiusFormat {
    /// Case-insensitive parse that rejects typos and empty strings.
    pub fn parse_strict(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "readable" => Ok(Self::Readable),
            "compact" => Ok(Self::Compact),
            "" => Err("format must be \"readable\" or \"compact\" (got empty string)".to_string()),
            other => Err(format!(
                "unknown format \"{other}\" (expected \"readable\" or \"compact\")"
            )),
        }
    }
}

/// Extra context that shapes the blast-radius header line.
///
/// Kept as a struct so new optional context (e.g. workspace label) can be added
/// without bumping the arity of `format_blast_radius`.
#[derive(Debug, Clone, Default)]
pub struct BlastRadiusHeader {
    /// True when more impact rows existed than the visible page kept.
    pub impact_overflow: bool,
    /// Pre-formatted `web`-mode caller rows (e.g. `"fetchUser  src/client.ts:3  via http_call GET /api/users/123"`).
    /// Empty in `default` mode, so the legacy blast-radius output is byte-identical.
    /// Truncated to the visible cap.
    pub web_callers: Vec<String>,
    /// Pre-truncate total count of web callers, driving the overflow marker.
    pub web_callers_total: usize,
}

pub fn format_blast_radius(
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
    likely_tests: &LikelyTests,
    format: BlastRadiusFormat,
    header: BlastRadiusHeader,
) -> String {
    let newline = match format {
        BlastRadiusFormat::Readable => "\n\n",
        BlastRadiusFormat::Compact => "\n",
    };

    let mut sections = Vec::new();
    sections.push(header_line(seed_context));

    if !impacts.is_empty() {
        let mut impact_block = String::from("High impact\n");
        impact_block.push_str(
            &impact_rows(impacts, 1)
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n"),
        );
        sections.push(impact_block);
    } else {
        sections.push("No impacted symbols found.".to_string());
    }

    if !likely_tests.likely_test_paths.is_empty() {
        sections.push(tests_block(
            "Likely tests",
            &likely_tests.likely_test_paths,
            likely_tests.likely_test_paths_total,
        ));
    }

    if !likely_tests.related_test_symbols.is_empty() {
        sections.push(tests_block(
            "Related test symbols",
            &likely_tests.related_test_symbols,
            likely_tests.related_test_symbols_total,
        ));
    }

    if !header.web_callers.is_empty() {
        let mut web_block = String::from("Web callers\n");
        web_block.push_str(&header.web_callers.join("\n"));
        let shown = header.web_callers.len();
        let effective_total = header.web_callers_total.max(shown);
        if effective_total > shown {
            let remaining = effective_total - shown;
            web_block.push_str(&format!("\n- …and {remaining} more web callers"));
        }
        sections.push(web_block);
    }

    if header.impact_overflow {
        sections.push(truncation_line(impacts.len()));
    }

    sections.join(newline)
}

fn tests_block(heading: &str, entries: &[String], total: usize) -> String {
    let mut block = format!("{}\n", heading);
    block.push_str(
        &entries
            .iter()
            .map(|entry| format!("- {}", entry))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // `total` is the pre-truncate count. If more entries existed than we
    // rendered, surface an overflow marker so agents know the list is capped.
    // If `total` is zero (legacy/default construction), fall back to the
    // visible count so we never emit a bogus "…and 0 more" line.
    let shown = entries.len();
    let effective_total = total.max(shown);
    if effective_total > shown {
        let remaining = effective_total - shown;
        block.push_str(&format!("\n- …and {remaining} more"));
    }
    block
}

pub fn impact_rows(impacts: &[RankedImpact], start_index: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < impacts.len() {
        let end = same_file_impact_run_end(impacts, index);
        if end - index > 1 {
            rows.push(format_impact_group(
                &impacts[index..end],
                start_index + index,
            ));
        } else {
            rows.push(format_impact_row(&impacts[index], start_index + index));
        }
        index = end;
    }
    rows
}

fn same_file_impact_run_end(impacts: &[RankedImpact], start: usize) -> usize {
    let file_path = &impacts[start].symbol.file_path;
    let mut end = start + 1;
    while end < impacts.len() && impacts[end].symbol.file_path == *file_path {
        end += 1;
    }
    end
}

fn format_impact_row(impact: &RankedImpact, rank: usize) -> String {
    format!(
        "{}. {}  {}:{}\n   why: {}",
        rank, impact.symbol.name, impact.symbol.file_path, impact.symbol.start_line, impact.why
    )
}

fn format_impact_group(impacts: &[RankedImpact], start_rank: usize) -> String {
    let file_path = &impacts[0].symbol.file_path;
    let mut block = format!("{file_path}:");
    for (offset, impact) in impacts.iter().enumerate() {
        block.push_str(&format!(
            "\n{}. {}  :{}\n   why: {}",
            start_rank + offset,
            impact.symbol.name,
            impact.symbol.start_line,
            impact.why
        ));
    }
    block
}

fn header_line(seed_context: &SeedContext) -> String {
    let file_count = seed_context.changed_files.len();
    let seed_count = seed_context.seed_symbols.len();

    match (file_count, seed_count) {
        (0, 0) => "Blast radius".to_string(),
        (0, 1) => "Blast radius from 1 seed symbol".to_string(),
        (0, _) => format!("Blast radius from {} seed symbols", seed_count),
        (1, 0) => "Blast radius from 1 changed file".to_string(),
        (1, 1) => "Blast radius from 1 changed file, 1 seed symbol".to_string(),
        (1, _) => format!(
            "Blast radius from 1 changed file, {} seed symbols",
            seed_count
        ),
        (_, 0) => format!("Blast radius from {} changed files", file_count),
        (_, 1) => format!(
            "Blast radius from {} changed files, 1 seed symbol",
            file_count
        ),
        _ => format!(
            "Blast radius from {} changed files, {} seed symbols",
            file_count, seed_count
        ),
    }
}
