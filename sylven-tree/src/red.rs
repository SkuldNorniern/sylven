use std::sync::Arc;

use sylven_lex::SyntaxKind;
use sylven_text::{TextRange, TextSize};

use crate::{GreenElement, GreenNode, GreenToken};

/// A view of a [`GreenNode`] at an absolute position in the source text.
///
/// `SyntaxNode`s are cheap to clone (an `Arc` clone plus a `TextSize`) and
/// computed on demand from the green tree, which stores only relative
/// (child-local) extents.
///
/// Stage 1 note: this is a *position-annotated view*, not a full red-tree
/// cursor — there is no [`SyntaxNode::parent`] yet. Parent navigation
/// (needed by, e.g., "find enclosing function" queries) is added once the
/// rules/query layer (plan.md Stage 3+) needs it.
#[derive(Debug, Clone)]
pub struct SyntaxNode {
    green: Arc<GreenNode>,
    offset: TextSize,
}

impl SyntaxNode {
    /// Wrap `green` as the root of a tree starting at offset zero.
    pub fn new_root(green: Arc<GreenNode>) -> SyntaxNode {
        SyntaxNode {
            green,
            offset: TextSize::ZERO,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.green.kind()
    }

    pub fn text_range(&self) -> TextRange {
        TextRange::new(self.offset, self.offset + self.green.text_len())
    }

    pub fn green(&self) -> &Arc<GreenNode> {
        &self.green
    }

    /// The source text covered by this node, reconstructed from its tokens.
    pub fn text(&self) -> String {
        self.green.text()
    }

    /// Direct children, both nodes and tokens, in source order.
    pub fn children_with_tokens(&self) -> impl Iterator<Item = SyntaxElement> + '_ {
        let mut offset = self.offset;
        self.green.children().iter().map(move |element| {
            let element_offset = offset;
            offset += element.text_len();
            match element {
                GreenElement::Node(node) => SyntaxElement::Node(SyntaxNode {
                    green: Arc::clone(node),
                    offset: element_offset,
                }),
                GreenElement::Token(token) => SyntaxElement::Token(SyntaxToken {
                    green: Arc::clone(token),
                    offset: element_offset,
                }),
            }
        })
    }

    /// Direct child nodes only (tokens skipped).
    pub fn children(&self) -> impl Iterator<Item = SyntaxNode> + '_ {
        self.children_with_tokens()
            .filter_map(|element| match element {
                SyntaxElement::Node(node) => Some(node),
                SyntaxElement::Token(_) => None,
            })
    }

    /// This node and all descendants (nodes and tokens), in preorder.
    pub fn preorder(&self) -> Preorder {
        Preorder {
            stack: vec![SyntaxElement::Node(self.clone())],
        }
    }
}

/// A leaf token at an absolute position in the source text.
#[derive(Debug, Clone)]
pub struct SyntaxToken {
    green: Arc<GreenToken>,
    offset: TextSize,
}

impl SyntaxToken {
    pub fn kind(&self) -> SyntaxKind {
        self.green.kind()
    }

    pub fn text(&self) -> &str {
        self.green.text()
    }

    pub fn text_range(&self) -> TextRange {
        TextRange::new(self.offset, self.offset + self.green.text_len())
    }

    pub fn green(&self) -> &Arc<GreenToken> {
        &self.green
    }
}

/// Either a [`SyntaxNode`] or a [`SyntaxToken`], at an absolute position.
#[derive(Debug, Clone)]
pub enum SyntaxElement {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

impl SyntaxElement {
    pub fn kind(&self) -> SyntaxKind {
        match self {
            SyntaxElement::Node(node) => node.kind(),
            SyntaxElement::Token(token) => token.kind(),
        }
    }

    pub fn text_range(&self) -> TextRange {
        match self {
            SyntaxElement::Node(node) => node.text_range(),
            SyntaxElement::Token(token) => token.text_range(),
        }
    }

    pub fn as_node(&self) -> Option<&SyntaxNode> {
        match self {
            SyntaxElement::Node(node) => Some(node),
            SyntaxElement::Token(_) => None,
        }
    }

    pub fn as_token(&self) -> Option<&SyntaxToken> {
        match self {
            SyntaxElement::Token(token) => Some(token),
            SyntaxElement::Node(_) => None,
        }
    }
}

/// Preorder (depth-first, parent-before-children) traversal of a
/// [`SyntaxNode`] and all its descendants, produced by
/// [`SyntaxNode::preorder`].
pub struct Preorder {
    stack: Vec<SyntaxElement>,
}

impl Iterator for Preorder {
    type Item = SyntaxElement;

    fn next(&mut self) -> Option<SyntaxElement> {
        let element = self.stack.pop()?;
        if let SyntaxElement::Node(node) = &element {
            let children: Vec<_> = node.children_with_tokens().collect();
            for child in children.into_iter().rev() {
                self.stack.push(child);
            }
        }
        Some(element)
    }
}
