use sylven_text::{TextRange, TextSize};
use sylven_tree::{GreenElement, GreenNode, SyntaxNode};

/// Find the index (into `root`'s direct children list, including trivia
/// tokens) of the smallest child that fully contains `edit_range`.
///
/// Returns `None` when the edit range spans more than one direct child, or
/// when `root` has no children. When `Some(i)` is returned, the child at
/// index `i` inside [`GreenNode::children`] fully covers the dirty bytes.
///
/// ## Why track GreenElement index?
///
/// The caller needs the index into the *green* children list (which includes
/// trivia tokens) so it can splice `old_green.children()` directly — no
/// conversion between the red "node-only" view and the full green children
/// list is needed.
pub fn find_reparse_root(root: &SyntaxNode, edit_range: TextRange) -> Option<usize> {
    let green = root.green();
    find_dirty_child(green, edit_range)
}

/// Walk `node`'s green children (including trivia) with a running offset and
/// return the index of the child that fully contains `edit_range`.
pub(crate) fn find_dirty_child(node: &GreenNode, edit_range: TextRange) -> Option<usize> {
    let mut offset = TextSize::ZERO;
    for (i, elem) in node.children().iter().enumerate() {
        let elem_end = offset + elem.text_len();
        let elem_range = TextRange::new(offset, elem_end);
        if elem_range.start() <= edit_range.start() && elem_range.end() >= edit_range.end() {
            return Some(i);
        }
        offset = elem_end;
    }
    None
}

/// Splice `old_green` and `new_green` together: reuse unchanged children from
/// `old_green` and take the dirty child (at index `dirty_idx`) from
/// `new_green`.
/// A non-dirty child is reused only when it is structurally equal to the newly
/// parsed child, since lexer state can make an edit affect later children.
///
/// Green nodes are **position-independent** — they store only text length, not
/// absolute offsets — so it is always safe to `Arc::clone` an old child into a
/// new tree, even after an offset-shifting edit.
///
/// Returns the spliced root as a new `Arc<GreenNode>`.
pub fn splice_green(
    old_green: &GreenNode,
    new_green: &GreenNode,
    dirty_idx: usize,
) -> std::sync::Arc<GreenNode> {
    let old_children = old_green.children();
    let new_children = new_green.children();

    // If the number of children changed (e.g. a new top-level item was added),
    // fall back to the new tree — splicing would produce a wrong tree.
    if old_children.len() != new_children.len() {
        return std::sync::Arc::new(GreenNode::new(new_green.kind(), new_children.to_vec()));
    }

    let mut children: Vec<GreenElement> = Vec::with_capacity(old_children.len());
    for (i, (old, new)) in old_children.iter().zip(new_children.iter()).enumerate() {
        if i == dirty_idx || old != new {
            children.push(new.clone()); // reparsed child
        } else {
            children.push(old.clone()); // unchanged — reuse Arc (no re-allocation)
        }
    }
    std::sync::Arc::new(GreenNode::new(old_green.kind(), children))
}

