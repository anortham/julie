//! Ablation tests for `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` env-var gates.
//!
//! # What these tests verify
//!
//! Two runtime env-var gates can disable parts of the `CodeTokenizer`'s aggressive
//! token expansion:
//!
//! - `JULIE_ABLATE_STEMMING=1`    — the English stemmer step is skipped entirely.
//! - `JULIE_ABLATE_CAMEL_EMIT=1`  — CamelCase splitting is skipped; the whole identifier
//!   is emitted as one lowercased token.
//!
//! When both flags are unset (or set to `"0"`), behavior is byte-identical to the
//! pre-ablation baseline. The tests below prove all four combinations.
//!
//! # Index-rebuild caveat
//!
//! `CodeTokenizer` is used at **both** index time and query time. If a corpus was
//! indexed without a flag, the stored tokens already contain stems / split parts.
//! Enabling a flag only shifts *query-time* tokenization until the index is rebuilt.
//!
//! For a clean A/B measurement the bakeoff harness (Task 4) must:
//! 1. Set the env var.
//! 2. Delete / wipe the existing Tantivy index.
//! 3. Rebuild (force-reindex) with the flag active.
//! 4. Run queries.
//!
//! The `TokenizerCompatibilitySignature` includes both ablation booleans, so changing
//! either flag changes the signature and triggers an automatic index rebuild the next
//! time the workspace is opened via `open_or_create_with_tokenizer`.

use std::env;

use tantivy::tokenizer::{TextAnalyzer, TokenStream};

use crate::search::tokenizer::CodeTokenizer;

// ---------------------------------------------------------------------------
// Helper: build a tokenizer with explicit ablation flags, bypassing env vars.
//
// We cannot safely mutate process-global env vars in tests (race conditions with
// parallel test runners). Instead we set the env var, construct the tokenizer
// (which reads env vars once at construction), then immediately unset the env var.
// Each test function is self-contained; we use a mutex to prevent data races.
// ---------------------------------------------------------------------------

use std::sync::Mutex;

/// Global lock to serialise env-var reads+writes across test functions.
/// Parallel tokenizer construction in the same process can race on env var reads.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Construct a `CodeTokenizer` with the given ablation env vars set.
/// Restores previous env state before returning.
fn build_tokenizer_with_env(ablate_stemming: bool, ablate_camel_emit: bool) -> CodeTokenizer {
    let _guard = ENV_LOCK.lock().unwrap();

    // Save previous state
    let prev_stem = env::var("JULIE_ABLATE_STEMMING").ok();
    let prev_camel = env::var("JULIE_ABLATE_CAMEL_EMIT").ok();

    // SAFETY: we hold ENV_LOCK, serialising all env mutations in this test binary.
    unsafe {
        // Set flags
        if ablate_stemming {
            env::set_var("JULIE_ABLATE_STEMMING", "1");
        } else {
            env::remove_var("JULIE_ABLATE_STEMMING");
        }
        if ablate_camel_emit {
            env::set_var("JULIE_ABLATE_CAMEL_EMIT", "1");
        } else {
            env::remove_var("JULIE_ABLATE_CAMEL_EMIT");
        }
    }

    // Construct — reads env vars here
    let tokenizer = CodeTokenizer::new(vec![]);

    // SAFETY: restoring previous state under the same lock.
    unsafe {
        match prev_stem {
            Some(v) => env::set_var("JULIE_ABLATE_STEMMING", v),
            None => env::remove_var("JULIE_ABLATE_STEMMING"),
        }
        match prev_camel {
            Some(v) => env::set_var("JULIE_ABLATE_CAMEL_EMIT", v),
            None => env::remove_var("JULIE_ABLATE_CAMEL_EMIT"),
        }
    }

    tokenizer
}

/// Run the tokenizer and collect all emitted token texts.
fn collect_tokens(tokenizer: CodeTokenizer, text: &str) -> Vec<String> {
    let mut analyzer = TextAnalyzer::builder(tokenizer).build();
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while let Some(tok) = stream.next() {
        tokens.push(tok.text.clone());
    }
    tokens
}

// ---------------------------------------------------------------------------
// Baseline (both gates off) — default behavior must be preserved
// ---------------------------------------------------------------------------

