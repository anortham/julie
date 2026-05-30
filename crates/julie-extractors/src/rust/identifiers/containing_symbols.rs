use crate::base::{Symbol, SymbolKind};

pub(super) struct ContainingSymbolIndex<'a> {
    symbols: Vec<IndexedSymbol<'a>>,
}

struct IndexedSymbol<'a> {
    symbol: &'a Symbol,
    priority: u32,
    size: u32,
}

impl<'a> ContainingSymbolIndex<'a> {
    pub(super) fn new(symbols: &'a [Symbol], file_path: &str) -> Self {
        let mut symbols: Vec<IndexedSymbol<'a>> = symbols
            .iter()
            .filter(|symbol| symbol.file_path == file_path)
            .map(|symbol| IndexedSymbol {
                symbol,
                priority: symbol_priority(&symbol.kind),
                size: symbol.end_byte.saturating_sub(symbol.start_byte),
            })
            .collect();
        symbols.sort_by(|left, right| {
            left.symbol
                .start_line
                .cmp(&right.symbol.start_line)
                .then_with(|| left.symbol.start_column.cmp(&right.symbol.start_column))
        });
        Self { symbols }
    }

    pub(super) fn find(&self, node: tree_sitter::Node) -> Option<&'a Symbol> {
        let position = node.start_position();
        let pos_line = (position.row + 1) as u32;
        let pos_column = position.column as u32;
        let mut best: Option<&IndexedSymbol<'a>> = None;

        for candidate in &self.symbols {
            if candidate.symbol.start_line > pos_line {
                break;
            }

            if !symbol_contains_position(candidate.symbol, pos_line, pos_column) {
                continue;
            }

            if best.is_none_or(|current| is_better_containing_symbol(candidate, current)) {
                best = Some(candidate);
            }
        }

        best.map(|candidate| candidate.symbol)
    }
}

fn symbol_contains_position(symbol: &Symbol, pos_line: u32, pos_column: u32) -> bool {
    let line_contains = symbol.start_line <= pos_line && symbol.end_line >= pos_line;
    if !line_contains {
        return false;
    }

    if pos_line == symbol.start_line && pos_line == symbol.end_line {
        symbol.start_column <= pos_column && symbol.end_column >= pos_column
    } else if pos_line == symbol.start_line {
        symbol.start_column <= pos_column
    } else if pos_line == symbol.end_line {
        symbol.end_column >= pos_column
    } else {
        true
    }
}

fn is_better_containing_symbol(candidate: &IndexedSymbol<'_>, current: &IndexedSymbol<'_>) -> bool {
    candidate.priority < current.priority
        || (candidate.priority == current.priority && candidate.size < current.size)
}

fn symbol_priority(kind: &SymbolKind) -> u32 {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => 1,
        SymbolKind::Class | SymbolKind::Interface => 2,
        SymbolKind::Namespace => 3,
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => 10,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn containing_symbol_index_keeps_existing_priority_and_smallest_span_rules() {
        let source = "fn caller() {\n    helper();\n}\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("failed to set Rust language");
        let tree = parser.parse(source, None).expect("failed to parse Rust");
        let call = find_first_node_kind(tree.root_node(), "call_expression")
            .expect("call expression should parse");

        let symbols = vec![
            test_symbol(
                "module",
                SymbolKind::Namespace,
                "test.rs",
                1,
                0,
                3,
                1,
                0,
                28,
            ),
            test_symbol(
                "wide_fn",
                SymbolKind::Function,
                "test.rs",
                1,
                0,
                3,
                1,
                0,
                28,
            ),
            test_symbol(
                "narrow_fn",
                SymbolKind::Function,
                "test.rs",
                2,
                4,
                2,
                13,
                call.start_byte() as u32,
                call.end_byte() as u32,
            ),
            test_symbol(
                "other_file",
                SymbolKind::Function,
                "other.rs",
                2,
                4,
                2,
                13,
                call.start_byte() as u32,
                call.end_byte() as u32,
            ),
        ];

        let index = ContainingSymbolIndex::new(&symbols, "test.rs");

        assert_eq!(
            index.find(call).map(|symbol| symbol.id.as_str()),
            Some("narrow_fn")
        );
    }

    fn find_first_node_kind<'a>(
        node: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first_node_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    fn test_symbol(
        id: &str,
        kind: SymbolKind,
        file_path: &str,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        start_byte: u32,
        end_byte: u32,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line,
            start_column,
            end_line,
            end_column,
            start_byte,
            end_byte,
            body_span: None,
            body_hash: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            annotations: Vec::new(),
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }
}
