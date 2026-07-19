use super::*;
use crate::database::StructuralFactQuery;
use julie_extractors::base::SourceRegionKind;

#[tokio::test]
async fn extract_scan_persists_v2_16_enrichment_domains() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::copy(
        "fixtures/extraction/consumer-upgrade/rust_http_client.rs",
        root.join("rust_http_client.rs"),
    )
    .expect("copy Rust enrichment fixture");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root, false))
        .await
        .expect("scan succeeds");
    assert_eq!(report.files_scanned, 1);

    let db = SymbolDatabase::new(&db_path).expect("open db");
    let regions = db
        .get_source_regions_for_file("rust_http_client.rs", &[])
        .expect("read source regions");
    assert!(
        regions
            .iter()
            .any(|region| region.kind == SourceRegionKind::DocComment)
    );

    let facts = db
        .search_structural_facts(&StructuralFactQuery {
            pattern_ids: vec!["http.client_request.v1".into()],
            ..Default::default()
        })
        .expect("read structural facts");
    assert!(facts.iter().any(|fact| {
        fact.metadata
            .as_ref()
            .is_some_and(|metadata| metadata.get("client") == Some(&serde_json::json!("reqwest")))
    }));

    let symbol = db
        .find_symbols_by_name("fetch_user")
        .expect("find fetch_user")
        .into_iter()
        .next()
        .expect("fetch_user symbol");
    let metric = db
        .get_complexity_metric_for_symbol(&symbol.id)
        .expect("read complexity metric")
        .expect("fetch_user complexity metric");
    assert!(metric.decision_count >= 1);
    assert!(metric.loop_count >= 1);
    assert_eq!(metric.parameter_count, Some(2));
}

#[tokio::test]
async fn extract_scan_persists_polyglot_type_arguments() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");

    // C#: a generic field — IList<RootObject> is one outermost use site.
    fs::write(
        root.join("poly.cs"),
        "public class Repo { public IList<RootObject> Items; }\n",
    )
    .expect("write c#");
    // TypeScript: a heritage clause generic Base<Foo, Bar> (ordered pair).
    fs::write(root.join("poly.ts"), "class A extends Base<Foo, Bar> {}\n").expect("write ts");
    // Python: a nested annotation Dict[str, List[User]] — Dict outermost,
    // List[User] nested under ordinal 1.
    fs::write(
        root.join("poly.py"),
        "class User: pass\nx: Dict[str, List[User]] = {}\n",
    )
    .expect("write py");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");
    assert_eq!(report.files_scanned, 3, "three source files scanned");
    assert!(
        report.type_arguments_total >= 3,
        "report must surface the persisted type-argument count, got {}",
        report.type_arguments_total
    );

    let db = SymbolDatabase::new(&db_path).expect("open db");

    // Each language's reader fired through the real pipeline.
    assert!(
        count_rows_where(&db, "type_arguments", "file_path = 'poly.cs'") >= 1,
        "C# IList<RootObject> must persist a type-argument row"
    );
    assert!(
        count_rows_where(&db, "type_arguments", "file_path = 'poly.ts'") >= 2,
        "TS Base<Foo, Bar> must persist two ordered type-argument rows"
    );
    assert!(
        count_rows_where(&db, "type_arguments", "file_path = 'poly.py'") >= 1,
        "Python Dict[str, List[User]] must persist type-argument rows"
    );

    // C# content: the single ordered argument is RootObject.
    assert_eq!(
        count_rows_where(
            &db,
            "type_arguments",
            "file_path = 'poly.cs' AND type_name = 'RootObject'"
        ),
        1,
        "the C# row's type_name must be the applied type RootObject"
    );

    // Python nesting survives the full path: the `User` row sits under the
    // `List` row via parent_arg_id (Dict's ordinal-1 nested argument).
    let list_id: String = db
        .conn
        .query_row(
            "SELECT id FROM type_arguments WHERE file_path = 'poly.py' AND type_name = 'List'",
            [],
            |row| row.get(0),
        )
        .expect("nested List row exists");
    let user_parent: Option<String> = db
        .conn
        .query_row(
            "SELECT parent_arg_id FROM type_arguments WHERE file_path = 'poly.py' AND type_name = 'User'",
            [],
            |row| row.get(0),
        )
        .expect("nested User row exists");
    assert_eq!(
        user_parent.as_deref(),
        Some(list_id.as_str()),
        "nested User must point at its List parent after the full persist path"
    );

    // The language column is distinctly labeled per language (three readers).
    let distinct_langs: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(DISTINCT language) FROM type_arguments",
            [],
            |row| row.get(0),
        )
        .expect("count distinct languages");
    assert_eq!(
        distinct_langs, 3,
        "C#, TypeScript, and Python rows must each carry their own language label"
    );
}

