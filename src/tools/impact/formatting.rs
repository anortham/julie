use crate::tools::impact::LikelyTests;
use crate::tools::impact::ranking::RankedImpact;
use crate::tools::impact::seed::SeedContext;
use crate::tools::spillover::{SpilloverFormat, SpilloverStore, more_available_marker};

/// Extra context that shapes the blast-radius header line.
///
/// Kept as a struct so new optional context (e.g. workspace label) can be added
/// without bumping the arity of `format_blast_radius`.
#[derive(Debug, Clone, Default)]
pub struct BlastRadiusHeader {
    /// Inclusive `(from, to)` database revision range driving the seed, if
    /// the caller asked for a revision-range blast radius.
    pub revision_range: Option<(i64, i64)>,
    /// True when deleted files are present. Julie does not keep historical
    /// caller graphs for removed files, so that section is path-only.
    pub deleted_files_path_only: bool,
    /// Spillover handle for high-impact rows beyond the first visible page.
    pub impact_overflow_handle: Option<String>,
    /// Spillover handle for likely-test paths beyond the visible cap.
    pub likely_test_paths_overflow_handle: Option<String>,
    /// Spillover handle for related test symbols beyond the visible cap.
    pub related_test_symbols_overflow_handle: Option<String>,
}

pub fn format_blast_radius(
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
    likely_tests: &LikelyTests,
    deleted_files: &[String],
    format: SpilloverFormat,
    header: BlastRadiusHeader,
) -> String {
    let newline = match format {
        SpilloverFormat::Readable => "\n\n",
        SpilloverFormat::Compact => "\n",
    };

    let mut sections = Vec::new();
    sections.push(header_line(seed_context, &header));

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

    if !likely_tests.likely_test_paths.is_empty() {
        sections.push(tests_block(
            "Likely tests",
            &likely_tests.likely_test_paths,
            likely_tests.likely_test_paths_total,
            "likely-test paths",
            header.likely_test_paths_overflow_handle.as_deref(),
        ));
    }

    if !likely_tests.related_test_symbols.is_empty() {
        sections.push(tests_block(
            "Related test symbols",
            &likely_tests.related_test_symbols,
            likely_tests.related_test_symbols_total,
            "related test symbols",
            header.related_test_symbols_overflow_handle.as_deref(),
        ));
    }

    if !deleted_files.is_empty() {
        let mut deleted_block = String::from("Deleted files\n");
        if header.deleted_files_path_only {
            deleted_block.push_str(
                "Note: deleted-file impact is path-only because historical callers are unavailable after the file has been removed.\n",
            );
        }
        deleted_block.push_str(
            &deleted_files
                .iter()
                .map(|file| format!("- {}", file))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        sections.push(deleted_block);
    }

    if let Some(handle) = header.impact_overflow_handle.as_deref() {
        sections.push(more_available_marker(handle));
    }

    sections.join(newline)
}

fn tests_block(
    heading: &str,
    entries: &[String],
    total: usize,
    overflow_label: &str,
    overflow_handle: Option<&str>,
) -> String {
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
        match overflow_handle {
            Some(handle) => {
                block.push_str(&format!(
                    "\n- …and {remaining} more {overflow_label} available\n{}",
                    more_available_marker(handle)
                ));
            }
            None => {
                block.push_str(&format!("\n- …and {remaining} more"));
            }
        }
    }
    block
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

pub(super) fn store_list_overflow(
    spillover_store: &SpilloverStore,
    session_id: &str,
    prefix: &str,
    title: &str,
    entries: &[String],
    visible_limit: usize,
    format: SpilloverFormat,
) -> Option<String> {
    if entries.len() <= visible_limit {
        return None;
    }

    let rows = entries[visible_limit..]
        .iter()
        .map(|entry| format!("- {}", entry))
        .collect();
    spillover_store.store_rows(session_id, prefix, title, rows, 0, visible_limit, format)
}

fn header_line(seed_context: &SeedContext, header: &BlastRadiusHeader) -> String {
    let file_count = seed_context.changed_files.len();
    let seed_count = seed_context.seed_symbols.len();

    let base = match (file_count, seed_count) {
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
    };

    match header.revision_range {
        Some((from, to)) => format!("{} (revs {}..{})", base, from, to),
        None => base,
    }
}
