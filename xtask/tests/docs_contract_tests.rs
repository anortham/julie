use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use xtask::manifest::TestManifest;
use xtask::workspace_root;

#[test]
fn docs_contract_tests_claude_md_uses_xtask_runner_as_canonical_workflow() {
    let contents = read_repo_file("CLAUDE.md");
    let manifest = load_manifest();

    assert_contains_public_commands(&contents);
    assert!(contents.contains("Use raw cargo filters only to narrow failures"));
    assert_blocked_tier_caveat(&contents, &manifest);
}

#[test]
fn docs_contract_tests_agents_md_promotes_changed_scope_first() {
    let contents = read_repo_file("AGENTS.md");

    assert!(contents.contains("cargo xtask test changed"));
    assert!(contents.contains("cargo xtask test dev"));
    assert!(contents.contains("Use raw cargo filters only to narrow failures"));
    assert!(!contents.contains("cargo test --lib -- --skip search_quality"));
    assert!(!contents.contains("This is the ONLY default. No exceptions."));
}

#[test]
fn docs_contract_tests_cargo_alias_uses_quiet_xtask_runner() {
    let contents = read_repo_file(".cargo/config.toml");

    assert!(contents.contains("xtask = \"run -q -p xtask --\""));
    assert!(contents.contains("xtask-eval = \"run -q -p xtask-eval --\""));
}

#[test]
fn docs_contract_tests_readme_lists_public_xtask_commands() {
    let contents = read_repo_file("README.md");
    let manifest = load_manifest();

    assert_contains_public_commands(&contents);
    assert_blocked_tier_caveat(&contents, &manifest);
}

#[test]
fn docs_contract_tests_verification_ledger_template_is_operational() {
    let contents = read_repo_file("docs/plans/verification-ledger-template.md");

    assert!(contents.contains("## Verification Ledger"));
    assert!(contents.contains("| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |"));
    assert!(contents.contains("empty until a command has actually run"));
    assert!(contents.contains("## Example Rows"));
    assert!(contents.contains("Do not copy them into plan evidence."));
    assert!(contents.contains("cargo nextest run --lib docs_contract_tests_verification_ledger_template_is_operational 2>&1 \\| tail -10"));
    assert!(contents.contains("cargo xtask test changed"));
    assert!(contents.contains("cargo xtask test dogfood"));
    assert!(contents.contains("example-sha"));
    assert!(!contents.contains("TODO"));
    assert!(!contents.contains("TBD"));
    assert!(!contents.contains("fill in later"));
}

#[test]
fn docs_contract_tests_testing_guide_documents_ledger_reuse() {
    let contents = read_repo_file("docs/TESTING_GUIDE.md");

    assert!(contents.contains("docs/plans/verification-ledger-template.md"));
    assert!(contents.contains("same HEAD"));
    assert!(contents.contains("reuse"));
    assert!(contents.contains("expensive gate"));
}

#[test]
fn docs_contract_tests_testing_guide_documents_bucket_command() {
    let contents = read_repo_file("docs/TESTING_GUIDE.md");

    assert!(contents.contains("cargo xtask test bucket <name>"));
    assert!(contents.contains("cargo xtask test inventory --bucket <name>"));
    assert!(contents.contains("cargo xtask test inventory --tier dev"));
    assert!(contents.contains("Inventory is diagnostic evidence"));
    assert!(contents.contains("not a passing test gate"));
}

#[test]
fn docs_contract_tests_agent_docs_stay_in_sync() {
    let agents = read_repo_file("AGENTS.md");
    let claude = read_repo_file("CLAUDE.md");

    for required in [
        "cargo xtask test bucket <name>",
        "cargo xtask test inventory --bucket <name>",
        "Workers run exact tests only",
        "The orchestrating session handles regression checks",
    ] {
        assert!(agents.contains(required), "AGENTS.md missing `{required}`");
        assert!(claude.contains(required), "CLAUDE.md missing `{required}`");
    }
}

#[test]
fn docs_contract_tests_agents_points_to_ledger_template() {
    let agents = read_repo_file("AGENTS.md");
    let claude = read_repo_file("CLAUDE.md");

    for contents in [agents, claude] {
        assert!(contents.contains("docs/plans/verification-ledger-template.md"));
        assert!(contents.contains("Verification Ledger"));
        assert!(contents.contains("### TDD Cycle for All Development"));
        assert!(contents.contains("1. **RED**: Write a failing test first"));
        assert!(contents.contains("2. **GREEN**: Write minimal code to make test pass"));
    }
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

fn assert_contains_public_commands(contents: &str) {
    for command in [
        "cargo xtask test changed",
        "cargo xtask test smoke",
        "cargo xtask test dev",
        "cargo xtask test system",
        "cargo xtask test dogfood",
        "cargo xtask test full",
        "cargo xtask test list",
    ] {
        assert!(
            contents.contains(command),
            "missing public command `{command}`"
        );
    }
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
    assert!(dependencies.contains("julie-extractors v2.16.0"));
    assert!(extraction.contains("Schema version 29"));
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

fn load_manifest() -> TestManifest {
    TestManifest::load(repo_file("xtask/test_tiers.toml")).unwrap()
}

fn assert_blocked_tier_caveat(contents: &str, manifest: &TestManifest) {
    if !manifest.blocked_tiers.is_empty() {
        assert!(contents.contains("green-by-default"));
        assert!(contents.contains("workspace_init"));

        for tier_name in manifest.blocked_tiers.keys() {
            assert!(
                contents.contains(&format!("`{tier_name}`")),
                "missing blocked tier name `{tier_name}` in docs"
            );
        }
    }
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
