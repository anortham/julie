/// Tests for `pretokenize_code` — the index-time preprocessing function that expands
/// a code token into original + CamelCase parts + snake_case parts.
///
/// The output is a space-joined string intended to be fed through `SimpleCodeTokenizer`
/// at index time so that searches for any component part find the original identifier.
use crate::search::tokenizer::pretokenize_code;

/// Core acceptance criterion: all of original, camel splits, and snake splits
/// are present in the output for `"getUserData_v2"`.
///
/// `split_camel_case("getUserData_v2")` → `["get", "User", "Data_v2"]`
/// `split_snake_case("getUserData_v2")` → `["getUserData", "v2"]`
/// So the output string contains the original plus those parts (deduped).
#[test]
fn camel_and_snake_split() {
    let result = pretokenize_code("getUserData_v2");
    let tokens: Vec<&str> = result.split_whitespace().collect();

    // Original identifier (case-preserved; SimpleCodeTokenizer lowercases downstream)
    assert!(
        tokens
            .iter()
            .any(|t| t.eq_ignore_ascii_case("getUserData_v2")),
        "output must contain original identifier; got: {result}"
    );

    // CamelCase splits: split_camel_case("getUserData_v2") → ["get", "User", "Data_v2"]
    assert!(
        tokens.iter().any(|t| t.eq_ignore_ascii_case("get")),
        "output must contain camel split 'get'; got: {result}"
    );
    assert!(
        tokens.iter().any(|t| t.eq_ignore_ascii_case("user")),
        "output must contain camel split 'user'; got: {result}"
    );
    // "Data_v2" is the third camel split (the underscore is inside the last word)
    assert!(
        tokens
            .iter()
            .any(|t| t.eq_ignore_ascii_case("data_v2") || t.eq_ignore_ascii_case("Data_v2")),
        "output must contain camel split 'Data_v2'; got: {result}"
    );

    // snake_case splits: split_snake_case("getUserData_v2") → ["getUserData", "v2"]
    assert!(
        tokens.iter().any(|t| t.eq_ignore_ascii_case("v2")),
        "output must contain snake split 'v2'; got: {result}"
    );
}

/// Multiple space-separated input tokens: each is independently expanded.
#[test]
fn multiple_tokens_expanded_independently() {
    let result = pretokenize_code("getUserData findByName");
    let tokens: Vec<&str> = result.split_whitespace().collect();

    // Both originals present
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("getUserData")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("findByName")));

    // Splits from first token
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("get")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("user")));

    // Splits from second token
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("find")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("by")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("name")));
}

/// Plain snake_case: original + snake splits.
#[test]
fn plain_snake_case() {
    let result = pretokenize_code("some_other_thing");
    let tokens: Vec<&str> = result.split_whitespace().collect();

    assert!(
        tokens
            .iter()
            .any(|t| t.eq_ignore_ascii_case("some_other_thing"))
    );
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("some")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("other")));
    assert!(tokens.iter().any(|t| t.eq_ignore_ascii_case("thing")));
}

/// Plain identifier with no splits: just original emitted once.
#[test]
fn plain_identifier_no_splits() {
    let result = pretokenize_code("handlerequest");
    let tokens: Vec<&str> = result.split_whitespace().collect();
    assert_eq!(tokens, vec!["handlerequest"]);
}

/// Empty string produces empty output.
#[test]
fn empty_input() {
    let result = pretokenize_code("");
    assert!(result.is_empty() || result.trim().is_empty());
}