#[test]
fn test_ablation_default_camel_splits_and_stems() {
    // Default tokenizer: "getUserData" → [getuserdata, get, user, data] plus any stems.
    // The exact stem set depends on the English stemmer; we assert the known-present tokens.
    let tokenizer = build_tokenizer_with_env(false, false);
    let tokens = collect_tokens(tokenizer, "getUserData");

    assert!(
        tokens.contains(&"getuserdata".to_string()),
        "Original lowercased token must be present; got: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"get".to_string()),
        "CamelCase part 'get' must be present; got: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"user".to_string()),
        "CamelCase part 'user' must be present; got: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"data".to_string()),
        "CamelCase part 'data' must be present; got: {:?}",
        tokens
    );
}

#[test]
fn test_ablation_default_stemming_is_active() {
    // Default: "running" → includes the stem "run" as an additional token.
    let tokenizer = build_tokenizer_with_env(false, false);
    let tokens = collect_tokens(tokenizer, "running");

    assert!(
        tokens.contains(&"running".to_string()),
        "Exact token 'running' must be present; got: {:?}",
        tokens
    );
    assert!(
        tokens.contains(&"run".to_string()),
        "Stem 'run' must be emitted in default mode; got: {:?}",
        tokens
    );
}

// ---------------------------------------------------------------------------
// JULIE_ABLATE_CAMEL_EMIT=1 — CamelCase splitting disabled
// ---------------------------------------------------------------------------

#[test]
fn test_ablate_camel_emit_suppresses_splits() {
    // With JULIE_ABLATE_CAMEL_EMIT=1, "getUserData" must emit only one token:
    // the whole identifier lowercased. No "get", "user", or "data" parts.
    let tokenizer = build_tokenizer_with_env(false, true);
    let tokens = collect_tokens(tokenizer, "getUserData");

    assert!(
        tokens.contains(&"getuserdata".to_string()),
        "Lowercased whole identifier must be present; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"get".to_string()),
        "CamelCase part 'get' must NOT be emitted when camel-emit is ablated; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"user".to_string()),
        "CamelCase part 'user' must NOT be emitted when camel-emit is ablated; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"data".to_string()),
        "CamelCase part 'data' must NOT be emitted when camel-emit is ablated; got: {:?}",
        tokens
    );
}

#[test]
fn test_ablate_camel_emit_signature_differs_from_default() {
    // The compat signature must differ when camel-emit is ablated vs default,
    // so that the index is rebuilt when the flag is toggled.
    let default_sig = build_tokenizer_with_env(false, false).compatibility_signature();
    let ablated_sig = build_tokenizer_with_env(false, true).compatibility_signature();

    assert_ne!(
        default_sig, ablated_sig,
        "Compat signatures must differ when JULIE_ABLATE_CAMEL_EMIT is toggled"
    );
    assert!(
        ablated_sig.ablate_camel_emit,
        "ablated_sig.ablate_camel_emit must be true"
    );
    assert!(
        !default_sig.ablate_camel_emit,
        "default_sig.ablate_camel_emit must be false"
    );
}

// ---------------------------------------------------------------------------
// JULIE_ABLATE_STEMMING=1 — stemmer disabled
// ---------------------------------------------------------------------------

#[test]
fn test_ablate_stemming_suppresses_stem_tokens() {
    // With JULIE_ABLATE_STEMMING=1, "running" must emit only "running" (exact).
    // The stem "run" must NOT appear.
    let tokenizer = build_tokenizer_with_env(true, false);
    let tokens = collect_tokens(tokenizer, "running");

    assert!(
        tokens.contains(&"running".to_string()),
        "Exact token 'running' must be present even when stemming is ablated; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"run".to_string()),
        "Stem 'run' must NOT be emitted when JULIE_ABLATE_STEMMING=1; got: {:?}",
        tokens
    );
}

#[test]
fn test_ablate_stemming_signature_differs_from_default() {
    let default_sig = build_tokenizer_with_env(false, false).compatibility_signature();
    let ablated_sig = build_tokenizer_with_env(true, false).compatibility_signature();

    assert_ne!(
        default_sig, ablated_sig,
        "Compat signatures must differ when JULIE_ABLATE_STEMMING is toggled"
    );
    assert!(
        ablated_sig.ablate_stemming,
        "ablated_sig.ablate_stemming must be true"
    );
    assert!(
        !default_sig.ablate_stemming,
        "default_sig.ablate_stemming must be false"
    );
}

// ---------------------------------------------------------------------------
// Both gates on simultaneously
// ---------------------------------------------------------------------------

