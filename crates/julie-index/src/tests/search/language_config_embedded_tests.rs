use crate::search::language_config::{LanguageConfig, LanguageConfigs};

/// Verify that ALL 34 embedded language configs parse successfully.
///
/// With the old warn+skip behavior, a broken TOML would silently reduce the
/// count. With the new panic behavior, this test documents that all configs
/// are present and valid. If this count fails, check that a new language was
/// added to the embedded list in `load_embedded` and its .toml parses cleanly.
///
/// 34 == every supported language has an embedded config. VB.NET was the last
/// holdout (added with the Phase 3b literal-carrier work): without an embedded
/// config its `[literal_carriers]` never loaded, so the gate silently dropped
/// every VB.NET literal. Keeping this at the full language count guarantees no
/// language ships a TOML on disk that the runtime never loads.
#[test]
fn test_all_embedded_language_configs_load_without_skips() {
    let configs = LanguageConfigs::load_embedded();
    assert_eq!(
        configs.len(),
        34,
        "Expected exactly 34 embedded language configs, got {}. \
             A broken TOML or missing entry would cause this count to be wrong.",
        configs.len()
    );
}

#[test]
fn test_literal_carrier_configs_loaded_and_lowercased_for_reference_legs() {
    let configs = LanguageConfigs::load_embedded();
    let carriers = configs.build_literal_carrier_configs();

    let ts = carriers
        .get("typescript")
        .expect("typescript carrier config");
    assert!(
        ts.url.contains("fetch") && ts.url.contains("axios.get"),
        "TS url carriers must include fetch + axios.get; got {:?}",
        ts.url
    );
    // TS SQL via local-receiver is now matched by the gate's last-segment
    // rule against bare method names, so `pool.query`/`db.execute` are
    // recognized without enumerating the variable receiver.
    assert!(
        ts.sql.contains("query") && ts.sql.contains("execute"),
        "TS sql carriers must include bare query + execute for local-receiver matching; got {:?}",
        ts.sql
    );

    // TS↔JS HTTP-client parity: a `.ts` file imports the same npm clients as
    // a `.js` file, so TypeScript's url carriers MUST be a superset of
    // JavaScript's. This guards the ky/got/ofetch backport (added to JS/Vue
    // first) from silently regressing TS out of parity with its own ecosystem.
    let js = carriers
        .get("javascript")
        .expect("javascript carrier config");
    for carrier in &js.url {
        assert!(
            ts.url.contains(carrier.as_str()),
            "TS url carriers must be a superset of JS (shared npm ecosystem); \
                 missing {carrier:?}. TS url = {:?}",
            ts.url
        );
    }
    // Spot-check the specific HTTP clients backported for parity.
    for client in ["ky", "ky.get", "got", "got.get", "ofetch"] {
        assert!(
            ts.url.contains(client),
            "TS url carriers must include backported HTTP client {client:?}; got {:?}",
            ts.url
        );
    }

    let cs = carriers.get("csharp").expect("csharp carrier config");
    // Stored lowercase for case-insensitive matching even though the TOML
    // could be written either case.
    assert!(
        cs.sql.contains("query") && cs.sql.contains("executeasync"),
        "C# sql carriers must include query + executeasync (lowercased); got {:?}",
        cs.sql
    );
    assert!(
        cs.url.contains("getasync"),
        "C# url carriers must include getasync; got {:?}",
        cs.url
    );

    // Breadth guard (Phase 3b): a capture arm without an embedded config is
    // dead — the extractor emits literals but the gate has no carrier
    // vocabulary, so it drops them all. Assert a representative language per
    // grammar family actually loaded its carriers. VB.NET is explicit here
    // because it was the one language missing from `load_embedded`; this
    // assertion is the regression guard for that class of gap.
    for (lang, url_probe, sql_probe) in [
        ("vbnet", "getasync", "query"),       // .NET invocation family
        ("go", "http.get", "query"),          // call_expression family
        ("ruby", "net::http.get", "execute"), // call family
        ("python", "requests.get", "execute"),
        ("kotlin", "url", "executequery"),
        ("lua", "http.request", "execute"), // function_call family
        ("bash", "curl", "psql"),           // command grammar (command-name carrier)
        ("powershell", "invoke-restmethod", "invoke-sqlcmd"), // command grammar (cmdlet carrier)
    ] {
        let cfg = carriers
            .get(lang)
            .unwrap_or_else(|| panic!("{lang} carrier config must be embedded + loaded"));
        assert!(
            cfg.url.contains(url_probe),
            "{lang} url carriers must include {url_probe:?} (is the TOML embedded in load_embedded?); got {:?}",
            cfg.url
        );
        assert!(
            cfg.sql.contains(sql_probe),
            "{lang} sql carriers must include {sql_probe:?}; got {:?}",
            cfg.sql
        );
    }
}

