use serde::{Deserialize, Serialize};

use super::span::NormalizedSpan;
use super::types::stable_location_id;

/// A single applied generic type argument captured at a *use site*, preserving
/// argument order (`ordinal`) and arbitrary nesting (`children`).
///
/// Example: `Dictionary<string, List<int>>` decomposes to
/// `[ {0, "string", []}, {1, "List", [ {0, "int", []} ]} ]`.
///
/// `ordinal` is the 0-based position among siblings (the whole point — e.g.
/// `CreateMap<A,B>` source-vs-dest direction). `children` is empty for a
/// non-generic argument. Flattened into the `type_arguments` table at persist
/// time; not resolved here (resolution is the consumer's job).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeArgument {
    pub ordinal: u32,
    pub type_name: String,
    pub children: Vec<TypeArgument>,
}

/// Ordered, nested generic type arguments applied at one use site, linked to the
/// use-site [`Identifier`](super::types::Identifier) by id (`identifier_id`).
/// One usage per generic use site; carries `file_path`/`language` from the
/// identifier so the persistence layer can flatten it into `type_arguments`
/// rows without re-deriving them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeArgumentUsage {
    pub identifier_id: String,
    pub file_path: String,
    pub language: String,
    pub arguments: Vec<TypeArgument>,
}

/// A string literal captured at a call-argument site (Miller bridge Phase 3).
///
/// Extractors emit one `Literal` per string-literal argument of a call,
/// **config-free**: `carrier` is the verbatim callee text (`fetch`, `axios.get`,
/// `QueryAsync`) and `kind` is always [`LiteralKind::Other`] at extraction time.
/// The `src/` indexing pipeline runs a single config-driven pass
/// (`classify_literals_by_carrier`) that consults each language's
/// `[literal_carriers]` TOML to set `kind` (`Url`/`Sql`/`Route`) on carrier
/// matches and **drop** literals whose carrier is not recognized — that drop is
/// the bloat gate. `kind` stays a read-time-reclassifiable hint among the stored
/// set because `carrier` is persisted.
///
/// `literal_text` is DECODED (delimiters stripped; interpolation holes replaced
/// by `{}`; concatenations folded) so a resolver sees `/api/users/{}` or
/// `SELECT ... FROM Users`. Span/`file_path`/`language`/`containing_symbol_id`
/// mirror [`Identifier`](super::types::Identifier) so the persistence and
/// cleanup paths are uniform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Literal {
    /// Unique id (MD5 of decoded text + span), stable across re-index.
    pub id: String,
    /// Decoded literal contents (no delimiters; `{}` for interpolation holes).
    pub literal_text: String,
    /// Best-effort extraction-time hint; authoritative `kind` is set by the
    /// config-driven classification pass. Always `Other` straight from a reader.
    pub kind: LiteralKind,
    /// Verbatim callee text that introduced this literal (`fetch`, `axios.get`,
    /// `QueryAsync`). `None` only when the callee text could not be derived.
    pub carrier: Option<String>,
    /// 0-based position of this argument within the call's full argument list.
    pub arg_position: u32,
    /// Programming language this literal is from.
    pub language: String,
    /// File path where this literal appears.
    pub file_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
    /// Id of the symbol that encloses this call (same notion as
    /// `Identifier::containing_symbol_id`).
    pub containing_symbol_id: Option<String>,
    /// Confidence score for the capture (1.0 for direct tree-sitter extraction).
    pub confidence: f32,
}

impl Literal {
    pub fn apply_normalized_span(&mut self, span: NormalizedSpan) {
        self.start_line = span.start_line;
        self.start_column = span.start_column;
        self.end_line = span.end_line;
        self.end_column = span.end_column;
        self.start_byte = span.start_byte;
        self.end_byte = span.end_byte;
    }

    /// Recompute the span-stable id (used after embedded-span offset shifts the
    /// span). Mirrors [`Identifier::refresh_id`](super::types::Identifier::refresh_id),
    /// keyed on `literal_text`.
    pub fn refresh_id(&mut self) {
        self.id = stable_location_id(
            self.file_path.as_str(),
            self.literal_text.as_str(),
            self.span(),
        );
    }

    fn span(&self) -> NormalizedSpan {
        NormalizedSpan {
            start_line: self.start_line,
            start_column: self.start_column,
            end_line: self.end_line,
            end_column: self.end_column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        }
    }
}

/// Classification of a captured [`Literal`]. An extraction-time hint that the
/// config-driven carrier pass refines; consumers may reclassify at read time
/// using the persisted `carrier`/`literal_text`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LiteralKind {
    /// URL / endpoint path passed to an HTTP client call.
    Url,
    /// SQL passed to a database query/execute call.
    Sql,
    /// Route template (mostly sourced from annotations; reserved here).
    Route,
    /// Captured but not (yet) classified as a recognized carrier kind.
    Other,
}

impl LiteralKind {
    /// Stable lowercase token used as the `literals.kind` DB column value and in
    /// the `[literal_carriers]` config keys. Matches the `snake_case` serde
    /// rename so DB text and JSON stay aligned.
    pub fn as_str(&self) -> &'static str {
        match self {
            LiteralKind::Url => "url",
            LiteralKind::Sql => "sql",
            LiteralKind::Route => "route",
            LiteralKind::Other => "other",
        }
    }

    /// Parse the `literals.kind` DB column back into a `LiteralKind`. Unknown
    /// values fall back to `Other` so a forward-written kind never panics a
    /// reader.
    pub fn from_db_str(value: &str) -> Self {
        match value {
            "url" => LiteralKind::Url,
            "sql" => LiteralKind::Sql,
            "route" => LiteralKind::Route,
            _ => LiteralKind::Other,
        }
    }
}
