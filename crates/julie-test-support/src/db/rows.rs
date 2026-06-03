use std::collections::HashMap;

use anyhow::bail;

use julie_core::database::SymbolDatabase;
use julie_core::database::types::FileInfo;
use julie_extractors::{
    AnnotationMarker, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol,
    SymbolKind, Visibility,
};

pub fn file_info_builder(path: impl Into<String>) -> FileInfoBuilder {
    FileInfoBuilder::new(path)
}

pub fn store_file_info_if_missing(
    db: &SymbolDatabase,
    file_info: &FileInfo,
) -> anyhow::Result<bool> {
    if db.get_file_hash(&file_info.path)?.is_some() {
        return Ok(false);
    }

    db.store_file_info(file_info)?;
    Ok(true)
}

pub struct FileInfoBuilder {
    path: String,
    language: String,
    hash: String,
    size: i64,
    last_modified: i64,
    last_indexed: i64,
    symbol_count: i32,
    line_count: i32,
    content: Option<String>,
}

impl FileInfoBuilder {
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            hash: format!("hash-{path}"),
            path,
            language: "rust".to_string(),
            size: 0,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: None,
        }
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = hash.into();
        self
    }

    pub fn size(mut self, size: i64) -> Self {
        self.size = size;
        self
    }

    pub fn last_modified(mut self, last_modified: i64) -> Self {
        self.last_modified = last_modified;
        self
    }

    pub fn last_indexed(mut self, last_indexed: i64) -> Self {
        self.last_indexed = last_indexed;
        self
    }

    pub fn symbol_count(mut self, symbol_count: i32) -> Self {
        self.symbol_count = symbol_count;
        self
    }

    pub fn line_count(mut self, line_count: i32) -> Self {
        self.line_count = line_count;
        self
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn build(self) -> FileInfo {
        FileInfo {
            path: self.path,
            language: self.language,
            hash: self.hash,
            size: self.size,
            last_modified: self.last_modified,
            last_indexed: self.last_indexed,
            symbol_count: self.symbol_count,
            line_count: self.line_count,
            content: self.content,
        }
    }
}

pub fn symbol_builder(
    id: impl Into<String>,
    name: impl Into<String>,
    file_path: impl Into<String>,
) -> SymbolBuilder {
    SymbolBuilder::new(id, name, file_path)
}

pub fn set_symbol_reference_scores(
    db: &SymbolDatabase,
    scores: &[(&str, f64)],
) -> anyhow::Result<()> {
    for (id, score) in scores {
        let updated = db.conn.execute(
            "UPDATE symbols SET reference_score = ?1 WHERE id = ?2",
            rusqlite::params![score, id],
        )?;
        if updated == 0 {
            bail!("failed to set reference_score for missing symbol id `{id}`");
        }
    }
    Ok(())
}

pub struct SymbolBuilder {
    id: String,
    name: String,
    kind: SymbolKind,
    language: String,
    file_path: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
    signature: Option<String>,
    doc_comment: Option<String>,
    visibility: Option<Visibility>,
    parent_id: Option<String>,
    metadata: Option<HashMap<String, serde_json::Value>>,
    semantic_group: Option<String>,
    confidence: Option<f32>,
    code_context: Option<String>,
    content_type: Option<String>,
    annotations: Vec<AnnotationMarker>,
}