/// End-to-end on the path Miller reads: a real scan must capture string-literal
/// call-args, classify them by the config carrier gate, DROP non-carrier
/// literals, and land url/sql rows in the `literals` table — proving the full
/// extractor → ExtractedBatch → classify_literals_by_carrier → CanonicalWriteSet
/// → insert path (Miller bridge Phase 3). Also guards the two negatives the
/// design hinges on: the gate drops non-carrier callees (`console.log`,
/// `Console.WriteLine`), and literal text never leaks into the name-indexed
/// `identifiers` table.
#[tokio::test]
async fn extract_scan_persists_gated_url_and_sql_literals() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");

    // TS: two URL carriers (fetch, axios.get), one local-receiver SQL carrier
    // (pool.query — matched by the gate's last-segment rule against bare
    // `query`), and one non-carrier (console.log).
    fs::write(
        root.join("http.ts"),
        "async function load(pool: any) {\n\
         \x20 await fetch(\"/api/users\");\n\
         \x20 await axios.get(\"/api/orders\");\n\
         \x20 await pool.query(\"SELECT token FROM Sessions\");\n\
         \x20 console.log(\"ignored debug line\");\n\
         }\n",
    )
    .expect("write ts");
    // C#: one SQL carrier (Dapper Query) + one non-carrier (Console.WriteLine).
    fs::write(
        root.join("repo.cs"),
        "public class Repo {\n\
         \x20 public void Load(System.Data.IDbConnection conn) {\n\
         \x20   conn.Query<User>(\"SELECT Id, Name FROM Users\");\n\
         \x20   System.Console.WriteLine(\"ignored log message\");\n\
         \x20 }\n\
         }\n\
         public class User {}\n",
    )
    .expect("write c#");
    // Python: one URL carrier (requests.get), one local-receiver SQL carrier
    // (cursor.execute), one non-carrier (print) — proves a third language flows
    // through the same language-agnostic chokepoint end-to-end.
    fs::write(
        root.join("api.py"),
        "def load(cursor):\n\
         \x20   requests.get(\"https://svc/api/items\")\n\
         \x20   cursor.execute(\"SELECT id FROM Items\")\n\
         \x20   print(\"ignored py message\")\n",
    )
    .expect("write py");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");
    assert_eq!(report.files_scanned, 3, "three source files scanned");
    assert_eq!(
        report.literals_total, 6,
        "report must surface exactly the 6 carrier-gated literals (2 ts url + 1 ts sql + 1 c# sql + 1 py url + 1 py sql), got {}",
        report.literals_total
    );

    let db = SymbolDatabase::new(&db_path).expect("open db");

    // TS URL leg: fetch + axios.get both captured and classified url.
    assert_eq!(
        count_rows_where(&db, "literals", "file_path = 'http.ts' AND kind = 'url'"),
        2,
        "TS fetch + axios.get must persist two url literals"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'http.ts' AND kind = 'url' AND carrier = 'fetch' AND literal_text = '/api/users'"
        ),
        1,
        "the fetch literal must carry its decoded text and verbatim carrier"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'http.ts' AND carrier = 'axios.get' AND literal_text = '/api/orders'"
        ),
        1,
        "the dotted axios.get carrier must be preserved verbatim"
    );

    // TS SQL leg (local-receiver): pool.query("SELECT ...") -> the gate's
    // last-segment rule matches the bare `query` carrier config, so the local
    // DB receiver is captured as sql without enumerating the variable name.
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'http.ts' AND kind = 'sql' AND carrier = 'pool.query' AND literal_text LIKE '%FROM Sessions%'"
        ),
        1,
        "TS local-receiver pool.query must persist one sql literal via last-segment carrier matching"
    );

    // C# SQL leg: Dapper Query captured and classified sql.
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'repo.cs' AND kind = 'sql' AND carrier = 'Query'"
        ),
        1,
        "C# Dapper Query must persist one sql literal with method-name carrier"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'repo.cs' AND literal_text LIKE '%FROM Users%'"
        ),
        1,
        "the C# sql literal must carry the decoded SQL body"
    );

    // Python leg: requests.get (url, dotted carrier) + cursor.execute (sql,
    // local-receiver matched by last segment).
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'api.py' AND kind = 'url' AND carrier = 'requests.get' AND literal_text = 'https://svc/api/items'"
        ),
        1,
        "Python requests.get must persist one url literal with its dotted carrier"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'api.py' AND kind = 'sql' AND carrier = 'cursor.execute' AND literal_text LIKE '%FROM Items%'"
        ),
        1,
        "Python cursor.execute must persist one sql literal via last-segment carrier matching"
    );

    // The gate dropped every non-carrier callee: no literal from console.log /
    // Console.WriteLine / print survives.
    assert_eq!(
        count_rows_where(&db, "literals", "literal_text LIKE '%ignored%'"),
        0,
        "non-carrier (console.log / Console.WriteLine / print) literals must be dropped by the gate"
    );

    // Name-leak negative: URLs/SQL must NOT enter the name-indexed identifiers
    // table (that would pollute fast-refs name matching and skew centrality).
    assert_eq!(
        count_rows_where(&db, "identifiers", "name = '/api/users'"),
        0,
        "URL text must not leak into the identifiers name index"
    );
    assert_eq!(
        count_rows_where(&db, "identifiers", "name LIKE '%SELECT%'"),
        0,
        "SQL text must not leak into the identifiers name index"
    );
}

