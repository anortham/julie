use anyhow::{Context, Result};
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context_from_files;
use crate::tools::BlastRadiusTool;

pub(super) struct Corpus {
    pub symbols: Vec<CorpusSymbol>,
    pub cases: Vec<CorpusCase>,
}

pub(super) struct CorpusSymbol {
    pub id: &'static str,
    pub name: &'static str,
    pub file_path: &'static str,
}

pub(super) struct CorpusCase {
    pub id: &'static str,
    pub seed: &'static str,
    pub max_depth: u32,
    pub expected_default: Vec<String>,
    pub expected_web: Vec<String>,
}

pub(super) struct CorpusFixture {
    _tree: TempDir,
    pub corpus: Corpus,
    pub context: FakeToolContext,
}

const SOURCES: &[(&str, &str)] = &[
    (
        "backend/save_user.php",
        "<?php\nfunction saveUser(int $id) { return $id; }\n",
    ),
    (
        "backend/show_user.php",
        "<?php\nuse Symfony\\Component\\Routing\\Attribute\\Route;\nclass UserController {\n    #[Route('/api/users/{id}', methods: ['GET'])]\n    public function showUser(int $id) { return saveUser($id); }\n}\n",
    ),
    (
        "web/fetch_user.ts",
        "export async function fetchUser() {\n  const r = await fetch(\"/api/users/42\");\n  return r.json();\n}\n",
    ),
    (
        "web/page_loader.vue",
        "<script>\nimport { fetchUser } from \"./fetch_user\";\nexport function pageLoader() { return fetchUser(); }\n</script>\n",
    ),
    (
        "web/fetch_unknown.ts",
        "export async function fetchUnknown() {\n  const r = await fetch(\"/api/ambiguous\");\n  return r.json();\n}\n",
    ),
    (
        "web/unknown_page.vue",
        "<script>\nimport { fetchUnknown } from \"./fetch_unknown\";\nexport function unknownPage() { return fetchUnknown(); }\n</script>\n",
    ),
    (
        "cycles/cycle_a.rs",
        "pub fn cycleA() {\n    saveUser(1);\n    cycleB();\n    cycleA();\n}\n",
    ),
    ("cycles/cycle_b.rs", "pub fn cycleB() {\n    cycleA();\n}\n"),
    (
        "schema/users.sql",
        "CREATE TABLE users (id INT, name TEXT);\n",
    ),
    (
        "reports/load_report.sql",
        "CREATE PROCEDURE loadReport() BEGIN UPDATE users SET name = 'x' WHERE id = 1; END;\n",
    ),
    (
        "reports/load_unknown.sql",
        "CREATE PROCEDURE loadUnknown() BEGIN UPDATE missing_users SET name = 'x' WHERE id = 1; END;\n",
    ),
    (
        "reports/report_page.py",
        "def reportPage():\n    loadReport()\n",
    ),
    (
        "reports/unknown_report_page.py",
        "def unknownReportPage():\n    loadUnknown()\n",
    ),
];

fn symbol(id: &'static str, name: &'static str, file_path: &'static str) -> CorpusSymbol {
    CorpusSymbol {
        id,
        name,
        file_path,
    }
}

fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn load_corpus() -> Corpus {
    Corpus {
        symbols: vec![
            symbol("save_user", "saveUser", "backend/save_user.php"),
            symbol("show_user", "showUser", "backend/show_user.php"),
            symbol("fetch_user", "fetchUser", "web/fetch_user.ts"),
            symbol("page_loader", "Page_loader", "web/page_loader.vue"),
            symbol("cycle_a", "cycleA", "cycles/cycle_a.rs"),
            symbol("cycle_b", "cycleB", "cycles/cycle_b.rs"),
            symbol("fetch_unknown", "fetchUnknown", "web/fetch_unknown.ts"),
            symbol("unknown_page", "Unknown_page", "web/unknown_page.vue"),
            symbol("users_table", "users", "schema/users.sql"),
            symbol("load_report", "loadReport", "reports/load_report.sql"),
            symbol("report_page", "reportPage", "reports/report_page.py"),
            symbol("load_unknown", "loadUnknown", "reports/load_unknown.sql"),
            symbol(
                "unknown_report_page",
                "unknownReportPage",
                "reports/unknown_report_page.py",
            ),
        ],
        cases: vec![
            CorpusCase {
                id: "http_mixed",
                seed: "save_user",
                max_depth: 3,
                expected_default: ids(&["show_user", "cycle_a", "cycle_b"]),
                expected_web: ids(&[
                    "show_user",
                    "cycle_a",
                    "cycle_b",
                    "fetch_user",
                    "page_loader",
                ]),
            },
            CorpusCase {
                id: "sql_mixed",
                seed: "users_table",
                max_depth: 2,
                expected_default: Vec::new(),
                expected_web: ids(&["load_report", "report_page"]),
            },
        ],
    }
}

impl Corpus {
    pub fn case(&self, id: &str) -> Result<&CorpusCase> {
        self.cases
            .iter()
            .find(|case| case.id == id)
            .with_context(|| format!("missing corpus case {id}"))
    }

    pub fn symbol(&self, id: &str) -> Result<&CorpusSymbol> {
        self.symbols
            .iter()
            .find(|symbol| symbol.id == id)
            .with_context(|| format!("missing corpus symbol {id}"))
    }
}

pub(super) fn build_fixture() -> Result<CorpusFixture> {
    let (tree, context) = snapshot_context_from_files(SOURCES)?;
    Ok(CorpusFixture {
        _tree: tree,
        corpus: load_corpus(),
        context,
    })
}

pub(super) async fn call_case(
    fixture: &CorpusFixture,
    case: &CorpusCase,
    mode: Option<&str>,
    max_depth: Option<u32>,
) -> Result<String> {
    let seed = fixture.corpus.symbol(case.seed)?;
    let result = BlastRadiusTool {
        symbol_ids: vec![seed.name.to_string()],
        max_depth: max_depth.unwrap_or(case.max_depth),
        limit: 100,
        include_tests: false,
        mode: mode.map(str::to_string),
        ..Default::default()
    }
    .call_tool(&fixture.context)
    .await?;
    Ok(call_tool_result_text(&result))
}