#[test]
fn test_language_config_defaults_empty_annotation_sections() {
    let config: LanguageConfig = toml::from_str(
        r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []
"#,
    )
    .expect("config without annotation sections should parse");

    assert!(config.annotation_classes.entrypoint.is_empty());
    assert!(config.annotation_classes.auth.is_empty());
    assert!(config.annotation_classes.auth_bypass.is_empty());
    assert!(config.annotation_classes.middleware.is_empty());
    assert!(config.annotation_classes.scheduler.is_empty());
    assert!(config.annotation_classes.test.test_case.is_empty());
    assert!(config.annotation_classes.test.parameterized_test.is_empty());
    assert!(config.annotation_classes.test.fixture_setup.is_empty());
    assert!(config.annotation_classes.test.fixture_teardown.is_empty());
    assert!(config.annotation_classes.test.test_container.is_empty());
    assert!(config.test_evidence.assertion_identifiers.is_empty());
    assert!(config.test_evidence.error_assertion_identifiers.is_empty());
    assert!(config.test_evidence.mock_identifiers.is_empty());
    assert!(config.early_warnings.review_markers.is_empty());
    assert_eq!(config.early_warnings.schema_version, 1);
}

#[test]
fn test_language_config_loads_populated_annotation_sections() {
    let config: LanguageConfig = toml::from_str(
        r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []

[annotation_classes]
entrypoint = ["app.route"]
auth = ["login_required"]
auth_bypass = ["allowanonymous"]
middleware = ["middleware"]
scheduler = ["celery.task"]

[annotation_classes.test]
parameterized_test = ["pytest.mark.parametrize"]
fixture_setup = ["pytest.fixture"]

[early_warnings]
review_markers = ["allowanonymous"]
schema_version = 7
"#,
    )
    .expect("config with annotation sections should parse");

    assert_eq!(config.annotation_classes.entrypoint, vec!["app.route"]);
    assert_eq!(config.annotation_classes.auth, vec!["login_required"]);
    assert_eq!(
        config.annotation_classes.auth_bypass,
        vec!["allowanonymous"]
    );
    assert_eq!(config.annotation_classes.middleware, vec!["middleware"]);
    assert_eq!(config.annotation_classes.scheduler, vec!["celery.task"]);
    assert_eq!(
        config.annotation_classes.test.parameterized_test,
        vec!["pytest.mark.parametrize"]
    );
    assert_eq!(
        config.annotation_classes.test.fixture_setup,
        vec!["pytest.fixture"]
    );
    assert!(config.annotation_classes.test.test_case.is_empty());
    assert!(config.annotation_classes.test.fixture_teardown.is_empty());
    assert!(config.annotation_classes.test.test_container.is_empty());
    assert_eq!(config.early_warnings.review_markers, vec!["allowanonymous"]);
    assert_eq!(config.early_warnings.schema_version, 7);
}

