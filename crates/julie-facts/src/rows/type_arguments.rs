use julie_extractors::{TypeArgument, TypeArgumentUsage};

/// One flattened type argument. `parent` is the index in the flattened list
/// (the row ordinal) of the enclosing argument, `None` at the top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatTypeArgument {
    pub identifier_id: String,
    pub parent: Option<usize>,
    pub position: u32,
    pub type_name: String,
}

/// Flatten use-site type-argument trees in document order, parents before
/// children, so the output index doubles as the row ordinal.
pub fn flatten_type_argument_usages(usages: &[TypeArgumentUsage]) -> Vec<FlatTypeArgument> {
    let mut rows = Vec::new();
    for usage in usages {
        for arg in &usage.arguments {
            flatten_arg(&usage.identifier_id, arg, None, &mut rows);
        }
    }
    rows
}

fn flatten_arg(
    identifier_id: &str,
    arg: &TypeArgument,
    parent: Option<usize>,
    rows: &mut Vec<FlatTypeArgument>,
) {
    let index = rows.len();
    rows.push(FlatTypeArgument {
        identifier_id: identifier_id.to_string(),
        parent,
        position: arg.ordinal,
        type_name: arg.type_name.clone(),
    });
    for child in &arg.children {
        flatten_arg(identifier_id, child, Some(index), rows);
    }
}
