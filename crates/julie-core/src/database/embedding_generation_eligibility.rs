//! Embedding eligibility policies for symbols and watcher admission.
//!
//! Aligns the database generation coverage checks with the pipeline's
//! embedding preparation rules: excludes non-embeddable languages (docs/configs),
//! test paths, test metadata annotations, and non-embeddable symbol kinds.

/// Non-embeddable languages (documentation and configs).
pub const NON_EMBEDDABLE_LANGUAGES: &[&str] = &[
    "markdown", "json", "jsonl", "toml", "yaml", "xml", "css", "html", "regex", "sql",
];

/// Default embeddable structural symbol kinds.
pub const DEFAULT_EMBEDDABLE_KINDS: &[&str] = &[
    "function",
    "method",
    "class",
    "struct",
    "interface",
    "trait",
    "enum",
    "type",
    "module",
    "namespace",
    "union",
];

/// Returns true if a path indicates test files or directories.
pub fn is_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    for segment in normalized.split('/') {
        match segment {
            "test" | "tests" | "Test" | "Tests" | "spec" | "Spec" | "__tests__" => return true,
            _ => {}
        }
        if segment.ends_with(".Tests")
            || segment.ends_with(".Test")
            || segment.contains(".Tests.")
            || segment.contains(".Test.")
        {
            return true;
        }
    }

    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
    if file_name.ends_with("_test.go")
        || file_name.ends_with("_test.c")
        || file_name.ends_with("_test.cc")
        || file_name.ends_with("_test.cpp")
    {
        return true;
    }

    let test_spec_extensions = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    for ext in &test_spec_extensions {
        if file_name.ends_with(ext) {
            return true;
        }
    }

    if file_name.starts_with("test_") && file_name.ends_with(".py") {
        return true;
    }

    false
}

/// Returns true if symbols in this language are eligible for semantic embeddings.
pub fn is_embeddable_language(language: &str) -> bool {
    !NON_EMBEDDABLE_LANGUAGES
        .iter()
        .any(|&l| l.eq_ignore_ascii_case(language))
}

/// Returns true if a symbol's JSON metadata indicates test code.
pub fn is_test_metadata(metadata_json: Option<&str>) -> bool {
    let Some(raw) = metadata_json else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return false;
    };
    let Some(map) = val.as_object() else {
        return false;
    };

    if map.contains_key("test_role") {
        return true;
    }

    map.get("is_test")
        .and_then(|v| {
            if let Some(b) = v.as_bool() {
                Some(b)
            } else if let Some(n) = v.as_i64() {
                Some(n != 0)
            } else {
                None
            }
        })
        .unwrap_or(false)
}

/// Check if a symbol is eligible for embedding using the pipeline's full policy.
pub fn is_symbol_eligible_for_embedding(
    kind: &str,
    language: &str,
    file_path: &str,
    metadata_json: Option<&str>,
    extra_kinds: &[&str],
) -> bool {
    let kind_lower = kind.to_ascii_lowercase();
    let kind_matches = DEFAULT_EMBEDDABLE_KINDS.contains(&kind_lower.as_str())
        || extra_kinds
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&kind_lower));

    if !kind_matches {
        return false;
    }

    if !is_embeddable_language(language) {
        return false;
    }

    if is_test_path(file_path) {
        return false;
    }

    if is_test_metadata(metadata_json) {
        return false;
    }

    true
}

