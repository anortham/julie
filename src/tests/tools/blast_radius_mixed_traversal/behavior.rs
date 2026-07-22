use anyhow::Result;

use super::corpus::{Corpus, build_fixture, call_case};

#[tokio::test]
async fn phase3_web_mode_traverses_http_mixed_path() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("http_mixed")?;
    let output = call_case(&fixture, case, Some("web"), None).await?;

    assert_symbol(&fixture.corpus, &output, "show_user")?;
    assert_symbol(&fixture.corpus, &output, "fetch_user")?;
    assert_symbol(&fixture.corpus, &output, "page_loader")?;
    assert!(output.contains("reaches showUser in 2 hops"), "{output}");
    assert!(output.contains("reaches fetchUser in 3 hops"), "{output}");
    Ok(())
}

#[tokio::test]
async fn phase3_web_mode_traverses_sql_mixed_path() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("sql_mixed")?;
    let output = call_case(&fixture, case, Some("web"), None).await?;

    assert_symbol(&fixture.corpus, &output, "load_report")?;
    assert_symbol(&fixture.corpus, &output, "report_page")?;
    assert!(output.contains("reaches loadReport in 2 hops"), "{output}");
    Ok(())
}

#[tokio::test]
async fn phase3_web_mode_keeps_external_edges_terminal() -> Result<()> {
    let fixture = build_fixture()?;
    let http_case = fixture.corpus.case("http_mixed")?;
    let sql_case = fixture.corpus.case("sql_mixed")?;
    let http_output = call_case(&fixture, http_case, Some("web"), None).await?;
    let sql_output = call_case(&fixture, sql_case, Some("web"), None).await?;

    assert_symbol_absent(&fixture.corpus, &http_output, "fetch_unknown")?;
    assert_symbol_absent(&fixture.corpus, &http_output, "unknown_page")?;
    assert_symbol_absent(&fixture.corpus, &sql_output, "load_unknown")?;
    assert_symbol_absent(&fixture.corpus, &sql_output, "unknown_report_page")?;
    Ok(())
}

#[tokio::test]
async fn phase3_web_mode_deduplicates_cycles_at_shortest_distance() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("http_mixed")?;
    let output = call_case(&fixture, case, Some("web"), None).await?;
    let high_impact = output.split("Web callers").next().unwrap_or(&output);

    assert_eq!(high_impact.matches("cycleA  cycles/cycle_a.rs:").count(), 1, "{output}");
    assert_eq!(high_impact.matches("cycleB  cycles/cycle_b.rs:").count(), 1, "{output}");
    assert_eq!(high_impact.matches("fetchUser  web/fetch_user.ts:").count(), 1, "{output}");
    Ok(())
}

#[tokio::test]
async fn phase3_web_mode_applies_one_combined_depth_limit() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("http_mixed")?;
    let output = call_case(&fixture, case, Some("web"), Some(2)).await?;

    assert_symbol(&fixture.corpus, &output, "fetch_user")?;
    assert_symbol_absent(&fixture.corpus, &output, "page_loader")?;
    Ok(())
}

#[tokio::test]
async fn phase3_web_mode_is_deterministic() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("http_mixed")?;
    let first = call_case(&fixture, case, Some("web"), None).await?;
    let second = call_case(&fixture, case, Some("web"), None).await?;

    assert_eq!(first, second);
    Ok(())
}

fn assert_symbol(corpus: &Corpus, output: &str, id: &str) -> Result<()> {
    let symbol = corpus.symbol(id)?;
    assert!(
        output.contains(&format!("{}  {}:", symbol.name, symbol.file_path)),
        "missing {id} from output:\n{output}"
    );
    Ok(())
}

fn assert_symbol_absent(corpus: &Corpus, output: &str, id: &str) -> Result<()> {
    let symbol = corpus.symbol(id)?;
    assert!(
        !output.contains(&format!("{}  {}:", symbol.name, symbol.file_path)),
        "unexpected {id} in output:\n{output}"
    );
    Ok(())
}
