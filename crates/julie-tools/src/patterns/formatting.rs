use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use julie_extractors::base::StructuralFact;
use serde::Serialize;
use serde_json::{Value, json};

use super::{PatternsFormat, PatternsGroupBy};

#[derive(Debug, Serialize)]
struct PatternListRow {
    pattern_id: String,
    count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SummaryKey {
    language: String,
    pattern_id: String,
    capture_name: String,
    location: String,
    facet_value: Option<String>,
}

pub(super) fn format_list(observed: Vec<(String, u64)>, format: PatternsFormat) -> Result<String> {
    match format {
        PatternsFormat::Compact => Ok(observed
            .into_iter()
            .map(|(pattern_id, count)| format!("{pattern_id} ({count})"))
            .collect::<Vec<_>>()
            .join("\n")),
        PatternsFormat::Json => {
            let patterns = observed
                .into_iter()
                .map(|(pattern_id, count)| PatternListRow { pattern_id, count })
                .collect::<Vec<_>>();
            Ok(serde_json::to_string(&json!({
                "schema_version": 1,
                "operation": "list",
                "patterns": patterns,
            }))?)
        }
    }
}

pub(super) fn format_search(
    facts: Vec<StructuralFact>,
    matched_pattern_ids: &[String],
    format: PatternsFormat,
) -> Result<String> {
    match format {
        PatternsFormat::Compact => {
            let mut lines = Vec::with_capacity(facts.len() + 1);
            if matched_pattern_ids.len() > 1 {
                lines.push(format!(
                    "matched_pattern_ids: {}",
                    matched_pattern_ids.join(", ")
                ));
            }
            lines.extend(facts.iter().map(compact_fact));
            Ok(lines.join("\n"))
        }
        PatternsFormat::Json => {
            let records = facts.iter().map(fact_json).collect::<Vec<_>>();
            Ok(serde_json::to_string(&json!({
                "schema_version": 1,
                "operation": "search",
                "matched_pattern_ids": matched_pattern_ids,
                "records": records,
            }))?)
        }
    }
}

pub(super) fn format_summary(
    facts: Vec<StructuralFact>,
    group_by: PatternsGroupBy,
    facet: Option<&str>,
    format: PatternsFormat,
) -> Result<String> {
    let groups = summary_groups(facts, group_by, facet);
    match format {
        PatternsFormat::Compact => Ok(groups
            .iter()
            .map(|(key, count)| {
                let mut labels = vec![
                    key.language.clone(),
                    key.pattern_id.clone(),
                    key.capture_name.clone(),
                ];
                if !key.location.is_empty() {
                    labels.push(key.location.clone());
                }
                if let Some(value) = &key.facet_value {
                    labels.push(format!("facet={value}"));
                }
                format!("{} ({count})", labels.join(" | "))
            })
            .collect::<Vec<_>>()
            .join("\n")),
        PatternsFormat::Json => {
            let groups = groups
                .into_iter()
                .map(|(key, count)| {
                    let mut value = json!({
                        "language": key.language,
                        "pattern_id": key.pattern_id,
                        "capture_name": key.capture_name,
                        "count": count,
                    });
                    match group_by {
                        PatternsGroupBy::LanguagePatternCapture => {}
                        PatternsGroupBy::File => value["file"] = Value::String(key.location),
                        PatternsGroupBy::Directory => {
                            value["directory"] = Value::String(key.location)
                        }
                    }
                    if let Some(facet_value) = key.facet_value {
                        value["facet_value"] = Value::String(facet_value);
                    }
                    value
                })
                .collect::<Vec<_>>();
            Ok(serde_json::to_string(&json!({
                "schema_version": 1,
                "operation": "summary",
                "group_by": group_by,
                "facet": facet,
                "groups": groups,
            }))?)
        }
    }
}

fn compact_fact(fact: &StructuralFact) -> String {
    let mut line = format!(
        "{}:{} {} {} {}",
        fact.file_path, fact.start_line, fact.capture_name, fact.pattern_id, fact.id
    );
    let metadata = sorted_metadata(fact);
    if !metadata.is_empty() {
        line.push_str(" metadata=");
        line.push_str(
            &metadata
                .into_iter()
                .map(|(key, value)| format!("{key}={}", display_value(&value)))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    line
}

fn fact_json(fact: &StructuralFact) -> Value {
    json!({
        "id": fact.id,
        "file_path": fact.file_path,
        "language": fact.language,
        "pattern_id": fact.pattern_id,
        "capture_name": fact.capture_name,
        "node_kind": fact.node_kind,
        "containing_symbol_id": fact.containing_symbol_id,
        "start_line": fact.start_line,
        "start_column": fact.start_column,
        "end_line": fact.end_line,
        "end_column": fact.end_column,
        "start_byte": fact.start_byte,
        "end_byte": fact.end_byte,
        "confidence": fact.confidence,
        "metadata": sorted_metadata(fact),
    })
}

fn summary_groups(
    facts: Vec<StructuralFact>,
    group_by: PatternsGroupBy,
    facet: Option<&str>,
) -> BTreeMap<SummaryKey, u64> {
    let mut groups = BTreeMap::new();
    for fact in facts {
        let facet_value = match facet {
            Some(key) => fact
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(key))
                .map(display_value),
            None => None,
        };
        if facet.is_some() && facet_value.is_none() {
            continue;
        }
        let location = match group_by {
            PatternsGroupBy::LanguagePatternCapture => String::new(),
            PatternsGroupBy::File => fact.file_path.clone(),
            PatternsGroupBy::Directory => directory_for(&fact.file_path),
        };
        let key = SummaryKey {
            language: fact.language,
            pattern_id: fact.pattern_id,
            capture_name: fact.capture_name,
            location,
            facet_value,
        };
        *groups.entry(key).or_default() += 1;
    }
    groups
}

fn sorted_metadata(fact: &StructuralFact) -> BTreeMap<String, Value> {
    fact.metadata
        .as_ref()
        .map(|metadata| {
            metadata
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn display_value(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn directory_for(file_path: &str) -> String {
    Path::new(file_path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| ".".to_string())
}
