use std::collections::BTreeSet;
use std::time::Instant;

use anyhow::{Result, ensure};
use serde_json::json;

use super::corpus::{Corpus, CorpusCase, CorpusFixture, build_fixture, call_case};

const SAMPLE_COUNT: usize = 7;

#[tokio::test]
async fn phase3_mixed_traversal_scorecard() -> Result<()> {
    let fixture = build_fixture()?;
    let mut case_reports = Vec::new();
    let mut default_micros = Vec::new();
    let mut web_micros = Vec::new();

    for case in &fixture.corpus.cases {
        let default_output = call_case(&fixture, case, None, None).await?;
        let web_output = call_case(&fixture, case, Some("web"), None).await?;
        let default_found = found_ids(&fixture.corpus, case, &default_output);
        let web_found = found_ids(&fixture.corpus, case, &web_output);
        let expected_default = id_set(&case.expected_default);
        let expected_web = id_set(&case.expected_web);
        let unexpected_default = difference(&default_found, &expected_default);
        let unexpected_web = difference(&web_found, &expected_web);

        ensure!(
            default_found == expected_default,
            "default output changed for {}: {default_output}",
            case.id
        );
        ensure!(
            unexpected_default.is_empty(),
            "unexpected default links for {}",
            case.id
        );
        ensure!(
            unexpected_web.is_empty(),
            "unexpected web links for {}: {web_output}",
            case.id
        );

        case_reports.push(json!({
            "id": case.id,
            "expected": expected_web,
            "default_found": default_found,
            "web_found": web_found,
            "web_missing": difference(&expected_web, &web_found),
            "unexpected_internal": unexpected_web,
        }));
        default_micros.extend(sample_mode(&fixture, case, None).await?);
        web_micros.extend(sample_mode(&fixture, case, Some("web")).await?);
    }

    let report = json!({
        "corpus_version": fixture.corpus.version,
        "hard_gates": {
            "default_unchanged": true,
            "unexpected_internal": 0,
        },
        "report_only": {
            "default_latency_micros": percentiles(default_micros),
            "web_latency_micros": percentiles(web_micros),
        },
        "cases": case_reports,
    });
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

async fn sample_mode(
    fixture: &CorpusFixture,
    case: &CorpusCase,
    mode: Option<&str>,
) -> Result<Vec<u128>> {
    let _ = call_case(fixture, case, mode, None).await?;
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        let _ = call_case(fixture, case, mode, None).await?;
        samples.push(started.elapsed().as_micros());
    }
    Ok(samples)
}

fn found_ids(corpus: &Corpus, case: &CorpusCase, output: &str) -> BTreeSet<String> {
    corpus
        .symbols
        .iter()
        .filter(|symbol| symbol.id != case.seed)
        .filter(|symbol| output.contains(&format!("{}  {}:", symbol.name, symbol.file_path)))
        .map(|symbol| symbol.id.clone())
        .collect()
}

fn id_set(ids: &[String]) -> BTreeSet<String> {
    ids.iter().cloned().collect()
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

fn percentiles(mut samples: Vec<u128>) -> serde_json::Value {
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    json!({ "samples": samples.len(), "p50": p50, "p95": p95 })
}
