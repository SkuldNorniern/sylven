use std::sync::Arc;

use sylven_lex::SyntaxKind;
use sylven_text::TextSize;

/// A leaf in the green tree: a kind plus its exact source text.
///
/// Green tokens are immutable and reference-counted so unchanged subtrees
/// can be shared between the syntax trees of successive edits (full reuse
/// lands with incremental parsing in a later stage; for now sharing simply
/// keeps clones of [`GreenNode`] cheap).
#[derive(Debug, PartialEq, Eq)]
pub struct GreenToken {
    kind: SyntaxKind,
    text: Box<str>,
}

impl GreenToken {
    pub fn new(kind: SyntaxKind, text: impl Into<Box<str>>) -> GreenToken {
        GreenToken {
            kind,
            text: text.into(),
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_len(&self) -> TextSize {
        TextSize::of(&self.text)
    }
}

/// An interior node in the green tree: a kind plus its children, in source
/// order. Lossless — trivia tokens (whitespace, comments) appear as ordinary
/// children alongside significant tokens and nodes.
#[derive(Debug, PartialEq, Eq)]
pub struct GreenNode {
    kind: SyntaxKind,
    children: Vec<GreenElement>,
    text_len: TextSize,
}

impl GreenNode {
    pub fn new(kind: SyntaxKind, children: Vec<GreenElement>) -> GreenNode {
        let text_len = children
            .iter()
            .fold(TextSize::ZERO, |len, child| len + child.text_len());
        GreenNode {
            kind,
            children,
            text_len,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn children(&self) -> &[GreenElement] {
        &self.children
    }

    pub fn text_len(&self) -> TextSize {
        self.text_len
    }

    /// The source text covered by this node, reconstructed by concatenating
    /// every descendant token's text in order.
    pub fn text(&self) -> String {
        let mut buf = String::with_capacity(self.text_len.to_usize());
        self.write_text(&mut buf);
        buf
    }

    fn write_text(&self, buf: &mut String) {
        for child in &self.children {
            match child {
                GreenElement::Node(node) => node.write_text(buf),
                GreenElement::Token(token) => buf.push_str(token.text()),
            }
        }
    }
}

/// A child of a [`GreenNode`]: either a nested node or a leaf token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GreenElement {
    Node(Arc<GreenNode>),
    Token(Arc<GreenToken>),
}

impl GreenElement {
    pub fn kind(&self) -> SyntaxKind {
        match self {
            GreenElement::Node(node) => node.kind(),
            GreenElement::Token(token) => token.kind(),
        }
    }

    pub fn text_len(&self) -> TextSize {
        match self {
            GreenElement::Node(node) => node.text_len(),
            GreenElement::Token(token) => token.text_len(),
        }
    }
}

impl From<Arc<GreenNode>> for GreenElement {
    fn from(node: Arc<GreenNode>) -> GreenElement {
        GreenElement::Node(node)
    }
}

impl From<Arc<GreenToken>> for GreenElement {
    fn from(token: Arc<GreenToken>) -> GreenElement {
        GreenElement::Token(token)
    }
}
