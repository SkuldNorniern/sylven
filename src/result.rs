use sylven_parse::ParseError;
use sylven_text::TextRange;
use sylven_tree::SyntaxTree;

/// The output of [`LanguagePlugin::parse`](crate::LanguagePlugin::parse): a
/// complete, lossless tree, any diagnostics produced while building it, and
/// derived editor features.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub tree: SyntaxTree,
    pub errors: Vec<ParseError>,
    pub features: SyntaxFeatures,
}

/// Editor-facing data derived from a parse, beyond the tree itself.
///
/// At Stage 1 every field is always empty: highlighting, folding, symbols,
/// injections, and bracket pairs are all driven by the typed-rules layer
/// (plan.md Stage 3+), which doesn't exist yet. The fields are placeholder
/// [`TextRange`]-shaped data so [`crate::SyntaxSession`] and its callers have
/// a stable shape to program against while that layer is built out.
#[derive(Debug, Clone, Default)]
pub struct SyntaxFeatures {
    /// Ranges to apply syntax highlighting to.
    pub highlights: Vec<TextRange>,
    /// Foldable regions (e.g. function bodies, blocks).
    pub folds: Vec<TextRange>,
    /// Document symbol ranges (e.g. function and variable declarations).
    pub symbols: Vec<TextRange>,
    /// Embedded-language regions (e.g. a string that holds another
    /// language's source).
    pub injections: Vec<TextRange>,
    /// Matching bracket/delimiter pairs.
    pub brackets: Vec<(TextRange, TextRange)>,
}
