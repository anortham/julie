use std::collections::HashSet;

use super::types::AnnotationMarker;

pub fn normalize_annotations<S: AsRef<str>>(
    raw_texts: &[S],
    language: &str,
) -> Vec<AnnotationMarker> {
    let language = language.trim().to_ascii_lowercase();
    let mut seen_keys = HashSet::new();
    let mut markers = Vec::new();

    for raw_text in raw_texts {
        let inner = strip_syntax(raw_text.as_ref(), language.as_str());
        if inner.is_empty() {
            continue;
        }

        for fragment in expand_fragments(inner, language.as_str()) {
            if let Some(marker) =
                build_marker(fragment.raw_text, language.as_str(), fragment.carrier)
            {
                if seen_keys.insert(marker.annotation_key.clone()) {
                    markers.push(marker);
                }
            }
        }
    }

    markers
}

#[derive(Debug, Clone, Copy)]
struct AnnotationFragment<'a> {
    raw_text: &'a str,
    carrier: Option<&'static str>,
}

fn strip_syntax<'a>(raw_text: &'a str, language: &str) -> &'a str {
    let text = raw_text.trim();

    match language {
        "rust" | "php" => strip_pair(text, "#[", "]"),
        "cpp" | "c++" => strip_pair(text, "[[", "]]"),
        "csharp" | "c#" | "powershell" => strip_pair(text, "[", "]"),
        "vbnet" | "vb.net" | "vb" => strip_pair(text, "<", ">"),
        _ => text.strip_prefix('@').unwrap_or(text).trim(),
    }
}

fn strip_pair<'a>(text: &'a str, prefix: &str, suffix: &str) -> &'a str {
    text.strip_prefix(prefix)
        .and_then(|inner| inner.strip_suffix(suffix))
        .unwrap_or(text)
        .trim()
}

fn expand_fragments<'a>(inner: &'a str, language: &str) -> Vec<AnnotationFragment<'a>> {
    if language == "rust" && invocation_name(inner).eq_ignore_ascii_case("derive") {
        return invocation_arguments(inner)
            .map(split_top_level_commas)
            .unwrap_or_default()
            .into_iter()
            .map(|raw_text| AnnotationFragment {
                raw_text,
                carrier: Some("derive"),
            })
            .collect();
    }

    if matches!(
        language,
        "csharp" | "c#" | "vbnet" | "vb.net" | "vb" | "php" | "cpp" | "c++"
    ) {
        return split_top_level_commas(inner)
            .into_iter()
            .map(|raw_text| AnnotationFragment {
                raw_text,
                carrier: None,
            })
            .collect();
    }

    vec![AnnotationFragment {
        raw_text: inner,
        carrier: None,
    }]
}

fn build_marker(
    raw_fragment: &str,
    language: &str,
    carrier: Option<&'static str>,
) -> Option<AnnotationMarker> {
    let raw_text = raw_fragment.trim();
    if raw_text.is_empty() {
        return None;
    }

    let callable = invocation_name(raw_text);
    let display = display_annotation(callable, language);
    if display.is_empty() {
        return None;
    }

    let mut annotation_key = key_annotation(display, language);
    if strips_attribute_suffix(language) {
        annotation_key = strip_attribute_suffix(&annotation_key).to_string();
    }

    Some(AnnotationMarker {
        annotation: display.to_string(),
        annotation_key,
        raw_text: Some(raw_text.to_string()),
        carrier: carrier.map(str::to_string),
    })
}

fn invocation_name(text: &str) -> &str {
    let mut depth = DelimiterDepth::default();
    let mut quote = None;
    let mut previous_was_escape = false;

    for (index, ch) in text.char_indices() {
        if update_quote_state(ch, &mut quote, &mut previous_was_escape) {
            continue;
        }
        if quote.is_some() {
            continue;
        }

        if ch == '(' && depth.is_empty() {
            return trim_annotation_token(&text[..index]);
        }

        depth.update(ch);
    }

    trim_annotation_token(text)
}

fn invocation_arguments(text: &str) -> Option<&str> {
    let start = text.find('(')?;
    let end = text.rfind(')')?;
    if start >= end {
        return None;
    }
    Some(text[start + 1..end].trim())
}

fn trim_annotation_token(text: &str) -> &str {
    text.trim()
        .split_once(char::is_whitespace)
        .map(|(token, _)| token)
        .unwrap_or_else(|| text.trim())
}

fn display_annotation<'a>(annotation: &'a str, language: &str) -> &'a str {
    let trimmed = annotation.trim();
    if trims_to_rightmost_type_name(language) {
        return rightmost_segment(trimmed);
    }
    trimmed
}

fn key_annotation<'a>(annotation: &'a str, language: &str) -> String {
    let key_source = if language == "php" {
        rightmost_segment(annotation)
    } else {
        annotation
    };
    key_source.trim().to_lowercase()
}

fn trims_to_rightmost_type_name(language: &str) -> bool {
    matches!(
        language,
        "java" | "kotlin" | "scala" | "csharp" | "c#" | "vbnet" | "vb.net" | "vb" | "powershell"
    )
}

fn rightmost_segment(text: &str) -> &str {
    text.rsplit(['.', '\\']).next().unwrap_or(text).trim()
}

fn strips_attribute_suffix(language: &str) -> bool {
    matches!(
        language,
        "csharp" | "c#" | "vbnet" | "vb.net" | "vb" | "powershell"
    )
}

fn strip_attribute_suffix(key: &str) -> &str {
    key.strip_suffix("attribute").unwrap_or(key)
}

fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = DelimiterDepth::default();
    let mut quote = None;
    let mut previous_was_escape = false;

    for (index, ch) in text.char_indices() {
        if update_quote_state(ch, &mut quote, &mut previous_was_escape) {
            continue;
        }
        if quote.is_some() {
            continue;
        }

        if ch == ',' && depth.is_empty() {
            push_trimmed(&mut parts, &text[start..index]);
            start = index + ch.len_utf8();
            continue;
        }

        depth.update(ch);
    }

    push_trimmed(&mut parts, &text[start..]);
    parts
}

fn push_trimmed<'a>(parts: &mut Vec<&'a str>, part: &'a str) {
    let trimmed = part.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }
}

fn update_quote_state(ch: char, quote: &mut Option<char>, previous_was_escape: &mut bool) -> bool {
    if let Some(active_quote) = *quote {
        if *previous_was_escape {
            *previous_was_escape = false;
            return true;
        }
        if ch == '\\' {
            *previous_was_escape = true;
            return true;
        }
        if ch == active_quote {
            *quote = None;
            return true;
        }
        return true;
    }

    if matches!(ch, '\'' | '"' | '`') {
        *quote = Some(ch);
        return true;
    }

    false
}

#[derive(Debug, Default)]
struct DelimiterDepth {
    parentheses: usize,
    brackets: usize,
    braces: usize,
    angles: usize,
}

impl DelimiterDepth {
    fn is_empty(&self) -> bool {
        self.parentheses == 0 && self.brackets == 0 && self.braces == 0 && self.angles == 0
    }

    fn update(&mut self, ch: char) {
        match ch {
            '(' => self.parentheses += 1,
            ')' => self.parentheses = self.parentheses.saturating_sub(1),
            '[' => self.brackets += 1,
            ']' => self.brackets = self.brackets.saturating_sub(1),
            '{' => self.braces += 1,
            '}' => self.braces = self.braces.saturating_sub(1),
            '<' => self.angles += 1,
            '>' => self.angles = self.angles.saturating_sub(1),
            _ => {}
        }
    }
}
