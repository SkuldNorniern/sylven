use sylven_lex::Token;
use sylven_tree::{GreenNodeBuilder, SyntaxTree};

use crate::{ParseError, ParseEvent, TokenId};

/// Replay a parser's event log into a [`SyntaxTree`], returning the tree and
/// any [`ParseError`]s collected along the way.
///
/// `tokens` and `source` must be the same token stream and source text the
/// parser ran over — `events` reference `tokens` by index via [`TokenId`].
///
/// Generic over language: this function knows nothing about what any
/// [`SyntaxKind`](sylven_lex::SyntaxKind) means, only how to replay
/// `StartNode`/`Token`/`FinishNode` events into a green tree.
pub fn build_tree(
    tokens: &[Token],
    source: &str,
    events: Vec<ParseEvent>,
) -> (SyntaxTree, Vec<ParseError>) {
    let mut builder = GreenNodeBuilder::new();
    let mut errors = Vec::new();

    for event in events {
        match event {
            ParseEvent::StartNode(kind) => builder.start_node(kind),
            ParseEvent::FinishNode => builder.finish_node(),
            ParseEvent::Token(TokenId(index)) => {
                let token = tokens[index as usize];
                let start = token.range.start().to_usize();
                let end = token.range.end().to_usize();
                builder.token(token.kind, &source[start..end]);
            }
            ParseEvent::Error(error) => errors.push(error),
        }
    }

    (SyntaxTree::new(builder.finish()), errors)
}