/// End-to-end coverage for the STRUCTURALLY-DISTINCT capture arms the
/// call/invocation/member test above can't reach: the command grammar (Bash and
/// PowerShell — carrier is the command/cmdlet name, args are command elements,
/// not a `call_expression`) and the Rust macro arm (`sqlx::query!` — a
/// `macro_invocation` token-tree, not a call). Proves all three flow through the
/// same extractor → classify_literals_by_carrier → persist path, that the gate
/// matches command-name and cmdlet carriers (case-insensitively for PowerShell's
/// PascalCase cmdlets, while the verbatim carrier is still stored), and that
/// non-carrier commands/macros (`echo`, `Write-Host`, `println!`) are dropped.
#[tokio::test]
async fn extract_scan_persists_command_grammar_and_macro_literals() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");

    // Bash command grammar: curl (url) + psql -c (sql) carriers; echo dropped.
    fs::write(
        root.join("deploy.sh"),
        "deploy() {\n\
         \x20   curl \"https://deploy.example.com/api\"\n\
         \x20   psql -c \"SELECT id FROM jobs\"\n\
         \x20   echo \"ignored shell message\"\n\
         }\n",
    )
    .expect("write bash");
    // PowerShell command grammar: Invoke-RestMethod (url) + Invoke-Sqlcmd (sql)
    // via named params; Write-Host dropped. Cmdlets are PascalCase — the gate
    // must match case-insensitively while persisting the verbatim carrier.
    fs::write(
        root.join("run.ps1"),
        "function Sync-Data {\n\
         \x20   Invoke-RestMethod -Uri \"https://ps.example.com/api\"\n\
         \x20   Invoke-Sqlcmd -Query \"SELECT id FROM runs\"\n\
         \x20   Write-Host \"ignored ps message\"\n\
         }\n",
    )
    .expect("write powershell");
    // Rust macro arm: sqlx::query! (sql, last-segment carrier `query`); the
    // non-carrier println! macro must be captured config-free then dropped.
    fs::write(
        root.join("queries.rs"),
        "async fn load() {\n\
         \x20   sqlx::query!(\"SELECT id FROM accounts\");\n\
         \x20   println!(\"ignored rust message\");\n\
         }\n",
    )
    .expect("write rust");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");
    assert_eq!(report.files_scanned, 3, "three source files scanned");
    assert_eq!(
        report.literals_total, 5,
        "exactly 5 carrier-gated literals (bash url+sql, ps url+sql, rust macro sql); got {}",
        report.literals_total
    );

    let db = SymbolDatabase::new(&db_path).expect("open db");

    // Bash: command-name carriers.
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'deploy.sh' AND kind = 'url' AND carrier = 'curl' AND literal_text = 'https://deploy.example.com/api'"
        ),
        1,
        "bash curl must persist a url literal with command-name carrier"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'deploy.sh' AND kind = 'sql' AND carrier = 'psql' AND literal_text LIKE '%FROM jobs%'"
        ),
        1,
        "bash psql -c must persist a sql literal"
    );

    // PowerShell: cmdlet carriers matched case-insensitively, stored verbatim.
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'run.ps1' AND kind = 'url' AND carrier = 'Invoke-RestMethod' AND literal_text = 'https://ps.example.com/api'"
        ),
        1,
        "PowerShell Invoke-RestMethod must persist a url literal with the verbatim PascalCase carrier"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'run.ps1' AND kind = 'sql' AND carrier = 'Invoke-Sqlcmd' AND literal_text LIKE '%FROM runs%'"
        ),
        1,
        "PowerShell Invoke-Sqlcmd must persist a sql literal"
    );

    // Rust macro arm: sqlx::query! captured via the macro token-tree, carrier is
    // the bare last segment `query`, matched against the sql set.
    assert_eq!(
        count_rows_where(
            &db,
            "literals",
            "file_path = 'queries.rs' AND kind = 'sql' AND carrier = 'query' AND literal_text LIKE '%FROM accounts%'"
        ),
        1,
        "Rust sqlx::query! macro must persist a sql literal with last-segment carrier"
    );

    // The gate dropped every non-carrier command/macro callee.
    assert_eq!(
        count_rows_where(&db, "literals", "literal_text LIKE '%ignored%'"),
        0,
        "non-carrier echo / Write-Host / println! literals must be dropped by the gate"
    );
}

