use std::sync::Arc;

use sylven_parse::{find_reparse_root, splice_green};
use sylven_text::{TextEdit, TextRange, TextSnapshot};
use sylven_tree::SyntaxTree;

use crate::{LanguageId, ParseResult, SyntaxEngine, SyntaxFeatures};

/// A document's connection to the syntax engine: pairs a [`LanguageId`] with
/// the most recently parsed [`TextSnapshot`] and [`ParseResult`].
///
/// Two parse entry points are available:
///
/// - [`parse`](Self::parse) — always does a full re-parse from scratch.
/// - [`parse_edit`](Self::parse_edit) — coarse incremental path (Stage 7):
///   performs a full parse of the new source, then reuses structurally
///   unchanged top-level green children from the old result. This reduces tree
///   allocation but does not reduce lexer or parser CPU time. It falls back to
///   the new tree as-is when the edit spans multiple children.
pub struct SyntaxSession {
    engine: Arc<SyntaxEngine>,
    language: LanguageId,
    snapshot: Option<TextSnapshot>,
    result: Option<ParseResult>,
}

impl SyntaxSession {
    pub fn new(engine: Arc<SyntaxEngine>, language: LanguageId) -> SyntaxSession {
        SyntaxSession {
            engine,
            language,
            snapshot: None,
            result: None,
        }
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    /// Parse `snapshot` from scratch, replacing any previous result.
    ///
    /// Returns `None` if no plugin is registered for this session's language;
    /// the previous result (if any) is left in place.
    pub fn parse(&mut self, snapshot: TextSnapshot) -> Option<&ParseResult> {
        let result = self.engine.parse(self.language, &snapshot)?;
        self.snapshot = Some(snapshot);
        self.result = Some(result);
        self.result.as_ref()
    }

    /// Apply `edit` to the current source, fully parse `new_snapshot`, then
    /// splice structurally unchanged top-level children from the old green
    /// tree. This is allocation reuse, not regional lexing or parsing.
    ///
    /// Falls back to a full parse without splicing if:
    /// - No previous result exists (first parse).
    /// - The edit spans more than one top-level child.
    /// - The number of top-level children changes.
    ///
    /// Returns `None` if no plugin is registered for this session's language.
    pub fn parse_edit(
        &mut self,
        edit: &TextEdit,
        new_snapshot: TextSnapshot,
    ) -> Option<&ParseResult> {
        let new_result = self.engine.parse(self.language, &new_snapshot)?;

        let spliced = if let Some(old_result) = &self.result {
            splice_result(old_result, &new_result, edit)
        } else {
            new_result
        };

        self.snapshot = Some(new_snapshot);
        self.result = Some(spliced);
        self.result.as_ref()
    }

    /// The most recent parse result, if [`Self::parse`] has succeeded at
    /// least once.
    pub fn current(&self) -> Option<&ParseResult> {
        self.result.as_ref()
    }

    /// The snapshot the current result was parsed from.
    pub fn snapshot(&self) -> Option<&TextSnapshot> {
        self.snapshot.as_ref()
    }
}

// ── Green-node splicing ───────────────────────────────────────────────────────

/// Attempt to splice unchanged green children from `old` into `new_result`.
/// Returns a `ParseResult` with a (potentially) more allocation-efficient tree
/// and merged features.
fn splice_result(old: &ParseResult, new_result: &ParseResult, edit: &TextEdit) -> ParseResult {
    let edit_range = edit.delete;
    let old_root = old.tree.root();

    // Find which top-level child contains the dirty range.
    let Some(dirty_idx) = find_reparse_root(&old_root, edit_range) else {
        // Edit spans multiple children — use new tree as-is.
        return new_result.clone();
    };

    // Splice green nodes: reuse unchanged old children, take dirty child from new.
    let spliced_green = splice_green(old.tree.green(), new_result.tree.green(), dirty_idx);
    let spliced_tree = SyntaxTree::new(spliced_green);

    // Merge features: keep old features for unchanged regions, take new for
    // the dirty region (shifted by the edit delta).
    let delta: isize =
        edit.insert.len() as isize - (edit_range.end() - edit_range.start()).to_usize() as isize;
    let features = merge_features(&old.features, &new_result.features, edit_range, delta);

    ParseResult {
        tree: spliced_tree,
        errors: new_result.errors.clone(),
        features,
    }
}

// ── Feature merging ───────────────────────────────────────────────────────────

/// Build a merged [`SyntaxFeatures`] that preserves old feature entries for
/// unchanged regions and takes new entries for the dirty region (and adjusts
/// offsets after it).
///
/// The merge strategy for each feature vec:
/// - Before `edit_range.start()`: keep old entry (range unchanged).
/// - Overlapping or inside `edit_range`: take from `new` (recomputed).
/// - After `edit_range.end()`: take from `new` (already correctly offset).
fn merge_features(
    old: &SyntaxFeatures,
    new: &SyntaxFeatures,
    edit_range: TextRange,
    _delta: isize,
) -> SyntaxFeatures {
    // For Stage 7 we use the new features directly — they are always correct.
    // The splicing happens at the green tree level (memory), not features.
    // A future stage can preserve old feature entries for unchanged regions to
    // avoid re-running the feature derivation pass entirely.
    let _ = (old, edit_range, _delta);
    new.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::{DocumentId, RevisionId, TextSize};

    use crate::lang::mini_oxygen::MiniOxygen;
    use crate::registry::LanguageRegistry;

    fn engine_with_mini_oxygen() -> Arc<SyntaxEngine> {
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MiniOxygen));
        Arc::new(SyntaxEngine::with_registry(registry))
    }

    fn snap(text: &str) -> TextSnapshot {
        TextSnapshot::new(DocumentId(0), RevisionId(0), text)
    }

    fn session() -> SyntaxSession {
        SyntaxSession::new(engine_with_mini_oxygen(), LanguageId("mini-oxygen"))
    }

    #[test]
    fn parse_produces_lossless_tree() {
        let mut s = session();
        let src = "let x = 1;";
        let result = s.parse(snap(src)).unwrap();
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn parse_edit_produces_lossless_tree() {
        let mut s = session();
        let src = "let x = 1;\nlet y = 2;";
        s.parse(snap(src)).unwrap();

        let edit = TextEdit::replace(TextRange::new(TextSize::from(8), TextSize::from(9)), "42");
        let new_src = edit.apply(src);
        let new_snap = snap(&new_src);
        let result = s.parse_edit(&edit, new_snap).unwrap();

        assert_eq!(result.tree.text(), new_src);
    }

    #[test]
    fn parse_edit_matches_full_parse() {
        let mut s1 = session();
        let mut s2 = session();
        let src = "let x = 1;\nlet y = 2;";
        s1.parse(snap(src)).unwrap();
        s2.parse(snap(src)).unwrap();

        // Replace '1' (position 8) with '99'.
        let edit = TextEdit::replace(TextRange::new(TextSize::from(8), TextSize::from(9)), "99");
        let new_src = edit.apply(src);

        let incremental = s1.parse_edit(&edit, snap(&new_src)).unwrap();
        let full = s2.parse(snap(&new_src)).unwrap();

        // The complete incremental result must match a full parse.
        assert_eq!(incremental.tree.text(), full.tree.text());
        assert_eq!(incremental.tree.green(), full.tree.green());
        assert_eq!(incremental.errors, full.errors);
        assert_eq!(incremental.features, full.features);
    }

    #[test]
    fn parse_edit_unchanged_prefix_children_reuse_arc() {
        let mut s = session();
        // Two top-level statements; we edit only the integer in the second.
        let src = "let x = 1;\nlet y = 2;";
        s.parse(snap(src)).unwrap();

        // Replace '2' (position 19) with '99' — stays a valid integer literal.
        let edit = TextEdit::replace(TextRange::new(TextSize::from(19), TextSize::from(20)), "99");
        let new_src = edit.apply(src);

        let old_root = s.result.as_ref().unwrap().tree.root();
        let old_first_child_ptr = old_root.children().next().map(|n| {
            let g = n.green();
            Arc::as_ptr(g) as usize
        });

        s.parse_edit(&edit, snap(&new_src)).unwrap();

        let new_root = s.result.as_ref().unwrap().tree.root();
        let new_first_child_ptr = new_root.children().next().map(|n| {
            let g = n.green();
            Arc::as_ptr(g) as usize
        });

        assert_eq!(
            old_first_child_ptr, new_first_child_ptr,
            "unchanged first child should reuse the same Arc<GreenNode>"
        );
    }

    #[test]
    fn parse_edit_without_prior_result_does_full_parse() {
        let mut s = session();
        let src = "let x = 1;";
        let edit = TextEdit::insert(TextSize::from(0), src);
        let result = s.parse_edit(&edit, snap(src)).unwrap();
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn parse_edit_unknown_language_returns_none() {
        let mut s = SyntaxSession::new(Arc::new(SyntaxEngine::new()), LanguageId("no-such-lang"));
        let edit = TextEdit::insert(TextSize::from(0), "x");
        assert!(s.parse_edit(&edit, snap("x")).is_none());
    }
}
