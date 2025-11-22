/// Component name extraction from Vue SFC
///
/// Handles extracting component name from export default { name: ... } or filename
use super::parsing::VueSection;

/// Extract component name from sections or filename
/// Priority: export default { name: 'X' } > filename
pub(super) fn extract_component_name(file_path: &str, sections: &[VueSection]) -> Option<String> {
    // First try to find name from script section: export default { name: 'ComponentName' }
    for section in sections {
        if section.section_type == "script" {
            // Look for: name: 'ComponentName' or name: "ComponentName"
            if let Some(name_match) = regex::Regex::new(r#"name\s*:\s*['"]([^'"]+)['"]"#)
                .ok()
                .and_then(|re| re.captures(&section.content))
            {
                if let Some(name) = name_match.get(1) {
                    return Some(name.as_str().to_string());
                }
            }
        }
    }

    // Fallback: use filename (convert kebab-case to PascalCase)
    if let Some(filename) = std::path::Path::new(file_path).file_stem() {
        let name = filename.to_str().unwrap_or("VueComponent");

        // Convert my-component.vue -> MyComponent
        let pascal_case = name
            .split('-')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<String>();

        return Some(pascal_case);
    }

    Some("VueComponent".to_string())
}
