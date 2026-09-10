mod behavior;
mod corpus;
mod scorecard;

use anyhow::Result;

use self::corpus::{build_fixture, call_case};

#[tokio::test]
async fn phase3_default_mode_matches_legacy_snapshot() -> Result<()> {
    let fixture = build_fixture()?;
    let case = fixture.corpus.case("http_mixed")?;
    let output = call_case(&fixture, case, None, None).await?;

    assert_eq!(
        output,
        "Blast radius from 1 seed symbol\nHigh impact\n1. cycleA  cycles/cycle_a.rs:1\n   why: direct caller, 1 hop, centrality=medium\n2. showUser  backend/show_user.php:4\n   why: direct caller, 1 hop, centrality=low\n3. cycleB  cycles/cycle_b.rs:1\n   why: reaches cycleA in 2 hops, centrality=medium"
    );
    Ok(())
}
