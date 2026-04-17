use crate::base::{Symbol, SymbolKind, Visibility};
use crate::vbnet::VbNetExtractor;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vb_dotnet::LANGUAGE.into())
        .expect("Error loading VB.NET grammar");
    parser
}

pub(crate) trait VisibilityExt {
    fn to_string(&self) -> String;
}

impl VisibilityExt for Visibility {
    fn to_string(&self) -> String {
        match self {
            Visibility::Public => "public".to_string(),
            Visibility::Private => "private".to_string(),
            Visibility::Protected => "protected".to_string(),
        }
    }
}

pub(crate) fn get_vb_visibility(symbol: &Symbol) -> String {
    if let Some(vb_visibility) = symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("vb_visibility"))
        .and_then(|v| v.as_str())
    {
        return vb_visibility.to_string();
    }

    symbol
        .visibility
        .as_ref()
        .map_or("private".to_string(), |v| VisibilityExt::to_string(v))
}

pub mod core;
pub mod identifiers;
pub mod members;
pub mod relationships;
pub mod types;
