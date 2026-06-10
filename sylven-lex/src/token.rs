use sylven_text::TextRange;

use crate::SyntaxKind;

/// A single lexed token: a kind and its byte range in the source text.
///
/// Tokens carry no text of their own — callers slice the source with
/// [`Token::range`] (or use [`TokenStream::text`]) so the lexer never
/// allocates per token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub range: TextRange,
}

impl Token {
    pub fn new(kind: SyntaxKind, range: TextRange) -> Token {
        Token { kind, range }
    }

    pub fn is_trivia(&self) -> bool {
        self.kind.is_trivia()
    }
}

/// A flat sequence of [`Token`]s covering an entire source text, including
/// trivia and a trailing [`SyntaxKind::EOF`] token.
///
/// Lossless: concatenating every token's source slice (in order) reproduces
/// the original text exactly.
#[derive(Debug, Clone, Default)]
pub struct TokenStream {
    tokens: Vec<Token>,
}

impl TokenStream {
    pub fn new(tokens: Vec<Token>) -> TokenStream {
        TokenStream { tokens }
    }

    pub fn as_slice(&self) -> &[Token] {
        &self.tokens
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<Token> {
        self.tokens.get(index).copied()
    }

    /// The source text covered by `token`.
    pub fn text<'a>(&self, token: Token, source: &'a str) -> &'a str {
        &source[token.range.start().to_usize()..token.range.end().to_usize()]
    }
}

impl IntoIterator for TokenStream {
    type Item = Token;
    type IntoIter = std::vec::IntoIter<Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.into_iter()
    }
}

impl<'a> IntoIterator for &'a TokenStream {
    type Item = &'a Token;
    type IntoIter = std::slice::Iter<'a, Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.iter()
    }
}
