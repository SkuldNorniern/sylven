use std::fmt;

/// An opaque syntax kind: a token or tree-node tag.
///
/// `SyntaxKind` is a plain `u16` newtype rather than a shared enum so that
/// each language plugin can define its own kind space. Values below
/// [`SyntaxKind::LANG_KIND_BASE`] are reserved for the handful of kinds every
/// language needs (trivia, errors, end-of-file); everything from
/// `LANG_KIND_BASE` upward is assigned by the language itself (today by hand,
/// later by the `sylven-dsl` compiler).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxKind(pub u16);

impl SyntaxKind {
    /// Placeholder kind used by tree builders before a real kind is known.
    /// Never appears in a finished tree.
    pub const TOMBSTONE: SyntaxKind = SyntaxKind(0);

    /// Marks the end of a token stream.
    pub const EOF: SyntaxKind = SyntaxKind(1);

    /// A lexer or parser error: an unrecognized character, or a node that
    /// recovery wrapped around unexpected input.
    pub const ERROR: SyntaxKind = SyntaxKind(2);

    /// Whitespace trivia (spaces, tabs, newlines).
    pub const WHITESPACE: SyntaxKind = SyntaxKind(3);

    /// Comment trivia.
    pub const COMMENT: SyntaxKind = SyntaxKind(4);

    /// First kind value available to a language plugin's own token and node
    /// kinds.
    pub const LANG_KIND_BASE: u16 = 16;

    /// Whether tokens of this kind are trivia: kept in the syntax tree for
    /// losslessness, but skipped by parser lookahead.
    ///
    /// Only the two shared trivia kinds count here. A language that needs
    /// additional trivia (e.g. a "significant" doc comment) gives it its own
    /// kind and handles it explicitly in its parser.
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
}

impl fmt::Debug for SyntaxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyntaxKind({})", self.0)
    }
}