/// Build a SQL WHERE clause fragment to filter eligible symbols directly in SQLite,
/// preserving each language's configured extra kinds and escaping '_' wildcards.
pub fn sql_symbol_eligibility_filter(
    s: &str,
    extra_kinds_per_lang: &[(String, Vec<String>)],
) -> String {
    let default_kinds = DEFAULT_EMBEDDABLE_KINDS
        .iter()
        .map(|k| format!("'{}'", k.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut kind_conditions = vec![format!("LOWER({s}.kind) IN ({default_kinds})")];

    for (lang, kinds) in extra_kinds_per_lang {
        let valid_kinds: Vec<String> = kinds
            .iter()
            .map(|k| k.to_ascii_lowercase())
            .filter(|k| !DEFAULT_EMBEDDABLE_KINDS.contains(&k.as_str()))
            .collect();
        if !valid_kinds.is_empty() {
            let kinds_in = valid_kinds
                .iter()
                .map(|k| format!("'{k}'"))
                .collect::<Vec<_>>()
                .join(", ");
            let lang_lower = lang.to_ascii_lowercase();
            kind_conditions.push(format!(
                "(LOWER({s}.language) = '{lang_lower}' AND LOWER({s}.kind) IN ({kinds_in}))"
            ));
        }
    }

    let kinds_clause = if kind_conditions.len() == 1 {
        kind_conditions[0].clone()
    } else {
        format!("({})", kind_conditions.join(" OR "))
    };

    let langs_not_in = NON_EMBEDDABLE_LANGUAGES
        .iter()
        .map(|l| format!("'{}'", l))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "{kinds_clause} \
         AND LOWER({s}.language) NOT IN ({langs_not_in}) \
         AND ({s}.metadata IS NULL OR ( \
             json_extract({s}.metadata, '$.test_role') IS NULL \
             AND (json_extract({s}.metadata, '$.is_test') IS NULL \
                  OR json_extract({s}.metadata, '$.is_test') = 0 \
                  OR json_extract({s}.metadata, '$.is_test') = false) \
         )) \
         AND NOT ( \
             {s}.file_path = 'test' OR {s}.file_path LIKE 'test/%' OR {s}.file_path LIKE '%/test/%' OR {s}.file_path LIKE '%/test' \
             OR {s}.file_path = 'tests' OR {s}.file_path LIKE 'tests/%' OR {s}.file_path LIKE '%/tests/%' OR {s}.file_path LIKE '%/tests' \
             OR {s}.file_path = 'Test' OR {s}.file_path LIKE 'Test/%' OR {s}.file_path LIKE '%/Test/%' OR {s}.file_path LIKE '%/Test' \
             OR {s}.file_path = 'Tests' OR {s}.file_path LIKE 'Tests/%' OR {s}.file_path LIKE '%/Tests/%' OR {s}.file_path LIKE '%/Tests' \
             OR {s}.file_path = 'spec' OR {s}.file_path LIKE 'spec/%' OR {s}.file_path LIKE '%/spec/%' OR {s}.file_path LIKE '%/spec' \
             OR {s}.file_path = 'Spec' OR {s}.file_path LIKE 'Spec/%' OR {s}.file_path LIKE '%/Spec/%' OR {s}.file_path LIKE '%/Spec' \
             OR {s}.file_path = '__tests__' OR {s}.file_path LIKE '!_!_tests!_!_/%' ESCAPE '!' OR {s}.file_path LIKE '%/!_!_tests!_!_/%' ESCAPE '!' OR {s}.file_path LIKE '%/!_!_tests!_!_' ESCAPE '!' \
             OR {s}.file_path LIKE '%.Tests/%' OR {s}.file_path LIKE '%.Tests' OR {s}.file_path LIKE '%.Test/%' OR {s}.file_path LIKE '%.Test' \
             OR {s}.file_path LIKE '%.Tests.%' OR {s}.file_path LIKE '%.Test.%' \
             OR {s}.file_path LIKE '%!_test.go' ESCAPE '!' \
             OR {s}.file_path LIKE '%!_test.c' ESCAPE '!' OR {s}.file_path LIKE '%!_test.cc' ESCAPE '!' OR {s}.file_path LIKE '%!_test.cpp' ESCAPE '!' \
             OR {s}.file_path LIKE '%.test.ts' OR {s}.file_path LIKE '%.test.tsx' OR {s}.file_path LIKE '%.test.js' OR {s}.file_path LIKE '%.test.jsx' \
             OR {s}.file_path LIKE '%.spec.ts' OR {s}.file_path LIKE '%.spec.tsx' OR {s}.file_path LIKE '%.spec.js' OR {s}.file_path LIKE '%.spec.jsx' \
             OR ({s}.file_path LIKE '%.py' AND (WITH RECURSIVE split(path, rem) AS (SELECT '', replace({s}.file_path, '\\', '/') UNION ALL SELECT CASE WHEN instr(rem, '/') = 0 THEN rem ELSE substr(rem, 1, instr(rem, '/') - 1) END, CASE WHEN instr(rem, '/') = 0 THEN '' ELSE substr(rem, instr(rem, '/') + 1) END FROM split WHERE rem != '') SELECT path FROM split WHERE rem = '') LIKE 'test!_%' ESCAPE '!') \
         )"
    )
}
