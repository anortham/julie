use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use xtask::workspace_root;

const TIER_COMMANDS: &[&str] = &[
    "cargo xtask test dev",
    "cargo xtask test dogfood",
    "cargo xtask test full",
    "cargo nextest run --lib <name>",
];

const RETIRED_RUNNER_TERMS: &[&str] = &[
    "xtask test changed",
    "xtask test nano",
    "xtask test fast",
    "xtask test smoke",
    "xtask test system",
    "xtask test bucket",
    "xtask test inventory",
    "xtask test list",
    "OverBudget",
    "PREBUILD",
    "COLD WALL",
    "NEXTEST_TEST_THREADS",
];

#[test]
fn docs_contract_tests_claude_md_equals_agents_md() {
    assert_eq!(read_repo_file("CLAUDE.md"), read_repo_file("AGENTS.md"));
}

#[test]
fn docs_contract_tests_agent_docs_and_readme_list_the_three_tiers() {
    for path in ["CLAUDE.md", "AGENTS.md", "README.md"] {
        let contents = read_repo_file(path);
        for command in TIER_COMMANDS {
            assert!(contents.contains(command), "{path} missing `{command}`");
        }
    }
}

#[test]
fn docs_contract_tests_docs_drop_the_retired_runner_terms() {
    for path in [
        "CLAUDE.md",
        "AGENTS.md",
        "README.md",
        "docs/TESTING_GUIDE.md",
        "docs/plans/verification-ledger-template.md",
    ] {
        let contents = read_repo_file(path);
        for term in RETIRED_RUNNER_TERMS {
            assert!(!contents.contains(term), "{path} still contains `{term}`");
        }
    }
}

#[test]
fn docs_contract_tests_cargo_alias_uses_quiet_xtask_runner() {
    let contents = read_repo_file(".cargo/config.toml");

    assert!(contents.contains("xtask = \"run -q -p xtask --\""));
    assert!(contents.contains("xtask-eval = \"run -q -p xtask-eval --\""));
}

#[test]
fn docs_contract_tests_site_marketing_page_stays_current() {
    let html = read_repo_file("docs/site/index.html");
    let script = read_repo_file("docs/site/script.js");
    let tools = extract_section(&html, "tools");
    let skills = extract_section(&html, "skills");
    let codex_panel = extract_section(&html, "panel-codex");

    let package_version = root_package_version();
    assert!(
        html.contains(&format!("<span>v{package_version}</span>")),
        "site footer should display Cargo.toml package version v{package_version}"
    );

    let expected_tools = public_tool_names();
    let rendered_tools = extract_tool_names(&tools);
    assert_eq!(rendered_tools, expected_tools);
    assert!(
        tools.contains(&format!("{} focused tools", expected_tools.len())),
        "tools section count should match public tool source of truth"
    );
    assert!(skills.contains("/web-research"));
    assert!(html.contains("og-card.svg"));
    assert!(repo_file("docs/site/og-card.svg").exists());
    assert!(codex_panel.contains("[mcp_servers.julie]"));
    assert!(!codex_panel.contains("\"mcpServers\""));
    assert!(!script.contains("JULIE_WORKSPACE\": \"${workspaceFolder}\""));
}

fn root_package_version() -> String {
    let manifest: toml::Value = read_repo_file("Cargo.toml").parse().unwrap();
    manifest["package"]["version"]
        .as_str()
        .expect("root Cargo.toml package.version should be a string")
        .to_string()
}

fn public_tool_names() -> BTreeSet<String> {
    let handler = read_repo_file("src/handler.rs");
    let tools_mod = read_repo_file("src/handler/tools/mod.rs");
    let mut names = BTreeSet::new();

    for line in handler.lines() {
        let Some(start) = line.find("Self::tool_router_") else {
            continue;
        };
        let after_prefix = &line[start + "Self::tool_router_".len()..];
        let Some(end) = after_prefix.find("()") else {
            continue;
        };
        let name = &after_prefix[..end];
        let module_decl = format!("pub(crate) mod {name};");
        assert!(
            tools_mod.contains(&module_decl),
            "handler router references `{name}`, but src/handler/tools/mod.rs is missing `{module_decl}`"
        );
        let tool_file = read_repo_file(&format!("src/handler/tools/{name}.rs"));
        assert!(
            tool_file.contains(&format!("tool_router_{name}")),
            "src/handler/tools/{name}.rs should define router tool_router_{name}"
        );
        assert!(
            tool_file.contains("#[tool("),
            "src/handler/tools/{name}.rs should expose an rmcp #[tool] method"
        );
        names.insert(name.to_string());
    }

    assert!(!names.is_empty(), "expected at least one public MCP tool");
    names
}

