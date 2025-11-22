// Vue SFC (Single File Component) parsing module
//
// Responsible for parsing .vue file structure and extracting template, script, and style sections

use super::helpers::{
    LANG_ATTR_RE, SCRIPT_START_RE, SECTION_END_RE, STYLE_START_RE, TEMPLATE_START_RE,
};
use std::fmt;

/// Represents a section within a Vue SFC file (template, script, or style)
#[derive(Debug, Clone)]
pub(crate) struct VueSection {
    pub(crate) section_type: String, // "template", "script", "style"
    pub(crate) content: String,
    pub(crate) start_line: usize,
    #[allow(dead_code)]
    pub(crate) end_line: usize,
    pub(crate) lang: Option<String>, // e.g., 'ts', 'scss'
}

impl fmt::Display for VueSection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}@{}{}",
            self.section_type,
            self.start_line,
            self.lang
                .as_deref()
                .map(|l| format!("({})", l))
                .unwrap_or_default()
        )
    }
}

/// Helper struct for building VueSection during parsing
#[derive(Debug)]
pub(crate) struct VueSectionBuilder {
    pub(crate) section_type: String,
    pub(crate) start_line: usize,
    pub(crate) lang: Option<String>,
}

impl VueSectionBuilder {
    pub(crate) fn build(self, content: String, end_line: usize) -> VueSection {
        VueSection {
            section_type: self.section_type,
            content,
            start_line: self.start_line,
            end_line,
            lang: self.lang,
        }
    }
}

/// Parse Vue SFC structure to extract template, script, and style sections
/// Implementation of parseVueSFC logic
pub(crate) fn parse_vue_sfc(content: &str) -> Result<Vec<VueSection>, Box<dyn std::error::Error>> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut current_section: Option<VueSectionBuilder> = None;
    let mut section_content = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Check for section start - following regex patterns
        let template_match = TEMPLATE_START_RE.captures(trimmed);
        let script_match = SCRIPT_START_RE.captures(trimmed);
        let style_match = STYLE_START_RE.captures(trimmed);

        if template_match.is_some() || script_match.is_some() || style_match.is_some() {
            // End previous section
            if let Some(section) = current_section.take() {
                sections.push(section.build(section_content.join("\n"), i));
            }

            // Start new section
            let section_type = if template_match.is_some() {
                "template"
            } else if script_match.is_some() {
                "script"
            } else {
                "style"
            };

            let attrs = template_match
                .or(script_match)
                .or(style_match)
                .and_then(|m| m.get(1))
                .map(|m| m.as_str())
                .unwrap_or("");

            let lang = LANG_ATTR_RE
                .captures(attrs)
                .and_then(|m| m.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| match section_type {
                    "script" => "js".to_string(),
                    "style" => "css".to_string(),
                    _ => "html".to_string(),
                });

            current_section = Some(VueSectionBuilder {
                section_type: section_type.to_string(),
                start_line: i + 1,
                lang: Some(lang),
            });
            section_content.clear();
            continue;
        }

        // Check for section end
        if SECTION_END_RE.is_match(trimmed) {
            if let Some(section) = current_section.take() {
                sections.push(section.build(section_content.join("\n"), i));
                section_content.clear();
            }
            continue;
        }

        // Add content to current section
        if current_section.is_some() {
            section_content.push(line.to_string());
        }
    }

    // Handle unclosed section - following reference logic
    if let Some(section) = current_section {
        sections.push(section.build(section_content.join("\n"), lines.len()));
    }

    Ok(sections)
}
