use sylven_parse::ParseError;
use sylven_text::TextRange;
use sylven_tree::SyntaxTree;

/// Editor-visible highlight category. Language-agnostic — maps to theme slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Keyword,
    KeywordControl,
    Type,
    String,
    Comment,
    Number,
    Operator,
    Punctuation,
    Function,
    Variable,
    Attribute,
    Macro,
    Lifetime,
    /// A `[section]` / `[[array of tables]]` header (TOML, INI, …).
    SectionHeader,
}

/// A single highlighted region with its semantic kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub range: TextRange,
    pub kind: HighlightKind,
}

/// Document symbol kind for outline and picker display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Constant,
    TypeAlias,
    Macro,
    /// A `[section]` / `[[array of tables]]` header (TOML, INI, …).
    Section,
    /// An ATX heading (`#` … `######`) in Markdown.
    Heading,
}

impl SymbolKind {
    /// Short human label shown in the symbol picker's detail column.
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Impl => "impl",
            SymbolKind::Module => "mod",
            SymbolKind::Constant => "const",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Macro => "macro",
            SymbolKind::Section => "section",
            SymbolKind::Heading => "heading",
        }
    }
}

/// One embedded-language injection: the child language and its byte range in
/// the parent document. Produced by language plugins that embed foreign code
/// (e.g. Markdown fenced code blocks). [`SyntaxEngine`](crate::SyntaxEngine)
/// parses each injection with the matching child plugin and merges the
/// offset-translated highlights into the parent result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Injection {
    /// Language identifier (e.g. `"rust"`, `"python"`).
    /// `None` when the fence had no language tag.
    pub language: Option<String>,
    /// Byte range of the code block body within the parent document.
    pub range: TextRange,
}

/// One document symbol: name, kind, and byte ranges for the declaration and
/// name token. The line number must be derived by the caller using a
/// [`LineIndex`](sylven_text::LineIndex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    /// Byte range of the name token only (for cursor placement).
    pub name_range: TextRange,
    /// Byte range of the whole declaration keyword (for folding anchors).
    pub decl_range: TextRange,
    pub kind: SymbolKind,
}

/// Editor-facing data derived from a parse, beyond the tree itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyntaxFeatures {
    /// Per-token highlight ranges with semantic kinds.
    pub highlights: Vec<Highlight>,
    /// Foldable block ranges (e.g. `{…}` spans that cross line boundaries).
    pub folds: Vec<TextRange>,
    /// Document symbols (functions, types, modules, …).
    pub symbols: Vec<SymbolInfo>,
    /// Embedded-language injection regions with their child language tags.
    pub injections: Vec<Injection>,
    /// Matching bracket/delimiter pairs.
    pub brackets: Vec<(TextRange, TextRange)>,
}

/// The output of [`LanguagePlugin::parse`](crate::LanguagePlugin::parse).
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub tree: SyntaxTree,
    pub errors: Vec<ParseError>,
    pub features: SyntaxFeatures,
}