#[test]
fn test_ablate_both_gates_on() {
    // With both flags set, "getUserData" should emit only the lowercased whole
    // token — no camel splits, no stems.
    let tokenizer = build_tokenizer_with_env(true, true);
    let tokens = collect_tokens(tokenizer, "getUserData");

    assert!(
        tokens.contains(&"getuserdata".to_string()),
        "Lowercased whole identifier must be present; got: {:?}",
        tokens
    );
    // No camel parts
    assert!(
        !tokens.contains(&"get".to_string()),
        "'get' must NOT appear with both gates active; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"user".to_string()),
        "'user' must NOT appear with both gates active; got: {:?}",
        tokens
    );
    assert!(
        !tokens.contains(&"data".to_string()),
        "'data' must NOT appear with both gates active; got: {:?}",
        tokens
    );
    // stemming-derived tokens that would appear for "getuserdata" fragments must
    // also be absent.  "getuserdata" itself is too long to have a useful stem that
    // differs from the original, but "data" would have been stemmed in the default
    // path; since it was never emitted, no stem of it should appear either.
    // Verify the token list has only "getuserdata" (possibly also a stem of the
    // whole lowercased identifier, but no camel-split-derived tokens).
    let unexpected: Vec<&String> = tokens
        .iter()
        .filter(|t| t.as_str() != "getuserdata")
        .collect();
    assert!(
        unexpected.is_empty(),
        "With both gates active, only 'getuserdata' should be emitted; got extra: {:?}",
        unexpected
    );
}

#[test]
fn test_ablate_both_gates_signature_differs_from_all_others() {
    let sig_default = build_tokenizer_with_env(false, false).compatibility_signature();
    let sig_stem_only = build_tokenizer_with_env(true, false).compatibility_signature();
    let sig_camel_only = build_tokenizer_with_env(false, true).compatibility_signature();
    let sig_both = build_tokenizer_with_env(true, true).compatibility_signature();

    assert_ne!(sig_default, sig_stem_only);
    assert_ne!(sig_default, sig_camel_only);
    assert_ne!(sig_default, sig_both);
    assert_ne!(sig_stem_only, sig_camel_only);
    assert_ne!(sig_stem_only, sig_both);
    assert_ne!(sig_camel_only, sig_both);
}

// ---------------------------------------------------------------------------
// env var value "0" must behave as unset (normal mode)
// ---------------------------------------------------------------------------

#[test]
fn test_ablation_zero_value_treated_as_off() {
    // "JULIE_ABLATE_STEMMING=0" and "JULIE_ABLATE_CAMEL_EMIT=0" must behave
    // identically to the env vars being unset.
    let _guard = ENV_LOCK.lock().unwrap();

    let prev_stem = env::var("JULIE_ABLATE_STEMMING").ok();
    let prev_camel = env::var("JULIE_ABLATE_CAMEL_EMIT").ok();

    // SAFETY: we hold ENV_LOCK, serialising all env mutations in this test binary.
    unsafe {
        env::set_var("JULIE_ABLATE_STEMMING", "0");
        env::set_var("JULIE_ABLATE_CAMEL_EMIT", "0");
    }

    let tokenizer_zero = CodeTokenizer::new(vec![]);
    let sig_zero = tokenizer_zero.compatibility_signature();

    // SAFETY: same lock held.
    unsafe {
        env::remove_var("JULIE_ABLATE_STEMMING");
        env::remove_var("JULIE_ABLATE_CAMEL_EMIT");
    }

    let tokenizer_unset = CodeTokenizer::new(vec![]);
    let sig_unset = tokenizer_unset.compatibility_signature();

    // Restore
    // SAFETY: same lock held.
    unsafe {
        match prev_stem {
            Some(v) => env::set_var("JULIE_ABLATE_STEMMING", v),
            None => env::remove_var("JULIE_ABLATE_STEMMING"),
        }
        match prev_camel {
            Some(v) => env::set_var("JULIE_ABLATE_CAMEL_EMIT", v),
            None => env::remove_var("JULIE_ABLATE_CAMEL_EMIT"),
        }
    }

    assert_eq!(
        sig_zero, sig_unset,
        "Value '0' must produce the same signature as unset"
    );
    assert!(
        !sig_zero.ablate_stemming,
        "ablate_stemming must be false when env var is '0'"
    );
    assert!(
        !sig_zero.ablate_camel_emit,
        "ablate_camel_emit must be false when env var is '0'"
    );
}
