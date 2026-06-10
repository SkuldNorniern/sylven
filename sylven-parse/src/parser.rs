use sylven_lex::{SyntaxKind, Token};

use crate::{ParseError, ParseEvent, TokenId};

/// A position in the event stream recorded by [`Parser::checkpoint`], used
/// with [`Parser::start_node_at`] to wrap already-emitted events in a node
/// that starts "in the past" — the standard trick for left-recursive
/// constructs like binary expressions (parse the left operand first, then
/// decide whether it's actually the LHS of a `BinaryExpr`).
///
/// A checkpoint is only valid until the next [`Parser::start_node_at`] call:
/// inserting an event shifts every later index.
#[derive(Debug, Clone, Copy)]
pub struct Checkpoint(usize);

/// A recovery-first event-stream parser driver.
///
/// `Parser` owns lookahead over a [`TokenStream`](sylven_lex::TokenStream)
/// (skipping trivia for decision-making, but never discarding it) and
/// records a flat [`ParseEvent`] log. It has no grammar of its own —
/// language plugins (e.g. `mini_oxygen`) drive it with recursive-descent
/// functions.
pub struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
    events: Vec<ParseEvent>,
}

impl<'t> Parser<'t> {
    /// `tokens` must be non-empty and end with a [`SyntaxKind::EOF`] token
    /// (as produced by, e.g., `sylven_lex::mini_oxygen::lex`).
    pub fn new(tokens: &'t [Token]) -> Parser<'t> {
        assert!(
            tokens.last().is_some_and(|t| t.kind == SyntaxKind::EOF),
            "Parser::new requires a token stream ending in SyntaxKind::EOF"
        );
        Parser {
            tokens,
            pos: 0,
            events: Vec::new(),
        }
    }

    /// Index of the EOF token, i.e. `tokens.len() - 1`.
    fn eof_index(&self) -> usize {
        self.tokens.len() - 1
    }

    /// Index of the `n`th non-trivia token at or after `self.pos` (0 =
    /// next). Saturates at the EOF index.
    fn nth_index(&self, n: usize) -> usize {
        let mut idx = self.pos;
        let mut seen = 0;
        loop {
            while idx < self.eof_index() && self.tokens[idx].is_trivia() {
                idx += 1;
            }
            if idx >= self.eof_index() {
                return self.eof_index();
            }
            if seen == n {
                return idx;
            }
            seen += 1;
            idx += 1;
        }
    }

    /// Kind of the `n`th non-trivia token ahead (0 = next).
    pub fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens[self.nth_index(n)].kind
    }

    /// Kind of the next non-trivia token.
    pub fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    pub fn at_eof(&self) -> bool {
        self.current() == SyntaxKind::EOF
    }

    /// The source range of the next non-trivia token (used to anchor errors
    /// at the current position).
    pub fn current_range(&self) -> sylven_text::TextRange {
        self.tokens[self.nth_index(0)].range
    }

    /// Open a node of `kind`. Must be matched by [`Self::finish_node`].
    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.events.push(ParseEvent::StartNode(kind));
    }

    pub fn finish_node(&mut self) {
        self.events.push(ParseEvent::FinishNode);
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.events.len())
    }

    /// Retroactively open a node of `kind` starting at `checkpoint`,
    /// wrapping every event recorded since. See [`Checkpoint`].
    pub fn start_node_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.events
            .insert(checkpoint.0, ParseEvent::StartNode(kind));
    }

    /// Consume the next non-trivia token, plus any trivia immediately
    /// preceding it, emitting a [`ParseEvent::Token`] for each.
    pub fn bump(&mut self) {
        let target = self.nth_index(0);
        while self.pos <= target {
            self.events
                .push(ParseEvent::Token(TokenId(self.pos as u32)));
            self.pos += 1;
        }
    }

    /// `bump` if the next token is `kind`; otherwise record an error and
    /// leave the position unchanged. Returns whether it matched.
    pub fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            self.error(format!("expected {kind:?}, found {:?}", self.current()));
            false
        }
    }

    /// Record an error at the current position without consuming anything.
    pub fn error(&mut self, message: impl Into<String>) {
        self.events.push(ParseEvent::Error(ParseError::new(
            message,
            self.current_range(),
        )));
    }

    /// Consume any trivia remaining before EOF. Call once, immediately
    /// before closing the root node, so trailing whitespace/comments are
    /// included in the tree (otherwise the tree would not losslessly cover
    /// the whole source).
    pub fn eat_trailing_trivia(&mut self) {
        while self.pos < self.eof_index() {
            self.events
                .push(ParseEvent::Token(TokenId(self.pos as u32)));
            self.pos += 1;
        }
    }

    /// Finish parsing and return the recorded event log.
    pub fn finish(self) -> Vec<ParseEvent> {
        self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_lex::mini_oxygen::{MiniOxygenKind, lex};

    #[test]
    fn bump_skips_and_attaches_leading_trivia() {
        let stream = lex("  fn");
        let tokens = stream.as_slice();
        let mut parser = Parser::new(tokens);

        assert_eq!(parser.current(), MiniOxygenKind::FnKw.into());
        parser.start_node(MiniOxygenKind::File.into());
        parser.bump();
        parser.eat_trailing_trivia();
        parser.finish_node();

        let events = parser.finish();
        // WHITESPACE then FnKw, both as Token events inside the node.
        assert_eq!(
            events,
            vec![
                ParseEvent::StartNode(MiniOxygenKind::File.into()),
                ParseEvent::Token(TokenId(0)),
                ParseEvent::Token(TokenId(1)),
                ParseEvent::FinishNode,
            ]
        );
    }

    #[test]
    fn expect_records_error_without_consuming() {
        let stream = lex("fn");
        let tokens = stream.as_slice();
        let mut parser = Parser::new(tokens);

        let ok = parser.expect(MiniOxygenKind::LetKw.into());
        assert!(!ok);
        assert_eq!(parser.current(), MiniOxygenKind::FnKw.into());
        assert!(matches!(parser.finish().as_slice(), [ParseEvent::Error(_)]));
    }

    #[test]
    fn checkpoint_wraps_prior_events() {
        let stream = lex("1");
        let tokens = stream.as_slice();
        let mut parser = Parser::new(tokens);

        let checkpoint = parser.checkpoint();
        parser.start_node(MiniOxygenKind::Literal.into());
        parser.bump();
        parser.finish_node();
        parser.start_node_at(checkpoint, MiniOxygenKind::ExprStmt.into());
        parser.finish_node();

        let events = parser.finish();
        assert_eq!(
            events[0],
            ParseEvent::StartNode(MiniOxygenKind::ExprStmt.into())
        );
        assert_eq!(
            events[1],
            ParseEvent::StartNode(MiniOxygenKind::Literal.into())
        );
        assert_eq!(*events.last().unwrap(), ParseEvent::FinishNode);
    }
}
