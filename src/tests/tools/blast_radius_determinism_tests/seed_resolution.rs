use super::*;

const NOISY_TS: &str = "import React from \"react\";\nexport class Pipeline {\n  status: string = \"\";\n  constructor() {}\n  execute() {}\n}\nexport interface Runner {}\nexport enum State { Ready }\nexport function runPipeline() {}\nexport type JobId = string;\nexport namespace PipelineNS {}\nlet tmp = 1;\nconst DEFAULT_LIMIT = 10;\n";

fn seed_names(graph: &Graph, tool: &BlastRadiusTool) -> Result<Vec<String>> {
    let mut names: Vec<String> = resolve_seed_context(tool, graph)?
        .seed_symbols
        .iter()
        .map(|id| graph.symbol(*id).name.clone())
        .collect();
    names.sort();
    Ok(names)
}

#[tokio::test]
async fn test_file_path_seeds_filter_noisy_structural_symbols_but_symbol_ids_are_exact()
-> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[("src/noisy.ts", NOISY_TS)])?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();

    let file_seeds = seed_names(
        graph,
        &BlastRadiusTool {
            file_paths: vec!["src/noisy.ts".to_string()],
            ..Default::default()
        },
    )?;
    assert_eq!(
        file_seeds,
        vec![
            "JobId",
            "Pipeline",
            "PipelineNS",
            "Runner",
            "State",
            "constructor",
            "execute",
            "runPipeline",
        ],
        "file path seeds should keep meaningful definitions and drop noisy structural symbols"
    );

    let explicit_seeds = seed_names(
        graph,
        &BlastRadiusTool {
            symbol_ids: vec![
                "status".to_string(),
                "Ready".to_string(),
                "tmp".to_string(),
                "DEFAULT_LIMIT".to_string(),
            ],
            ..Default::default()
        },
    )?;
    assert_eq!(
        explicit_seeds,
        vec!["DEFAULT_LIMIT", "Ready", "status", "tmp"],
        "explicit symbol ids must be preserved even when they point to noisy structural symbols"
    );
    Ok(())
}

#[tokio::test]
async fn test_symbol_ids_accept_graph_row_ids_and_dedupe_against_names() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[("src/noisy.ts", NOISY_TS)])?;
    let snapshot = snapshot_of(&context).await?;
    let graph = snapshot.graph();
    let row_id = graph.symbol(definition(graph, "runPipeline")).id.clone();

    let seeds = resolve_seed_context(
        &BlastRadiusTool {
            symbol_ids: vec![row_id, "runPipeline".to_string()],
            ..Default::default()
        },
        graph,
    )?;

    assert_eq!(seeds.seed_symbols, vec![definition(graph, "runPipeline")]);
    Ok(())
}
