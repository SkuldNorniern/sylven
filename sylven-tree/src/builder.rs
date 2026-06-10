use std::sync::Arc;

use sylven_lex::SyntaxKind;

use crate::{GreenElement, GreenNode, GreenToken};

/// Builds a [`GreenNode`] tree bottom-up from a flat sequence of
/// `start_node` / `token` / `finish_node` calls.
///
/// This is the standard shape a parser drives: `start_node` and
/// `finish_node` must balance, and [`GreenNodeBuilder::finish`] requires
/// exactly one finished root node.
#[derive(Debug, Default)]
pub struct GreenNodeBuilder {
    /// One entry per currently-open node: its kind, and the children
    /// accumulated so far.
    stack: Vec<(SyntaxKind, Vec<GreenElement>)>,
    /// Set once the outermost node is finished.
    root: Option<Arc<GreenNode>>,
}

impl GreenNodeBuilder {
    pub fn new() -> GreenNodeBuilder {
        GreenNodeBuilder::default()
    }

    /// Open a new node of `kind`. Must be matched by [`Self::finish_node`].
    pub fn start_node(&mut self, kind: SyntaxKind) {
        assert!(
            self.root.is_none(),
            "start_node called after the root node was finished"
        );
        self.stack.push((kind, Vec::new()));
    }

    /// Append a leaf token to the currently-open node.
    pub fn token(&mut self, kind: SyntaxKind, text: impl Into<Box<str>>) {
        let token = GreenElement::Token(Arc::new(GreenToken::new(kind, text)));
        self.current_children().push(token);
    }

    /// Close the most recently opened node, attaching it to its parent (or,
    /// if it was the outermost node, recording it as the root).
    pub fn finish_node(&mut self) {
        let (kind, children) = self
            .stack
            .pop()
            .expect("finish_node called with no open node");
        let node = Arc::new(GreenNode::new(kind, children));

        if let Some((_, parent_children)) = self.stack.last_mut() {
            parent_children.push(GreenElement::Node(node));
        } else {
            self.root = Some(node);
        }
    }

    /// Consume the builder and return the finished root.
    ///
    /// Panics if no node was ever opened, or if a `start_node` was never
    /// matched by `finish_node`.
    pub fn finish(self) -> Arc<GreenNode> {
        assert!(
            self.stack.is_empty(),
            "finish called with {} unclosed node(s)",
            self.stack.len()
        );
        self.root.expect("finish called before any node was opened")
    }

    fn current_children(&mut self) -> &mut Vec<GreenElement> {
        &mut self
            .stack
            .last_mut()
            .expect("token called with no open node")
            .1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: SyntaxKind = SyntaxKind(100);
    const LEAF: SyntaxKind = SyntaxKind(101);
    const WS: SyntaxKind = SyntaxKind(102);

    #[test]
    fn builds_nested_tree_and_round_trips_text() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(ROOT);
        builder.token(LEAF, "fn");
        builder.token(WS, " ");
        builder.start_node(LEAF);
        builder.token(LEAF, "main");
        builder.finish_node();
        builder.finish_node();

        let root = builder.finish();
        assert_eq!(root.kind(), ROOT);
        assert_eq!(root.text(), "fn main");
        assert_eq!(root.children().len(), 3);
    }

    #[test]
    #[should_panic(expected = "unclosed node")]
    fn finish_panics_on_unclosed_node() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(ROOT);
        builder.token(LEAF, "x");
        let _ = builder.finish();
    }
}