/// Phase 1 of the test-role enrichment plan: the extract DB Miller reads must
/// carry `symbols.metadata.test_role` (and `is_test`) for every test symbol the
/// classifier recognizes — INCLUDING class/struct containers (`[TestClass]`),
/// which the per-extractor `is_test` (callable-only) can never flag.
///
/// This is the test that proves the WIRING: `classify_symbols_by_role` runs on
/// the extract-CLI persistence path, not only the live daemon pipeline. A unit
/// test on `classify_symbols_by_role` alone would NOT catch the broken path —
/// this reads the DB `run_external_scan` writes. RED before wiring: the extract
/// path never classified, so every `test_role` is NULL and test classes have no
/// `is_test`.
#[tokio::test]
async fn extract_scan_persists_test_role_metadata_across_languages() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");

    // C# MSTest: [TestClass] class (container), [TestInitialize] (fixture_setup),
    // [TestMethod] (test_case). Plus a production class + method that must stay
    // unclassified. Container + fixture come ONLY from the annotation classifier
    // (the callable-only per-extractor is_test cannot flag a class).
    fs::write(
        root.join("CalculatorTests.cs"),
        "using Microsoft.VisualStudio.TestTools.UnitTesting;\n\
         \n\
         [TestClass]\n\
         public class CalculatorTests {\n\
         \x20   [TestInitialize]\n\
         \x20   public void Setup() { }\n\
         \n\
         \x20   [TestMethod]\n\
         \x20   public void AddsNumbers() { }\n\
         }\n\
         \n\
         public class Calculator {\n\
         \x20   public int Add(int a, int b) { return a + b; }\n\
         }\n",
    )
    .expect("write csharp");
    // Python pytest: @pytest.fixture (fixture_setup via annotation), a
    // convention-named test_* function (test_case via the is_test fallback), a
    // unittest.TestCase subclass (test_container via the base-type rule — no
    // annotation), and a plain helper that must stay unclassified.
    fs::write(
        root.join("test_calc.py"),
        "import pytest\n\
         import unittest\n\
         \n\
         @pytest.fixture\n\
         def db():\n\
         \x20   return {}\n\
         \n\
         def test_adds():\n\
         \x20   assert 1 + 1 == 2\n\
         \n\
         class CalcTestCase(unittest.TestCase):\n\
         \x20   def test_add(self):\n\
         \x20       self.assertEqual(1 + 1, 2)\n\
         \n\
         def helper():\n\
         \x20   return 0\n",
    )
    .expect("write python");
    // C++ GoogleTest: `TEST(...)` (test_case) and `TEST_P(...)` (parameterized_test)
    // come from the macro-synthesized annotation keys (`test`/`test_p`) classified
    // against cpp.toml — NOT a structural is_test, which would collapse both to
    // test_case. The fixture class `: public ::testing::TestWithParam<int>` is a
    // test_container via the base-type rule (last segment `TestWithParam`). A plain
    // class + method must stay unclassified. Path-independent (annotation/base-type
    // driven), so the file can live at the repo root.
    fs::write(
        root.join("calc_test.cpp"),
        "#include <gtest/gtest.h>\n\
         \n\
         TEST(CalculatorTest, AddsNumbers) {\n\
         \x20   EXPECT_EQ(2 + 2, 4);\n\
         }\n\
         \n\
         class CalculatorFixture : public ::testing::TestWithParam<int> {\n\
         };\n\
         \n\
         TEST_P(CalculatorFixture, HandlesValues) {\n\
         \x20   EXPECT_GE(GetParam(), 0);\n\
         }\n\
         \n\
         class RealCalculator {\n\
         public:\n\
         \x20   int Compute() { return 0; }\n\
         };\n",
    )
    .expect("write cpp");
    // Swift XCTest: a class extending `XCTestCase` is a test_container via the
    // base-type rule (no annotation; matched on recorded `base_types`). The
    // `test`-prefixed method is a test_case via the is_test fallback (needs a test
    // path, hence the `Tests/` directory). A non-test free function stays
    // unclassified even inside the test path.
    let swift_dir = root.join("Tests");
    std::fs::create_dir(&swift_dir).expect("swift Tests dir");
    fs::write(
        swift_dir.join("MathTests.swift"),
        "import XCTest\n\
         \n\
         class MathTests: XCTestCase {\n\
         \x20   func testAdds() {\n\
         \x20       XCTAssertEqual(2 + 2, 4)\n\
         \x20   }\n\
         }\n\
         \n\
         func mathHelper() -> Int {\n\
         \x20   return 0\n\
         }\n",
    )
    .expect("write swift");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");
    assert_eq!(report.files_scanned, 4, "four source files scanned");

    let db = SymbolDatabase::new(&db_path).expect("open db");

    // C# test CLASS -> test_container AND is_test (the previously-missing signal).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'CalculatorTests' AND file_path = 'CalculatorTests.cs' \
             AND json_extract(metadata,'$.test_role') = 'test_container' \
             AND json_extract(metadata,'$.is_test') = 1"
        ),
        1,
        "[TestClass] class must persist test_role=test_container and is_test=true"
    );
    // [TestInitialize] method -> fixture_setup.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'Setup' AND file_path = 'CalculatorTests.cs' \
             AND json_extract(metadata,'$.test_role') = 'fixture_setup'"
        ),
        1,
        "[TestInitialize] method must persist test_role=fixture_setup"
    );
    // [TestMethod] method -> test_case.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'AddsNumbers' AND file_path = 'CalculatorTests.cs' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "[TestMethod] method must persist test_role=test_case"
    );
    // Python @pytest.fixture -> fixture_setup (annotation-driven).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'db' AND file_path = 'test_calc.py' \
             AND json_extract(metadata,'$.test_role') = 'fixture_setup'"
        ),
        1,
        "@pytest.fixture function must persist test_role=fixture_setup"
    );
    // Python convention test_* -> test_case (is_test fallback path).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'test_adds' AND file_path = 'test_calc.py' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "convention-named test_adds must persist test_role=test_case"
    );
    // Python unittest.TestCase subclass -> test_container via the base-type rule
    // (no annotation; matched on the recorded `superclasses` base type).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'CalcTestCase' AND file_path = 'test_calc.py' \
             AND json_extract(metadata,'$.test_role') = 'test_container' \
             AND json_extract(metadata,'$.is_test') = 1"
        ),
        1,
        "unittest.TestCase subclass must persist test_role=test_container via base-type rule"
    );
    // C++ GoogleTest TEST(...) -> test_case (synthesized `test` annotation).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'CalculatorTest.AddsNumbers' AND file_path = 'calc_test.cpp' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "GoogleTest TEST(...) must persist test_role=test_case"
    );
    // C++ GoogleTest TEST_P(...) -> parameterized_test. This is the Option-C payoff:
    // the synthesized `test_p` annotation promotes it ABOVE the structural is_test
    // (which would otherwise collapse it to test_case).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'CalculatorFixture.HandlesValues' AND file_path = 'calc_test.cpp' \
             AND json_extract(metadata,'$.test_role') = 'parameterized_test'"
        ),
        1,
        "GoogleTest TEST_P(...) must persist test_role=parameterized_test (not test_case)"
    );
    // C++ fixture class extending ::testing::TestWithParam -> test_container via the
    // base-type rule (last segment `TestWithParam`, qualified base stripped of template).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'CalculatorFixture' AND file_path = 'calc_test.cpp' \
             AND json_extract(metadata,'$.test_role') = 'test_container' \
             AND json_extract(metadata,'$.is_test') = 1"
        ),
        1,
        "GoogleTest fixture (: ::testing::TestWithParam) must persist test_role=test_container"
    );
    // Swift XCTest class -> test_container via the base-type rule (recorded base_types
    // = ["XCTestCase"]). Second language proving the cross-language base-type rule.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'MathTests' AND file_path = 'Tests/MathTests.swift' \
             AND json_extract(metadata,'$.test_role') = 'test_container' \
             AND json_extract(metadata,'$.is_test') = 1"
        ),
        1,
        "Swift XCTestCase subclass must persist test_role=test_container via base-type rule"
    );
    // Swift test-prefixed method -> test_case via the is_test fallback (test path).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'testAdds' AND file_path = 'Tests/MathTests.swift' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "Swift test-prefixed XCTest method must persist test_role=test_case"
    );

    // Negatives: production symbols get NO test_role and NO is_test.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "file_path = 'CalculatorTests.cs' AND name IN ('Calculator','Add') \
             AND json_extract(metadata,'$.test_role') IS NOT NULL"
        ),
        0,
        "production class/method must not be classified as tests"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'helper' AND file_path = 'test_calc.py' \
             AND json_extract(metadata,'$.test_role') IS NOT NULL"
        ),
        0,
        "a plain helper in a test file must not be classified"
    );
    // C++ production class/method (no test base type, no test macro) stays unclassified.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "file_path = 'calc_test.cpp' AND name IN ('RealCalculator','Compute') \
             AND json_extract(metadata,'$.test_role') IS NOT NULL"
        ),
        0,
        "C++ production class/method must not be classified as tests"
    );
    // Swift non-test free function inside a test path stays unclassified.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'mathHelper' AND file_path = 'Tests/MathTests.swift' \
             AND json_extract(metadata,'$.test_role') IS NOT NULL"
        ),
        0,
        "a non-test Swift free function must not be classified"
    );
}

