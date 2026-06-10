use std::sync::Arc;

use crate::{GreenNode, SyntaxNode};

/// A complete, lossless syntax tree: a [`GreenNode`] root plus the
/// [`SyntaxNode`] view over it.
///
/// Cheap to clone — both fields are reference-counted.
#[derive(Debug, Clone)]
pub struct SyntaxTree {
    green: Arc<GreenNode>,
}

impl SyntaxTree {
    pub fn new(green: Arc<GreenNode>) -> SyntaxTree {
        SyntaxTree { green }
    }

    pub fn root(&self) -> SyntaxNode {
        SyntaxNode::new_root(Arc::clone(&self.green))
    }

    pub fn green(&self) -> &Arc<GreenNode> {
        &self.green
    }

    /// The full source text, reconstructed from the tree. Equal to the text
    /// that was parsed, byte for byte, by construction (every token —
    /// including trivia and error tokens — is a leaf somewhere in the tree).
    pub fn text(&self) -> String {
        self.green.text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GreenNodeBuilder;
    use sylven_lex::SyntaxKind;

    const ROOT: SyntaxKind = SyntaxKind(100);
    const TOK: SyntaxKind = SyntaxKind(101);

    #[test]
    fn round_trips_source_text() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(ROOT);
        builder.token(TOK, "hello");
        builder.token(SyntaxKind::WHITESPACE, " ");
        builder.token(TOK, "world");
        builder.finish_node();

        let tree = SyntaxTree::new(builder.finish());
        assert_eq!(tree.text(), "hello world");
        assert_eq!(tree.root().kind(), ROOT);
        assert_eq!(tree.root().children_with_tokens().count(), 3);
    }
}
