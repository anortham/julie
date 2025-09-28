pub(crate) fn capitalize_first_letter(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if let Some(first) = chars.get_mut(0) {
        if let Some(upper) = first.to_uppercase().next() {
            *first = upper;
        }
    }
    chars.into_iter().collect()
}

pub(crate) fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(capitalize_first_letter)
        .collect::<Vec<String>>()
        .join("")
}

pub(crate) fn to_camel_case(s: &str) -> String {
    let words: Vec<&str> = s.split('_').collect();
    if words.is_empty() {
        return s.to_string();
    }

    let mut result = words[0].to_lowercase();
    for word in &words[1..] {
        result.push_str(&capitalize_first_letter(word));
    }
    result
}
