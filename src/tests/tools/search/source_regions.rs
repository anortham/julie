use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_extractors::SourceRegionKind;
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::search::regions::SourceRegionFilter;
use crate::tools::search::{FastSearchParams, FastSearchTool, SearchBackend};

struct RegionSearchFixture {
    _temp: TempDir,
    context: FakeToolContext,
}

fn region_search_fixture(content: &str) -> Result<RegionSearchFixture> {
    let temp = TempDir::new()?;
    std::fs::create_dir_all(temp.path().join("src"))?;
    std::fs::write(temp.path().join("src/lib.rs"), content)?;
    let context = snapshot_context(temp.path())?;
    Ok(RegionSearchFixture {
        _temp: temp,
        context,
    })
}

#[tokio::test]
async fn fast_search_regions_returns_only_matching_source_region_lines() -> Result<()> {
    let fixture = region_search_fixture("// region needle\nlet region_needle = 1;\n")?;

    let result = FastSearchParams {
        search: FastSearchTool {
            query: "region needle".into(),
            return_format: "full".into(),
            ..Default::default()
        },
        regions: Some("comment".into()),
    }
    .call_tool(&fixture.context)
    .await?;

    let text = call_tool_result_text(&result);
    assert!(text.contains("src/lib.rs:1"), "{text}");
    assert!(!text.contains("src/lib.rs:2"), "{text}");
    Ok(())
}

#[tokio::test]
async fn fast_search_regions_rejects_unknown_region_and_symbol_backends() -> Result<()> {
    let fixture = region_search_fixture("// region needle\n")?;
    let parsed =
        SourceRegionFilter::parse("comment,doc_comment,docstring,string_literal,embedded")?;
    assert_eq!(
        parsed.0,
        vec![
            SourceRegionKind::Comment,
            SourceRegionKind::DocComment,
            SourceRegionKind::StringLiteral,
            SourceRegionKind::Embedded,
        ]
    );

    let unknown = FastSearchParams {
        search: FastSearchTool {
            query: "region needle".into(),
            ..Default::default()
        },
        regions: Some("unknown".into()),
    }
    .call_tool(&fixture.context)
    .await
    .unwrap_err();
    assert!(
        unknown
            .to_string()
            .contains("unknown source region: unknown")
    );

    for backend in [SearchBackend::Semantic, SearchBackend::Hybrid] {
        let error = FastSearchParams {
            search: FastSearchTool {
                query: "region needle".into(),
                backend: Some(backend),
                ..Default::default()
            },
            regions: Some("comment,doc_comment,docstring,string_literal,embedded".into()),
        }
        .call_tool(&fixture.context)
        .await
        .unwrap_err();
        assert!(error.to_string().contains("regions require lexical search"));
    }

    Ok(())
}

#[tokio::test]
async fn fast_search_regions_respects_target_workspace() -> Result<()> {
    let fixture = region_search_fixture("// target workspace needle\n")?;
    let context = fixture
        .context
        .with_resolved_target(WorkspaceTarget::Target("target-workspace".into()));

    let result = FastSearchParams {
        search: FastSearchTool {
            query: "workspace needle".into(),
            workspace: Some("target-workspace".into()),
            return_format: "full".into(),
            ..Default::default()
        },
        regions: Some("comment".into()),
    }
    .call_tool(&context)
    .await?;

    let text = call_tool_result_text(&result);
    assert!(text.contains("target workspace needle"), "{text}");
    assert!(!text.contains("primary workspace needle"), "{text}");
    Ok(())
}
