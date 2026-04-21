use crate::tools::impact::ranking::RankedImpact;
use crate::tools::impact::seed::SeedContext;
use crate::tools::spillover::SpilloverFormat;

pub fn format_blast_radius(
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
    likely_tests: &[String],
    deleted_files: &[String],
    overflow_handle: Option<&str>,
    format: SpilloverFormat,
) -> String {
    let newline = match format {
        SpilloverFormat::Readable => "\n\n",
        SpilloverFormat::Compact => "\n",
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
    } else if deleted_files.is_empty() {
        sections.push("No impacted symbols found.".to_string());
    }

    if !likely_tests.is_empty() {
        let mut tests_block = String::from("Likely tests\n");
        tests_block.push_str(
            &likely_tests
                .iter()
                .map(|test| format!("- {}", test))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        sections.push(tests_block);
    }

    if !deleted_files.is_empty() {
        let mut deleted_block = String::from("Deleted files\n");
        deleted_block.push_str(
            &deleted_files
                .iter()
                .map(|file| format!("- {}", file))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        sections.push(deleted_block);
    }

    if let Some(handle) = overflow_handle {
        sections.push(format!("More available: spillover_handle={}", handle));
    }

    sections.join(newline)
}

pub fn impact_rows(impacts: &[RankedImpact], start_index: usize) -> Vec<String> {
    impacts
        .iter()
        .enumerate()
        .map(|(offset, impact)| {
            format!(
                "{}. {}  {}:{}\n   why: {}",
                start_index + offset,
                impact.symbol.name,
                impact.symbol.file_path,
                impact.symbol.start_line,
                impact.why
            )
        })
        .collect()
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
