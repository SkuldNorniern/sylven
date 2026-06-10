use sylven_lex::SyntaxKind;

use crate::ParseError;

/// Index of a token (including trivia and the trailing EOF token) within the
/// [`TokenStream`](sylven_lex::TokenStream) the parser was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenId(pub u32);

/// One step of building a [`SyntaxTree`](sylven_tree::SyntaxTree).
///
/// A parser produces a flat `Vec<ParseEvent>`; [`crate::build_tree`] replays
/// it through a [`GreenNodeBuilder`](sylven_tree::GreenNodeBuilder) to
/// produce the tree. `StartNode`/`FinishNode` pairs must balance, exactly as
/// they do in [`GreenNodeBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseEvent {
    StartNode(SyntaxKind),
    FinishNode,
    Token(TokenId),
    Error(ParseError),
}