/// Call-style frameworks (tests are call expressions, not named functions or
/// annotated methods) persist test_role end-to-end across C/C++/Lua/R/Elixir.
/// These ride the shared `test_calls` core (C/C++/Lua/R) or a bespoke extractor
/// (Elixir); all are config-free, so classification flows through the call-style
/// container lever (test_container -> TestContainer) and the is_test fallback
/// (test/case -> test_case). Lifecycle hooks assert only is_test (their precise
/// fixture role is a separate refinement).
#[tokio::test]
async fn extract_scan_persists_call_style_test_roles() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");

    // C Criterion: `Test(suite, name)` parses as a call; name = "suite.name".
    fs::write(
        root.join("calc_criterion.c"),
        "#include <criterion/criterion.h>\n\
         \n\
         Test(math_suite, addition) {\n\
         \x20   cr_assert(2 + 2 == 4);\n\
         }\n",
    )
    .expect("write c");
    // C++ Catch2: TEST_CASE -> test_case, nested SECTION -> test_container.
    fs::write(
        root.join("calc_catch.cpp"),
        "#include <catch2/catch_test_macros.hpp>\n\
         \n\
         TEST_CASE(\"addition works\", \"[math]\") {\n\
         \x20   SECTION(\"positive numbers\") {\n\
         \x20       REQUIRE(2 + 2 == 4);\n\
         \x20   }\n\
         }\n",
    )
    .expect("write cpp");
    // Lua busted: describe -> container, it -> case, before_each -> lifecycle.
    fs::write(
        root.join("calc_spec.lua"),
        "describe(\"math\", function()\n\
         \x20 before_each(function() end)\n\
         \x20 it(\"adds\", function() assert.equal(2, 1 + 1) end)\n\
         end)\n",
    )
    .expect("write lua");
    // R testthat: test_that -> case, describe -> container, nested it -> case.
    fs::write(
        root.join("calc_test.R"),
        "test_that(\"addition works\", {\n\
         \x20 expect_equal(1 + 1, 2)\n\
         })\n\
         describe(\"a widget\", {\n\
         \x20 it(\"renders\", {\n\
         \x20   expect_true(TRUE)\n\
         \x20 })\n\
         })\n",
    )
    .expect("write r");
    // Elixir ExUnit: describe -> container, test -> case, setup -> lifecycle.
    fs::write(
        root.join("calc_test.exs"),
        "defmodule CalcTest do\n\
         \x20 use ExUnit.Case\n\
         \n\
         \x20 describe \"addition\" do\n\
         \x20   setup do\n\
         \x20     :ok\n\
         \x20   end\n\
         \n\
         \x20   test \"adds two numbers\" do\n\
         \x20     assert 1 + 1 == 2\n\
         \x20   end\n\
         \x20 end\n\
         end\n",
    )
    .expect("write elixir");

    let db_path = tmp.path().join("external.sqlite");
    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");
    assert_eq!(
        report.files_scanned, 5,
        "five call-style source files scanned"
    );

    let db = SymbolDatabase::new(&db_path).expect("open db");

    // C Criterion Test(suite, name) -> test_case named "suite.name".
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'math_suite.addition' AND file_path = 'calc_criterion.c' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "Criterion Test(suite,name) must persist test_role=test_case named suite.name"
    );
    // C++ Catch2 TEST_CASE -> test_case.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'addition works' AND file_path = 'calc_catch.cpp' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "Catch2 TEST_CASE must persist test_role=test_case"
    );
    // C++ Catch2 SECTION -> test_container (via the call-style container lever).
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'positive numbers' AND file_path = 'calc_catch.cpp' \
             AND json_extract(metadata,'$.test_role') = 'test_container'"
        ),
        1,
        "Catch2 SECTION must persist test_role=test_container"
    );
    // Lua busted describe -> test_container, it -> test_case, before_each -> is_test.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'math' AND file_path = 'calc_spec.lua' \
             AND json_extract(metadata,'$.test_role') = 'test_container'"
        ),
        1,
        "busted describe must persist test_role=test_container"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'adds' AND file_path = 'calc_spec.lua' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "busted it must persist test_role=test_case"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'before_each' AND file_path = 'calc_spec.lua' \
             AND json_extract(metadata,'$.is_test') = 1"
        ),
        1,
        "busted before_each lifecycle hook must persist is_test=true"
    );
    // R testthat test_that -> test_case, describe -> test_container, it -> test_case.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'addition works' AND file_path = 'calc_test.R' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "testthat test_that must persist test_role=test_case"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'a widget' AND file_path = 'calc_test.R' \
             AND json_extract(metadata,'$.test_role') = 'test_container'"
        ),
        1,
        "testthat describe must persist test_role=test_container"
    );
    // Elixir ExUnit describe -> test_container, test -> test_case, setup -> is_test.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'addition' AND file_path = 'calc_test.exs' \
             AND json_extract(metadata,'$.test_role') = 'test_container'"
        ),
        1,
        "ExUnit describe must persist test_role=test_container"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'adds two numbers' AND file_path = 'calc_test.exs' \
             AND json_extract(metadata,'$.test_role') = 'test_case'"
        ),
        1,
        "ExUnit test must persist test_role=test_case"
    );
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name = 'setup' AND file_path = 'calc_test.exs' \
             AND json_extract(metadata,'$.is_test') = 1 \
             AND json_extract(metadata,'$.test_lifecycle') = 1"
        ),
        1,
        "ExUnit setup hook must persist is_test=true and test_lifecycle=true"
    );

    // Negative: assertion-style calls (cr_assert/REQUIRE/expect_equal/assert.equal)
    // must NOT become test symbols.
    assert_eq!(
        count_rows_where(
            &db,
            "symbols",
            "name IN ('cr_assert','REQUIRE','expect_equal','expect_true','assert') \
             AND json_extract(metadata,'$.test_role') IS NOT NULL"
        ),
        0,
        "assertion-helper calls must not be classified as tests"
    );
}
