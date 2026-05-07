pub(super) fn clean_r_name(text: &str) -> Option<String> {
    let trimmed = text
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim()
        .to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn argument_list_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn split_top_level_arguments(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in text.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let argument = current.trim();
                if !argument.is_empty() {
                    args.push(argument.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let argument = current.trim();
    if !argument.is_empty() {
        args.push(argument.to_string());
    }
    args
}

pub(super) fn function_signature(value: &str) -> String {
    let Some(start) = value.find("function") else {
        return value.to_string();
    };
    let after_function = &value[start + "function".len()..];
    let Some(open) = after_function.find('(') else {
        return "function()".to_string();
    };
    let params_start = start + "function".len() + open;
    let Some(close_offset) = value[params_start..].find(')') else {
        return "function()".to_string();
    };
    value[start..params_start + close_offset + 1].to_string()
}
