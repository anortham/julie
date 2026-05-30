use serde::{Deserialize, Serialize};

/// Role classification for test-related symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestRole {
    TestCase,
    ParameterizedTest,
    FixtureSetup,
    FixtureTeardown,
    TestContainer,
}

impl TestRole {
    /// Returns true for roles where quality scoring (assert density, stub detection) applies.
    pub fn is_scorable(&self) -> bool {
        matches!(self, TestRole::TestCase | TestRole::ParameterizedTest)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TestRole::TestCase => "test_case",
            TestRole::ParameterizedTest => "parameterized_test",
            TestRole::FixtureSetup => "fixture_setup",
            TestRole::FixtureTeardown => "fixture_teardown",
            TestRole::TestContainer => "test_container",
        }
    }
}

/// Identifier kinds - types of references/usages in code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierKind {
    /// Function/method call
    Call,
    /// Variable reference (reading a variable)
    VariableRef,
    /// Type usage (in type annotations, casts, etc.)
    TypeUsage,
    /// Member access (object.property, object.method)
    MemberAccess,
}

impl std::fmt::Display for IdentifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentifierKind::Call => write!(f, "call"),
            IdentifierKind::VariableRef => write!(f, "variable_ref"),
            IdentifierKind::TypeUsage => write!(f, "type_usage"),
            IdentifierKind::MemberAccess => write!(f, "member_access"),
        }
    }
}

impl IdentifierKind {
    /// Convert from string representation (for database deserialization)
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "call" => Some(IdentifierKind::Call),
            "variable_ref" => Some(IdentifierKind::VariableRef),
            "type_usage" => Some(IdentifierKind::TypeUsage),
            "member_access" => Some(IdentifierKind::MemberAccess),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown identifier kind: {s}"))
    }
}

/// Symbol kinds - Implementation of SymbolKind enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Class,
    Interface,
    Function,
    Method,
    Variable,
    Constant,
    Property,
    Enum,
    #[serde(rename = "enum_member")]
    EnumMember,
    Module,
    Namespace,
    Type,
    Trait,
    Struct,
    Union,
    Field,
    Constructor,
    Destructor,
    Operator,
    Import,
    Export,
    Event,
    Delegate,
}

impl SymbolKind {
    /// Convert from string representation (for database deserialization).
    /// Returns `None` for unrecognised strings instead of panicking.
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "class" => Some(SymbolKind::Class),
            "interface" => Some(SymbolKind::Interface),
            "function" => Some(SymbolKind::Function),
            "method" => Some(SymbolKind::Method),
            "variable" => Some(SymbolKind::Variable),
            "constant" => Some(SymbolKind::Constant),
            "property" => Some(SymbolKind::Property),
            "enum" => Some(SymbolKind::Enum),
            "enum_member" => Some(SymbolKind::EnumMember),
            "module" => Some(SymbolKind::Module),
            "namespace" => Some(SymbolKind::Namespace),
            "type" => Some(SymbolKind::Type),
            "trait" => Some(SymbolKind::Trait),
            "struct" => Some(SymbolKind::Struct),
            "union" => Some(SymbolKind::Union),
            "field" => Some(SymbolKind::Field),
            "constructor" => Some(SymbolKind::Constructor),
            "destructor" => Some(SymbolKind::Destructor),
            "operator" => Some(SymbolKind::Operator),
            "import" => Some(SymbolKind::Import),
            "export" => Some(SymbolKind::Export),
            "event" => Some(SymbolKind::Event),
            "delegate" => Some(SymbolKind::Delegate),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    #[allow(dead_code)]
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown symbol kind: {s}"))
    }
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Variable => write!(f, "variable"),
            SymbolKind::Constant => write!(f, "constant"),
            SymbolKind::Property => write!(f, "property"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::EnumMember => write!(f, "enum_member"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Namespace => write!(f, "namespace"),
            SymbolKind::Type => write!(f, "type"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Union => write!(f, "union"),
            SymbolKind::Field => write!(f, "field"),
            SymbolKind::Constructor => write!(f, "constructor"),
            SymbolKind::Destructor => write!(f, "destructor"),
            SymbolKind::Operator => write!(f, "operator"),
            SymbolKind::Import => write!(f, "import"),
            SymbolKind::Export => write!(f, "export"),
            SymbolKind::Event => write!(f, "event"),
            SymbolKind::Delegate => write!(f, "delegate"),
        }
    }
}

/// Visibility levels for symbols - reference implementation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
    FilePrivate,
    Open,
}

impl Visibility {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
            Visibility::FilePrivate => "fileprivate",
            Visibility::Open => "open",
        }
    }

    pub fn from_storage_str(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Visibility::Public),
            "private" => Some(Visibility::Private),
            "protected" => Some(Visibility::Protected),
            "internal" => Some(Visibility::Internal),
            "fileprivate" => Some(Visibility::FilePrivate),
            "open" => Some(Visibility::Open),
            _ => None,
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "Public"),
            Visibility::Private => write!(f, "Private"),
            Visibility::Protected => write!(f, "Protected"),
            Visibility::Internal => write!(f, "Internal"),
            Visibility::FilePrivate => write!(f, "FilePrivate"),
            Visibility::Open => write!(f, "Open"),
        }
    }
}

/// Relationship kinds - direct port from RelationshipKind enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Calls,
    Extends,
    Implements,
    Uses,
    Returns,
    Parameter,
    Imports,
    Instantiates,
    References,
    Defines,
    Overrides,
    Contains,
    Joins,
    Composition,
}

impl std::fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationshipKind::Calls => write!(f, "calls"),
            RelationshipKind::Extends => write!(f, "extends"),
            RelationshipKind::Implements => write!(f, "implements"),
            RelationshipKind::Uses => write!(f, "uses"),
            RelationshipKind::Returns => write!(f, "returns"),
            RelationshipKind::Parameter => write!(f, "parameter"),
            RelationshipKind::Imports => write!(f, "imports"),
            RelationshipKind::Instantiates => write!(f, "instantiates"),
            RelationshipKind::References => write!(f, "references"),
            RelationshipKind::Defines => write!(f, "defines"),
            RelationshipKind::Overrides => write!(f, "overrides"),
            RelationshipKind::Contains => write!(f, "contains"),
            RelationshipKind::Joins => write!(f, "joins"),
            RelationshipKind::Composition => write!(f, "composition"),
        }
    }
}

impl RelationshipKind {
    /// Convert from string representation (for database deserialization)
    #[allow(dead_code)]
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "calls" => Some(RelationshipKind::Calls),
            "extends" => Some(RelationshipKind::Extends),
            "implements" => Some(RelationshipKind::Implements),
            "uses" => Some(RelationshipKind::Uses),
            "returns" => Some(RelationshipKind::Returns),
            "parameter" => Some(RelationshipKind::Parameter),
            "imports" => Some(RelationshipKind::Imports),
            "instantiates" => Some(RelationshipKind::Instantiates),
            "references" => Some(RelationshipKind::References),
            "defines" => Some(RelationshipKind::Defines),
            "overrides" => Some(RelationshipKind::Overrides),
            "contains" => Some(RelationshipKind::Contains),
            "joins" => Some(RelationshipKind::Joins),
            "composition" => Some(RelationshipKind::Composition),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    #[allow(dead_code)]
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown relationship kind: {s}"))
    }
}