#[test]
fn test_embedded_language_configs_include_expected_annotation_classes() {
    let configs = LanguageConfigs::load_embedded();

    let python = configs.get("python").expect("python config should exist");
    assert!(
        python
            .annotation_classes
            .entrypoint
            .contains(&"app.route".into())
    );
    assert!(
        python
            .annotation_classes
            .auth
            .contains(&"login_required".into())
    );
    assert!(
        python
            .annotation_classes
            .test
            .parameterized_test
            .contains(&"pytest.mark.parametrize".into())
    );
    assert!(
        python
            .annotation_classes
            .test
            .fixture_setup
            .contains(&"pytest.fixture".into())
    );

    let java = configs.get("java").expect("java config should exist");
    assert!(
        java.annotation_classes
            .entrypoint
            .contains(&"getmapping".into())
    );
    assert!(
        java.annotation_classes
            .auth
            .contains(&"preauthorize".into())
    );
    assert!(
        java.annotation_classes
            .auth_bypass
            .contains(&"permitall".into())
    );

    let kotlin = configs.get("kotlin").expect("kotlin config should exist");
    assert!(
        kotlin
            .annotation_classes
            .entrypoint
            .contains(&"getmapping".into())
    );
    assert!(
        kotlin
            .annotation_classes
            .auth
            .contains(&"preauthorize".into())
    );
    assert!(
        kotlin
            .annotation_classes
            .auth_bypass
            .contains(&"permitall".into())
    );

    let csharp = configs.get("csharp").expect("csharp config should exist");
    assert!(
        csharp
            .annotation_classes
            .entrypoint
            .contains(&"httpget".into())
    );
    assert!(csharp.annotation_classes.auth.contains(&"authorize".into()));
    assert!(
        csharp
            .annotation_classes
            .auth_bypass
            .contains(&"allowanonymous".into())
    );

    let typescript = configs
        .get("typescript")
        .expect("typescript config should exist");
    assert!(
        typescript
            .annotation_classes
            .entrypoint
            .contains(&"controller".into())
    );
    assert!(
        typescript
            .annotation_classes
            .entrypoint
            .contains(&"get".into())
    );
    assert!(
        typescript
            .annotation_classes
            .auth
            .contains(&"useguards".into())
    );
    assert!(typescript.annotation_classes.auth_bypass.is_empty());
    assert!(typescript.early_warnings.review_markers.is_empty());
    assert_eq!(typescript.early_warnings.schema_version, 1);

    let javascript = configs
        .get("javascript")
        .expect("javascript config should exist");
    assert!(
        javascript
            .annotation_classes
            .entrypoint
            .contains(&"controller".into())
    );
    assert!(
        javascript
            .annotation_classes
            .auth
            .contains(&"useguards".into())
    );
    assert!(javascript.annotation_classes.auth_bypass.is_empty());

    let rust = configs.get("rust").expect("rust config should exist");
    assert!(rust.annotation_classes.entrypoint.is_empty());
    assert!(
        rust.annotation_classes
            .test
            .test_case
            .contains(&"test".into())
    );
    assert!(
        rust.annotation_classes
            .test
            .test_case
            .contains(&"tokio::test".into())
    );

    // Verify Java has role-classified test annotations
    assert!(
        java.annotation_classes
            .test
            .test_case
            .contains(&"test".into())
    );
    assert!(
        java.annotation_classes
            .test
            .parameterized_test
            .contains(&"parameterizedtest".into())
    );
    assert!(
        java.annotation_classes
            .test
            .fixture_setup
            .contains(&"beforeeach".into())
    );
    assert!(
        java.annotation_classes
            .test
            .fixture_teardown
            .contains(&"aftereach".into())
    );

    // Verify C# has role-classified test annotations
    assert!(
        csharp
            .annotation_classes
            .test
            .test_case
            .contains(&"fact".into())
    );
    assert!(
        csharp
            .annotation_classes
            .test
            .parameterized_test
            .contains(&"theory".into())
    );
    assert!(
        csharp
            .annotation_classes
            .test
            .fixture_setup
            .contains(&"setup".into())
    );
    assert!(
        csharp
            .annotation_classes
            .test
            .test_container
            .contains(&"testfixture".into())
    );
}