impl SymbolBuilder {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        file_path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.into(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        }
    }

    pub fn kind(mut self, kind: SymbolKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn span(
        mut self,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        self.start_line = start_line;
        self.start_column = start_column;
        self.end_line = end_line;
        self.end_column = end_column;
        self
    }

    pub fn bytes(mut self, start_byte: u32, end_byte: u32) -> Self {
        self.start_byte = start_byte;
        self.end_byte = end_byte;
        self
    }

    pub fn signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn doc_comment(mut self, doc_comment: impl Into<String>) -> Self {
        self.doc_comment = Some(doc_comment.into());
        self
    }

    pub fn visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn parent_id(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn code_context(mut self, code_context: impl Into<String>) -> Self {
        self.code_context = Some(code_context.into());
        self
    }

    pub fn annotations(mut self, annotations: Vec<AnnotationMarker>) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn build(self) -> Symbol {
        Symbol {
            id: self.id,
            name: self.name,
            kind: self.kind,
            language: self.language,
            file_path: self.file_path,
            start_line: self.start_line,
            start_column: self.start_column,
            end_line: self.end_line,
            end_column: self.end_column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            signature: self.signature,
            doc_comment: self.doc_comment,
            visibility: self.visibility,
            parent_id: self.parent_id,
            metadata: self.metadata,
            semantic_group: self.semantic_group,
            confidence: self.confidence,
            code_context: self.code_context,
            content_type: self.content_type,
            body_span: None,
            body_hash: None,
            annotations: self.annotations,
        }
    }
}

pub fn relationship_builder(
    id: impl Into<String>,
    from_symbol_id: impl Into<String>,
    to_symbol_id: impl Into<String>,
) -> RelationshipBuilder {
    RelationshipBuilder::new(id, from_symbol_id, to_symbol_id)
}

pub struct RelationshipBuilder {
    id: String,
    from_symbol_id: String,
    to_symbol_id: String,
    kind: RelationshipKind,
    file_path: String,
    line_number: u32,
    confidence: f32,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl RelationshipBuilder {
    pub fn new(
        id: impl Into<String>,
        from_symbol_id: impl Into<String>,
        to_symbol_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            from_symbol_id: from_symbol_id.into(),
            to_symbol_id: to_symbol_id.into(),
            kind: RelationshipKind::Calls,
            file_path: String::new(),
            line_number: 1,
            confidence: 1.0,
            metadata: None,
        }
    }

    pub fn kind(mut self, kind: RelationshipKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn file_path(mut self, file_path: impl Into<String>) -> Self {
        self.file_path = file_path.into();
        self
    }

    pub fn line_number(mut self, line_number: u32) -> Self {
        self.line_number = line_number;
        self
    }

    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn build(self) -> Relationship {
        Relationship {
            id: self.id,
            from_symbol_id: self.from_symbol_id,
            to_symbol_id: self.to_symbol_id,
            kind: self.kind,
            file_path: self.file_path,
            line_number: self.line_number,
            confidence: self.confidence,
            metadata: self.metadata,
        }
    }
}

pub fn identifier_builder(
    id: impl Into<String>,
    name: impl Into<String>,
    file_path: impl Into<String>,
) -> IdentifierBuilder {
    IdentifierBuilder::new(id, name, file_path)
}

pub struct IdentifierBuilder {
    id: String,
    name: String,
    kind: IdentifierKind,
    language: String,
    file_path: String,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
    containing_symbol_id: Option<String>,
    target_symbol_id: Option<String>,
    confidence: f32,
    code_context: Option<String>,
}

impl IdentifierBuilder {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        file_path: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let end_column = name.len() as u32;
        Self {
            id: id.into(),
            name,
            kind: IdentifierKind::Call,
            language: "rust".to_string(),
            file_path: file_path.into(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column,
            start_byte: 0,
            end_byte: end_column,
            containing_symbol_id: None,
            target_symbol_id: None,
            confidence: 1.0,
            code_context: None,
        }
    }

    pub fn kind(mut self, kind: IdentifierKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn line(mut self, line: u32) -> Self {
        self.start_line = line;
        self.end_line = line;
        self
    }

    pub fn column(mut self, start_column: u32, end_column: u32) -> Self {
        self.start_column = start_column;
        self.end_column = end_column;
        self
    }

    pub fn bytes(mut self, start_byte: u32, end_byte: u32) -> Self {
        self.start_byte = start_byte;
        self.end_byte = end_byte;
        self
    }

    pub fn containing_symbol_id(mut self, containing_symbol_id: impl Into<String>) -> Self {
        self.containing_symbol_id = Some(containing_symbol_id.into());
        self
    }

    pub fn target_symbol_id(mut self, target_symbol_id: impl Into<String>) -> Self {
        self.target_symbol_id = Some(target_symbol_id.into());
        self
    }

    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn build(self) -> Identifier {
        Identifier {
            id: self.id,
            name: self.name,
            kind: self.kind,
            language: self.language,
            file_path: self.file_path,
            start_line: self.start_line,
            start_column: self.start_column,
            end_line: self.end_line,
            end_column: self.end_column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            containing_symbol_id: self.containing_symbol_id,
            target_symbol_id: self.target_symbol_id,
            confidence: self.confidence,
            code_context: self.code_context,
        }
    }
}
