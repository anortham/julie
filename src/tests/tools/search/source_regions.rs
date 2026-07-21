use std::sync::Arc;

use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_core::database::SymbolDatabase;
use julie_core::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use julie_extractors::base::{SourceRegion, SourceRegionKind};
use julie_index::search::index::{SearchDocument, SearchIndex};
use julie_test_support::FakeToolContext;
use julie_test_support::db::file_info_builder;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tools::search::regions::SourceRegionFilter;
use crate::tools::search::{FastSearchParams, FastSearchTool, SearchBackend};

struct RegionSearchFixture {
    _temp: TempDir,
    context: FakeToolContext,
}

fn source_region(
    id: &str,
    file_path: &str,
    kind: SourceRegionKind,
    start_line: u32,
    end_line: u32,
) -> SourceRegion {
    SourceRegion {
        id: id.into(),
        file_path: file_path.into(),
        language: "rust".into(),
        kind,
        containing_symbol_id: None,
        start_line,
        start_column: 0,
        end_line,
        end_column: 128,
        start_byte: 0,
        end_byte: 128,
        metadata: None,
    }
}

fn seed_database(
    db_path: &std::path::Path,
    workspace_id: &str,
    file_path: &str,
    content: &str,
    regions: &[SourceRegion],
) -> Result<()> {
    let mut db = SymbolDatabase::new(db_path)?;
    let files = [file_info_builder(file_path)
        .language("rust")
        .hash(format!("{workspace_id}-hash"))
        .line_count(content.lines().count() as i32)
        .content(content)
        .build()];
    db.incremental_update_atomic_with_metadata(
        &[file_path.into()],
        &CanonicalWriteSet {
            files: &files,
            source_regions: regions,
            ..Default::default()
        },
        workspace_id,
        AtomicPersistenceMetadata::default(),
    )?;
    Ok(())
}

fn region_search_fixture(
    primary_content: &str,
    target_content: Option<&str>,
) -> Result<RegionSearchFixture> {
    let temp = TempDir::new()?;
    let primary_db_path = temp.path().join("primary.db");
    let target_db_path = temp.path().join("target.db");
    let file_path = "src/lib.rs";

    seed_database(
        &primary_db_path,
        "primary-workspace",
        file_path,
        primary_content,
        &[source_region(
            "primary-comment",
            file_path,
            SourceRegionKind::Comment,
            1,
            1,
        )],
    )?;

    if let Some(target_content) = target_content {
        seed_database(
            &target_db_path,
            "target-workspace",
            file_path,
            target_content,
            &[source_region(
                "target-comment",
                file_path,
                SourceRegionKind::Comment,
                1,
                1,
            )],
        )?;
    }

    let index_path = temp.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;
    let index = SearchIndex::create(&index_path)?;
    index.add_search_doc(&SearchDocument::file_from_parts(
        file_path,
        "// region workspace needle\nlet region_workspace_needle = 1;\n",
        "rust",
    ))?;
    index.commit()?;

    let mut context = FakeToolContext::new()
        .with_workspace_id("primary-workspace")
        .with_primary_root(temp.path())
        .with_primary_db_path(&primary_db_path)
        .with_search_index(Arc::new(index));
    if target_content.is_some() {
        context = context.with_workspace_db_path("target-workspace", &target_db_path);
    }

    Ok(RegionSearchFixture {
        _temp: temp,
        context,
    })
}

#[tokio::test]
async fn fast_search_regions_returns_only_matching_source_region_lines() -> Result<()> {
    let fixture = region_search_fixture("// region needle\nlet region_needle = 1;\n", None)?;

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
    let fixture = region_search_fixture("// region needle\n", None)?;
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
    let fixture = region_search_fixture(
        "// primary workspace needle\n",
        Some("// target workspace needle\n"),
    )?;
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