#[test]
fn docs_contract_tests_public_surface_includes_patterns() {
    assert!(public_tool_names().contains("patterns"));
}

#[test]
fn docs_contract_tests_extractor_enrichment_surfaces_are_documented() {
    let readme = read_repo_file("README.md");
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    let dependencies = read_repo_file("docs/DEPENDENCIES.md");
    let extraction = read_repo_file("docs/EXTRACTION_CONTRACT.md");

    for required in [
        "patterns",
        "regions",
        "source_regions",
        "structural_facts",
        "complexity_metrics",
    ] {
        assert!(readme.contains(required), "README missing {required}");
        assert!(
            instructions.contains(required),
            "agent instructions missing {required}"
        );
    }
    assert!(dependencies.contains("julie-extractors v2.34.3"));
    assert!(extraction.contains("Schema version 29"));
}

#[test]
fn docs_contract_tests_agent_instructions_fit_the_server_instruction_budget() {
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    let count = instructions.chars().count();
    assert!(
        count <= 2000,
        "JULIE_AGENT_INSTRUCTIONS.md is {count} characters; the ceiling is 2000"
    );
    for name in [
        "fast_search",
        "get_symbols",
        "deep_dive",
        "fast_refs",
        "call_path",
        "get_context",
        "blast_radius",
        "patterns",
        "edit_file",
        "manage_workspace",
        "regions",
        "source_regions",
        "structural_facts",
        "complexity_metrics",
    ] {
        assert!(
            instructions.contains(&format!("`{name}`")),
            "instructions must name {name}"
        );
    }
}

#[test]
fn docs_contract_tests_instructions_carry_the_full_guidance() {
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    assert!(
        instructions.len() < 2000,
        "instructions are {} bytes; the ceiling is 2000",
        instructions.len()
    );
    assert!(instructions.contains("## Workflows"));
    for name in [
        "fast_search",
        "get_symbols",
        "deep_dive",
        "fast_refs",
        "call_path",
        "get_context",
        "blast_radius",
        "patterns",
        "edit_file",
        "manage_workspace",
        "offset",
        "dry_run",
    ] {
        assert!(
            instructions.contains(name),
            "instructions must mention {name}"
        );
    }
    let hooks: serde_json::Value =
        serde_json::from_str(&read_repo_file(".claude/hooks/hooks.json")).unwrap();
    assert!(
        hooks["hooks"]["SessionStart"].is_array(),
        "hooks.json registers SessionStart"
    );
}

fn extract_tool_names(section: &str) -> BTreeSet<String> {
    let marker = "class=\"tool-name\">";
    let mut names = BTreeSet::new();
    let mut rest = section;

    while let Some(start) = rest.find(marker) {
        let after_marker = &rest[start + marker.len()..];
        let end = after_marker
            .find("</div>")
            .expect("tool-name div should close");
        names.insert(after_marker[..end].trim().to_string());
        rest = &after_marker[end..];
    }

    assert!(!names.is_empty(), "tools section should render tool cards");
    names
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_file(relative_path)).unwrap()
}

fn extract_section(contents: &str, id: &str) -> String {
    let marker = format!("id=\"{id}\"");
    let start = contents
        .find(&marker)
        .unwrap_or_else(|| panic!("missing section marker `{marker}`"));
    let after_start = &contents[start..];
    let end = after_start
        .find("</section>")
        .unwrap_or_else(|| panic!("missing section close for `{id}`"));
    after_start[..end].to_string()
}

fn repo_file(relative_path: &str) -> PathBuf {
    workspace_root().join(relative_path)
}