#[test]
fn test_evidence_config_defaults_empty_when_absent() {
    let config: LanguageConfig = toml::from_str(
        r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []
"#,
    )
    .expect("config without test_evidence section should parse");

    assert!(config.test_evidence.assertion_identifiers.is_empty());
    assert!(config.test_evidence.error_assertion_identifiers.is_empty());
    assert!(config.test_evidence.mock_identifiers.is_empty());
}

#[test]
fn test_evidence_config_loads_from_toml() {
    let config: LanguageConfig = toml::from_str(
        r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []

[test_evidence]
assertion_identifiers = ["assert_eq", "assert_ne"]
error_assertion_identifiers = ["should_panic"]
mock_identifiers = ["mock", "spy"]
"#,
    )
    .expect("config with test_evidence section should parse");

    assert_eq!(
        config.test_evidence.assertion_identifiers,
        vec!["assert_eq", "assert_ne"]
    );
    assert_eq!(
        config.test_evidence.error_assertion_identifiers,
        vec!["should_panic"]
    );
    assert_eq!(config.test_evidence.mock_identifiers, vec!["mock", "spy"]);
}

#[test]
fn test_evidence_rust_has_assertion_identifiers() {
    let configs = LanguageConfigs::load_embedded();
    let rust = configs.get("rust").expect("rust config should exist");

    assert!(
        rust.test_evidence
            .assertion_identifiers
            .contains(&"assert_eq".into()),
        "Rust test_evidence should contain assert_eq"
    );
    assert!(
        rust.test_evidence
            .assertion_identifiers
            .contains(&"assert_ne".into()),
        "Rust test_evidence should contain assert_ne"
    );
    assert!(
        rust.test_evidence
            .assertion_identifiers
            .contains(&"assert".into()),
        "Rust test_evidence should contain assert"
    );
    assert!(
        rust.test_evidence
            .error_assertion_identifiers
            .contains(&"should_panic".into()),
        "Rust test_evidence should contain should_panic"
    );
    assert!(
        rust.test_evidence.mock_identifiers.is_empty(),
        "Rust test_evidence should have no mock identifiers"
    );
}

#[test]
fn test_evidence_populated_for_all_configured_languages() {
    let configs = LanguageConfigs::load_embedded();

    // Python
    let python = configs.get("python").expect("python config should exist");
    assert!(
        python
            .test_evidence
            .assertion_identifiers
            .contains(&"assertequal".into())
    );
    assert!(
        python
            .test_evidence
            .mock_identifiers
            .contains(&"mock".into())
    );
    assert!(
        python
            .test_evidence
            .error_assertion_identifiers
            .contains(&"assertraises".into())
    );

    // Java
    let java = configs.get("java").expect("java config should exist");
    assert!(
        java.test_evidence
            .assertion_identifiers
            .contains(&"assertequals".into())
    );
    assert!(java.test_evidence.mock_identifiers.contains(&"mock".into()));

    // C#
    let csharp = configs.get("csharp").expect("csharp config should exist");
    assert!(
        csharp
            .test_evidence
            .assertion_identifiers
            .contains(&"equal".into())
    );
    assert!(
        csharp
            .test_evidence
            .error_assertion_identifiers
            .contains(&"throws".into())
    );
    assert!(
        csharp
            .test_evidence
            .mock_identifiers
            .contains(&"mock".into())
    );

    // Kotlin
    let kotlin = configs.get("kotlin").expect("kotlin config should exist");
    assert!(
        kotlin
            .test_evidence
            .assertion_identifiers
            .contains(&"assertequals".into())
    );
    assert!(
        kotlin
            .test_evidence
            .mock_identifiers
            .contains(&"every".into())
    );
}
