use anyhow::{Result, anyhow, bail};
use julie_core::Symbol;
use julie_index::snapshot::Snapshot;
use serde::Serialize;

pub const DEFAULT_BODY_LIMIT: u32 = 100;

#[derive(Debug, Clone, Serialize)]
pub struct BodyPage {
    pub symbol_id: String,
    pub symbol: String,
    pub file_path: String,
    pub source_hash: String,
    pub canonical_source_range: [u32; 2],
    pub total_lines: usize,
    pub body_offset: usize,
    pub returned_range: [usize; 2],
    pub complete: bool,
    pub end_reached: bool,
    pub canonical_span_exact: bool,
    pub text: String,
    pub continuation: Option<String>,
    pub status: &'static str,
}

pub struct BodyExtraction {
    pub symbols: Vec<Symbol>,
    pub pages: Vec<BodyPage>,
    pub source_hash: Option<String>,
}

struct VerifiedSource {
    bytes: Vec<u8>,
    hash: String,
}

fn verified_source(
    snapshot: &Snapshot,
    path: &str,
    expected_hash: Option<&str>,
) -> Result<VerifiedSource> {
    let (bytes, hash) = snapshot.verified_file_bytes(path)?.ok_or_else(|| {
        anyhow!("source changed since the indexed snapshot; refresh and restart with body_offset=0")
    })?;
    if expected_hash.is_some_and(|expected| expected != hash) {
        bail!(
            "source changed since the previous body page; restart with body_offset=0 (expected {expected}, current {hash})",
            expected = expected_hash.unwrap_or_default()
        );
    }
    Ok(VerifiedSource { bytes, hash })
}

pub fn validate_source_hash(snapshot: &Snapshot, path: &str, expected_hash: &str) -> Result<()> {
    verified_source(snapshot, path, Some(expected_hash)).map(|_| ())
}

fn verify_symbol_version(
    snapshot: &Snapshot,
    symbol: &Symbol,
    source: &VerifiedSource,
) -> Result<()> {
    let row = snapshot
        .graph()
        .symbol_by_row_id(&symbol.id)
        .map(|id| snapshot.graph().symbol(id))
        .ok_or_else(|| anyhow!("symbol is absent from the current snapshot"))?;
    if row.blob_hash != source.hash {
        bail!("source changed since the symbol snapshot; refresh and restart with body_offset=0");
    }
    Ok(())
}

pub fn body_page(
    snapshot: &Snapshot,
    symbol: &Symbol,
    body_offset: u32,
    body_limit: u32,
    expected_hash: Option<&str>,
) -> Result<BodyPage> {
    let source = verified_source(snapshot, &symbol.file_path, expected_hash)?;
    verify_symbol_version(snapshot, symbol, &source)?;
    body_page_from_source(&source, symbol, body_offset, body_limit)
}

fn body_page_from_source(
    source: &VerifiedSource,
    symbol: &Symbol,
    body_offset: u32,
    body_limit: u32,
) -> Result<BodyPage> {
    let start = symbol.start_byte as usize;
    let end = symbol.end_byte as usize;
    let full_text = std::str::from_utf8(&source.bytes)
        .map_err(|_| anyhow!("source file is not valid UTF-8"))?;
    let (canonical, exact, status) = if start < end && end <= source.bytes.len() {
        let Some(canonical) = full_text.get(start..end) else {
            return unavailable_page(source, symbol, body_offset);
        };
        (canonical, true, "exact")
    } else if start == 0 && end == 0 && symbol.start_line > 0 {
        let lines = full_text.split_inclusive('\n').collect::<Vec<_>>();
        let start_line = symbol.start_line.saturating_sub(1) as usize;
        let end_line = (symbol.end_line as usize).min(lines.len());
        if start_line >= end_line {
            return unavailable_page(source, symbol, body_offset);
        }
        let start_byte = lines[..start_line]
            .iter()
            .map(|line| line.len())
            .sum::<usize>();
        let end_byte = lines[..end_line]
            .iter()
            .map(|line| line.len())
            .sum::<usize>();
        (
            &full_text[start_byte..end_byte],
            false,
            "inferred_line_span",
        )
    } else {
        return unavailable_page(source, symbol, body_offset);
    };
    let lines = canonical.split_inclusive('\n').collect::<Vec<_>>();
    let offset = body_offset as usize;
    let limit = body_limit.max(1) as usize;
    let returned_end = offset.saturating_add(limit).min(lines.len());
    let text = if offset < lines.len() {
        lines[offset..returned_end].concat()
    } else {
        String::new()
    };
    let end_reached = returned_end >= lines.len();
    Ok(BodyPage {
        symbol_id: symbol.id.clone(),
        symbol: symbol.name.clone(),
        file_path: symbol.file_path.clone(),
        source_hash: source.hash.clone(),
        canonical_source_range: [symbol.start_line, symbol.end_line],
        total_lines: lines.len(),
        body_offset: offset,
        returned_range: [offset.min(lines.len()), returned_end],
        complete: exact && offset == 0 && end_reached,
        end_reached,
        canonical_span_exact: exact,
        text,
        continuation: None,
        status,
    })
}

fn unavailable_page(
    source: &VerifiedSource,
    symbol: &Symbol,
    body_offset: u32,
) -> Result<BodyPage> {
    Ok(BodyPage {
        symbol_id: symbol.id.clone(),
        symbol: symbol.name.clone(),
        file_path: symbol.file_path.clone(),
        source_hash: source.hash.clone(),
        canonical_source_range: [symbol.start_line, symbol.end_line],
        total_lines: 0,
        body_offset: body_offset as usize,
        returned_range: [0, 0],
        complete: false,
        end_reached: true,
        canonical_span_exact: false,
        text: String::new(),
        continuation: None,
        status: "missing_canonical_span",
    })
}

pub fn extract_code_bodies(
    snapshot: &Snapshot,
    mut symbols: Vec<Symbol>,
    mode: &str,
    body_offset: u32,
    body_limit: Option<u32>,
    source_hash: Option<&str>,
) -> Result<BodyExtraction> {
    if mode == "structure" {
        for symbol in &mut symbols {
            symbol.code_context = None;
        }
        return Ok(BodyExtraction {
            symbols,
            pages: Vec::new(),
            source_hash: None,
        });
    }

    let limit = body_limit.unwrap_or(DEFAULT_BODY_LIMIT).max(1);
    let Some(first) = symbols.first() else {
        return Ok(BodyExtraction {
            symbols,
            pages: Vec::new(),
            source_hash: None,
        });
    };
    let source = verified_source(snapshot, &first.file_path, source_hash)?;
    let mut pages = Vec::new();
    for symbol in &mut symbols {
        let should_extract = mode == "full" || (mode == "minimal" && symbol.parent_id.is_none());
        if !should_extract {
            symbol.code_context = None;
            continue;
        }
        verify_symbol_version(snapshot, symbol, &source)?;
        let page = body_page_from_source(&source, symbol, body_offset, limit)?;
        symbol.code_context = Some(page.text.clone());
        pages.push(page);
    }
    let source_hash = pages.first().map(|page| page.source_hash.clone());
    Ok(BodyExtraction {
        symbols,
        pages,
        source_hash,
    })
}
