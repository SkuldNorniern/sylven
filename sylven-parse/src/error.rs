use sylven_text::TextRange;

/// A diagnostic produced during parsing: a human-readable message and the
/// source range it applies to.
///
/// Producing a [`ParseError`] never stops parsing — Sylven is recovery-first
/// (plan.md §2.1), so the parser always finishes and returns a complete tree
/// alongside any errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub range: TextRange,
}

impl ParseError {
    pub fn new(message: impl Into<String>, range: TextRange) -> ParseError {
        ParseError {
            message: message.into(),
            range,
        }
    }
}