/// Replace the child at `dirty_idx` in `old_green` with `replacement`
/// (zero or more elements), leaving every other child untouched.
///
/// Used for regional reparse: `replacement` is the top-level children of a
/// standalone reparse of just the dirty child's (possibly resized) text, and
/// may contain a different number of elements than the single child it
/// replaces (e.g. an edit that splits one token into several, or merges
/// several into one).
///
/// Green nodes are position-independent, so every kept child's `Arc` is
/// reused as-is.
pub fn splice_region(
    old_green: &GreenNode,
    dirty_idx: usize,
    replacement: &[GreenElement],
) -> std::sync::Arc<GreenNode> {
    let old_children = old_green.children();
    let mut children: Vec<GreenElement> =
        Vec::with_capacity(old_children.len() - 1 + replacement.len());
    children.extend_from_slice(&old_children[..dirty_idx]);
    children.extend_from_slice(replacement);
    children.extend_from_slice(&old_children[dirty_idx + 1..]);
    std::sync::Arc::new(GreenNode::new(old_green.kind(), children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sylven_lex::SyntaxKind;
    use sylven_text::TextSize;
    use sylven_tree::{GreenNodeBuilder, SyntaxTree};

    fn build_file(items: &[&str]) -> (SyntaxTree, Arc<GreenNode>) {
        const FILE: SyntaxKind = SyntaxKind(16);
        const STMT: SyntaxKind = SyntaxKind(17);
        const TOK: SyntaxKind = SyntaxKind(18);
        const WS: SyntaxKind = SyntaxKind::WHITESPACE;

        let mut b = GreenNodeBuilder::new();
        b.start_node(FILE);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                b.token(WS, "\n");
            }
            b.start_node(STMT);
            b.token(TOK, *item);
            b.finish_node();
        }
        b.finish_node();
        let green = b.finish();
        let tree = SyntaxTree::new(Arc::clone(&green));
        (tree, green)
    }

    #[test]
    fn finds_dirty_child_in_middle() {
        let (tree, _) = build_file(&["aaa", "bbb", "ccc"]);
        // "aaa" = 3, "\n" = 1, "bbb" = 3 (starts at 4, ends at 7)
        // WS "\n" at index 1, STMT "bbb" at index 2
        let edit_range = TextRange::new(TextSize::from(5), TextSize::from(6)); // inside "bbb"
        let root = tree.root();
        let idx = find_reparse_root(&root, edit_range);
        assert!(idx.is_some(), "should find a containing child");
    }

    #[test]
    fn returns_none_when_edit_spans_multiple_children() {
        let (tree, _) = build_file(&["aaa", "bbb"]);
        // Edit that spans from inside "aaa" to inside "bbb"
        let edit_range = TextRange::new(TextSize::from(1), TextSize::from(6));
        let root = tree.root();
        assert!(
            find_reparse_root(&root, edit_range).is_none(),
            "multi-child edit should return None"
        );
    }

    #[test]
    fn splice_reuses_unchanged_children() {
        let (_, old_green) = build_file(&["aaa", "bbb", "ccc"]);
        let (_, new_green) = build_file(&["aaa", "BBB", "ccc"]);
        // dirty index 2 (STMT "BBB", counting from 0 in green children: WS, STMT, WS, STMT, ...)
        // Actually let's just find the dirty index:
        let edit_range = TextRange::new(TextSize::from(5), TextSize::from(7));
        let dirty_idx = find_dirty_child(&old_green, edit_range).unwrap();

        let spliced = splice_green(&old_green, &new_green, dirty_idx);

        // Unchanged children must share the same Arc pointer as the old tree.
        for (i, (old_elem, spliced_elem)) in old_green
            .children()
            .iter()
            .zip(spliced.children().iter())
            .enumerate()
        {
            if i == dirty_idx {
                // The dirty child should be the new one.
                if let (GreenElement::Node(old_n), GreenElement::Node(spliced_n)) =
                    (old_elem, spliced_elem)
                {
                    assert!(
                        !Arc::ptr_eq(old_n, spliced_n),
                        "dirty child should be replaced"
                    );
                }
            } else if let (GreenElement::Node(old_n), GreenElement::Node(spliced_n)) =
                (old_elem, spliced_elem)
            {
                assert!(
                    Arc::ptr_eq(old_n, spliced_n),
                    "unchanged child {i} should reuse old Arc"
                );
            }
        }
    }

    #[test]
    fn splice_keeps_changed_non_dirty_children_from_new_tree() {
        let (_, old_green) = build_file(&["aaa", "bbb", "ccc"]);
        let (_, new_green) = build_file(&["aaa", "BBB", "CCC"]);
        let edit_range = TextRange::new(TextSize::from(5), TextSize::from(7));
        let dirty_idx = find_dirty_child(&old_green, edit_range).unwrap();

        let spliced = splice_green(&old_green, &new_green, dirty_idx);

        assert_eq!(spliced.as_ref(), new_green.as_ref());
        if let (GreenElement::Node(old_last), GreenElement::Node(spliced_last)) = (
            old_green.children().last().unwrap(),
            spliced.children().last().unwrap(),
        ) {
            assert!(
                !Arc::ptr_eq(old_last, spliced_last),
                "a changed child outside the dirty index must come from the new tree"
            );
        }
    }
}
