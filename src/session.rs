use std::sync::Arc;

use sylven_parse::{ParseError, find_reparse_root, splice_green, splice_region};
use sylven_text::{TextEdit, TextRange, TextSize, TextSnapshot};
use sylven_tree::{GreenElement, SyntaxElement, SyntaxNode, SyntaxTree};

use crate::{
    Highlight, Injection, LanguageId, ParseResult, SymbolInfo, SyntaxEngine, SyntaxFeatures,
};

/// A document's connection to the syntax engine: pairs a [`LanguageId`] with
/// the most recently parsed [`TextSnapshot`] and [`ParseResult`].
///
/// Two parse entry points are available:
///
/// - [`parse`](Self::parse) — always does a full re-parse from scratch.
/// - [`parse_edit`](Self::parse_edit) — incremental path: reparses only the
///   dirty top-level child's text (a *regional* reparse) when a
///   boundary-stability probe shows the edit can't change how neighboring
///   children are lexed, and falls back to a full reparse-and-splice
///   otherwise.
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

    /// Apply `edit` to the current source and reparse.
    ///
    /// Tries a *regional* reparse first: only the dirty top-level child (the
    /// smallest old child fully containing `edit.delete`) is re-lexed and
    /// re-parsed from its resized text, then grafted back into the old tree.
    /// This is skipped, and a full reparse-and-splice is used instead, when:
    /// - No previous result exists (first parse).
    /// - The edit spans more than one top-level child.
    /// - The dirty child collapses to nothing.
    /// - A boundary-stability probe shows the edit could change how the
    ///   following sibling is lexed (e.g. the edit opens an unterminated
    ///   block comment or string that should swallow later text).
    /// - The regional result would not losslessly reproduce `new_snapshot`.
    ///
    /// Returns `None` if no plugin is registered for this session's language.
    pub fn parse_edit(
        &mut self,
        edit: &TextEdit,
        new_snapshot: TextSnapshot,
    ) -> Option<&ParseResult> {
        if let Some(spliced) = self.try_regional_reparse(edit, &new_snapshot) {
            self.snapshot = Some(new_snapshot);
            self.result = Some(spliced);
            return self.result.as_ref();
        }

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

    /// Try a regional reparse of just the dirty top-level child.
    ///
    /// On success, returns a [`ParseResult`] whose tree losslessly reproduces
    /// `new_snapshot` and whose features are the old features with the dirty
    /// child's contribution replaced by a translated reparse of its new text.
    /// Returns `None` if any precondition or the boundary-stability probe
    /// fails, so the caller falls back to a full reparse.
    fn try_regional_reparse(
        &self,
        edit: &TextEdit,
        new_snapshot: &TextSnapshot,
    ) -> Option<ParseResult> {
        let old_result = self.result.as_ref()?;
        if !self.engine.registry().get(self.language)?.regional() {
            return None;
        }
        let old_root = old_result.tree.root();

        let dirty_idx = find_reparse_root(&old_root, edit.delete)?;
        let old_child = old_root.children_with_tokens().nth(dirty_idx)?;
        let old_child_range = old_child.text_range();

        // an edit right at the start of a child can join it to what comes
        // before (text typed after a paragraph, say); only the trailing side
        // is probed below, so leave those to a full parse
        if edit.delete.start() <= old_child_range.start() || old_child_range.is_empty() {
            return None;
        }

        let delta: isize = edit.insert.len() as isize
            - (edit.delete.end() - edit.delete.start()).to_usize() as isize;

        let new_text = new_snapshot.text();
        let new_start = old_child_range.start();
        let new_end = shift_size(old_child_range.end(), delta);
        // an edit that doesn't fit the old snapshot can't be trusted here
        if new_end <= new_start
            || new_end.to_usize() > new_text.len()
            || !new_text.is_char_boundary(new_start.to_usize())
            || !new_text.is_char_boundary(new_end.to_usize())
        {
            return None;
        }
        let sub_source = &new_text[new_start.to_usize()..new_end.to_usize()];

        // Boundary-stability probe: reparse the dirty text together with
        // the next child. The last token of `sub_source` must not run on
        // into it, and the next child must come out as the same tokens it
        // was: some lexers carry state across (a markdown fence only starts
        // a line), so text typed here can change how what follows lexes.
        let next = old_root
            .children_with_tokens()
            .nth(dirty_idx + 1)
            .filter(|next| !next.text_range().is_empty());
        if let Some(next) = next {
            let next_len = next.text_range().len().to_usize();
            let probe_end = new_end.to_usize() + next_len;
            if probe_end > new_text.len() || !new_text.is_char_boundary(probe_end) {
                return None;
            }
            let probe_source = &new_text[new_start.to_usize()..probe_end];
            let probe_snapshot = TextSnapshot::new(
                new_snapshot.document_id(),
                new_snapshot.revision(),
                probe_source,
            );
            let probe_result = self.engine.parse(self.language, &probe_snapshot)?;
            let boundary = TextSize::from(sub_source.len() as u32);
            let probe_root = probe_result.tree.root();
            if token_spans_boundary(&probe_root, boundary) {
                return None;
            }
            // a construct begun in the region that now runs on past it (a new
            // heading folding the sections after it) needs the whole file
            let past = |r: TextRange| r.start() < boundary && r.end() > boundary;
            let features = &probe_result.features;
            if features.folds.iter().any(|r| past(*r))
                || features.symbols.iter().any(|s| past(s.decl_range))
            {
                return None;
            }
            let mut after = Vec::new();
            collect_tokens(&SyntaxElement::Node(probe_root), Some(boundary), &mut after);
            let mut before = Vec::new();
            collect_tokens(&next, None, &mut before);
            if after != before {
                return None;
            }
        }

        let sub_snapshot = TextSnapshot::new(
            new_snapshot.document_id(),
            new_snapshot.revision(),
            sub_source,
        );
        let sub_result = self.engine.parse(self.language, &sub_snapshot)?;

        // Some lexers are stateful across tokens (e.g. Markdown's fenced
        // code body vs. plain text). Reparsing `sub_source` in isolation
        // loses that state, which the boundary probe and lossless check
        // can't detect on their own — guard against it by requiring the
        // reparsed region to start with the same kind as the child it
        // replaces.
        let sub_children = sub_result.tree.green().children();
        if sub_children.first().map(|c| c.kind()) != Some(old_child.kind()) {
            return None;
        }

        // an error where the sub parse ran out of text says what the end of
        // the region looked like, not what the file has after it
        let sub_len = TextSize::from(sub_source.len() as u32);
        if sub_result.errors.iter().any(|e| e.range.end() >= sub_len) {
            return None;
        }
        // a feature from outside reaching into the region (a heading fold
        // over a whole section, say) only a full parse can redo
        if reaches_into(&old_result.features, old_child_range) {
            return None;
        }

        // brackets pair across the whole file; swapping a region that has
        // a bracket pairing with something outside it (before or after the
        // edit) would leave pairs and folds out of date
        let old_green = old_result.tree.green().children().get(dirty_idx)?;
        if !brackets_self_contained(std::slice::from_ref(old_green))
            || !brackets_self_contained(sub_children)
        {
            return None;
        }

        // the sub parse ends with its own empty EOF token; the file has one
        let mut replacement = sub_children;
        if let Some(GreenElement::Token(last)) = replacement.last()
            && last.text().is_empty()
        {
            replacement = &replacement[..replacement.len() - 1];
        }
        let spliced_green = splice_region(old_result.tree.green(), dirty_idx, replacement);
        let spliced_tree = SyntaxTree::new(spliced_green);
        if spliced_tree.text() != new_text {
            return None;
        }

        let features = merge_regional_features(
            &old_result.features,
            &sub_result.features,
            old_child_range,
            delta,
            new_start,
        );
        let errors = merge_items(
            &old_result.errors,
            &sub_result.errors,
            old_child_range,
            delta,
            new_start,
        );

        Some(ParseResult {
            tree: spliced_tree,
            errors,
            features,
        })
    }
}

/// `true` when a fold, highlight, symbol or bracket starts outside `child`
/// and ends inside it, past it, or right where it starts (text added there
/// can carry on the construct before it, like a markdown paragraph).
fn reaches_into(features: &SyntaxFeatures, child: TextRange) -> bool {
    let crosses = |r: TextRange| {
        r.start() < child.start() && r.end() >= child.start()
            || r.start() < child.end() && r.end() > child.end() && r.start() >= child.start()
    };
    features.folds.iter().any(|r| crosses(*r))
        || features.highlights.iter().any(|h| crosses(h.range))
        || features.symbols.iter().any(|s| crosses(s.decl_range))
        || features
            .brackets
            .iter()
            .any(|(a, b)| crosses(TextRange::new(a.start(), b.end())))
}

/// `true` when every bracket token among `elements` pairs with one of its
/// kind inside them, so none of them pairs with anything outside.
fn brackets_self_contained(elements: &[GreenElement]) -> bool {
    fn walk(elements: &[GreenElement], depth: &mut [usize; 3]) -> bool {
        for element in elements {
            match element {
                GreenElement::Node(node) => {
                    if !walk(node.children(), depth) {
                        return false;
                    }
                }
                GreenElement::Token(token) => {
                    let (slot, open) = match token.text() {
                        "(" => (0, true),
                        ")" => (0, false),
                        "[" => (1, true),
                        "]" => (1, false),
                        "{" => (2, true),
                        "}" => (2, false),
                        _ => continue,
                    };
                    if open {
                        depth[slot] += 1;
                    } else if depth[slot] == 0 {
                        return false;
                    } else {
                        depth[slot] -= 1;
                    }
                }
            }
        }
        true
    }
    let mut depth = [0; 3];
    walk(elements, &mut depth) && depth == [0; 3]
}

/// kind and text of the non-empty tokens under `element`, only those that
/// start at or after `from` when given.
fn collect_tokens(
    element: &SyntaxElement,
    from: Option<TextSize>,
    out: &mut Vec<(sylven_lex::SyntaxKind, String)>,
) {
    match element {
        SyntaxElement::Node(node) => {
            for child in node.children_with_tokens() {
                collect_tokens(&child, from, out);
            }
        }
        SyntaxElement::Token(token) => {
            let range = token.text_range();
            if !range.is_empty() && from.is_none_or(|f| range.start() >= f) {
                out.push((token.kind(), token.text().to_owned()));
            }
        }
    }
}

/// `true` if any token in `node`'s subtree starts before `boundary` and ends
/// after it — i.e. `boundary` falls strictly inside a token rather than on a
/// token edge.
fn token_spans_boundary(node: &SyntaxNode, boundary: TextSize) -> bool {
    node.children_with_tokens().any(|child| match child {
        SyntaxElement::Node(n) => token_spans_boundary(&n, boundary),
        SyntaxElement::Token(t) => {
            let r = t.text_range();
            r.start() < boundary && r.end() > boundary
        }
    })
}

// ── Green-node splicing (full-parse fallback) ─────────────────────────────────

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

    ParseResult {
        tree: spliced_tree,
        errors: new_result.errors.clone(),
        features: new_result.features.clone(),
    }
}

// ── Feature/error merging for regional reparse ────────────────────────────────

/// Shift a byte offset by a signed delta, clamping at zero.
fn shift_size(size: TextSize, delta: isize) -> TextSize {
    let shifted = size.to_u32() as i64 + delta as i64;
    TextSize::from(shifted.max(0) as u32)
}

/// Map an old endpoint to its position in the new document: endpoints at or
/// after the dirty child's old end shift by `delta`; endpoints at or before
/// its old start (and any endpoint strictly inside it, which can't occur
/// since feature ranges align to token edges) are unaffected.
fn relocate_point(p: TextSize, old_child_range: TextRange, delta: isize) -> TextSize {
    if p >= old_child_range.end() {
        shift_size(p, delta)
    } else {
        p
    }
}

fn relocate_range(r: TextRange, old_child_range: TextRange, delta: isize) -> TextRange {
    TextRange::new(
        relocate_point(r.start(), old_child_range, delta),
        relocate_point(r.end(), old_child_range, delta),
    )
}

/// A feature/error entry whose byte ranges can be relocated (old entries
/// outside the dirty child) or translated (new entries from the dirty
/// child's regional reparse, relative to its own text).
trait Relocate: Sized {
    /// The range used to decide whether this entry lies entirely inside the
    /// dirty child (and is therefore dropped in favor of the regional
    /// reparse's own entries).
    fn cover_range(&self) -> TextRange;
    /// Adjust every range field for an entry that survives outside the dirty
    /// child.
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> Self;
    /// Shift every range field by `offset` for an entry produced by the
    /// regional reparse (ranges relative to the dirty child's own text).
    fn translate(&self, offset: TextSize) -> Self;
}

impl Relocate for TextRange {
    fn cover_range(&self) -> TextRange {
        *self
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> TextRange {
        relocate_range(*self, old_child_range, delta)
    }
    fn translate(&self, offset: TextSize) -> TextRange {
        self.shift(offset)
    }
}

impl Relocate for Highlight {
    fn cover_range(&self) -> TextRange {
        self.range
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> Highlight {
        Highlight {
            range: relocate_range(self.range, old_child_range, delta),
            kind: self.kind,
        }
    }
    fn translate(&self, offset: TextSize) -> Highlight {
        Highlight {
            range: self.range.shift(offset),
            kind: self.kind,
        }
    }
}

impl Relocate for Injection {
    fn cover_range(&self) -> TextRange {
        self.range
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> Injection {
        Injection {
            language: self.language.clone(),
            range: relocate_range(self.range, old_child_range, delta),
        }
    }
    fn translate(&self, offset: TextSize) -> Injection {
        Injection {
            language: self.language.clone(),
            range: self.range.shift(offset),
        }
    }
}

impl Relocate for SymbolInfo {
    fn cover_range(&self) -> TextRange {
        self.decl_range
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> SymbolInfo {
        SymbolInfo {
            name: self.name.clone(),
            name_range: relocate_range(self.name_range, old_child_range, delta),
            decl_range: relocate_range(self.decl_range, old_child_range, delta),
            kind: self.kind,
        }
    }
    fn translate(&self, offset: TextSize) -> SymbolInfo {
        SymbolInfo {
            name: self.name.clone(),
            name_range: self.name_range.shift(offset),
            decl_range: self.decl_range.shift(offset),
            kind: self.kind,
        }
    }
}

impl Relocate for (TextRange, TextRange) {
    fn cover_range(&self) -> TextRange {
        self.0.cover(self.1)
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> (TextRange, TextRange) {
        (
            relocate_range(self.0, old_child_range, delta),
            relocate_range(self.1, old_child_range, delta),
        )
    }
    fn translate(&self, offset: TextSize) -> (TextRange, TextRange) {
        (self.0.shift(offset), self.1.shift(offset))
    }
}

impl Relocate for ParseError {
    fn cover_range(&self) -> TextRange {
        self.range
    }
    fn relocate(&self, old_child_range: TextRange, delta: isize) -> ParseError {
        ParseError::new(
            self.message.clone(),
            relocate_range(self.range, old_child_range, delta),
        )
    }
    fn translate(&self, offset: TextSize) -> ParseError {
        ParseError::new(self.message.clone(), self.range.shift(offset))
    }
}

/// Merge one feature/error vec for the regional-reparse path.
///
/// Old entries whose `cover_range` lies entirely inside the dirty child are
/// dropped (replaced by `sub`'s entries); the rest are relocated. Entries
/// starting at or before the dirty child's old start come first (preserving
/// source order with the dirty child's own entries), then `sub`'s entries
/// (translated into the parent's coordinates), then entries starting after
/// the dirty child.
fn merge_items<T: Relocate + Clone>(
    old: &[T],
    sub: &[T],
    old_child_range: TextRange,
    delta: isize,
    new_start: TextSize,
) -> Vec<T> {
    let mut before = Vec::new();
    let mut after = Vec::new();
    for item in old {
        let r = item.cover_range();
        let inside = r.start() >= old_child_range.start() && r.end() <= old_child_range.end();
        if inside {
            continue;
        }
        if r.start() <= old_child_range.start() {
            before.push(item.relocate(old_child_range, delta));
        } else {
            after.push(item.relocate(old_child_range, delta));
        }
    }
    before.extend(sub.iter().map(|item| item.translate(new_start)));
    before.extend(after);
    before
}

fn merge_regional_features(
    old: &SyntaxFeatures,
    sub: &SyntaxFeatures,
    old_child_range: TextRange,
    delta: isize,
    new_start: TextSize,
) -> SyntaxFeatures {
    SyntaxFeatures {
        highlights: merge_items(
            &old.highlights,
            &sub.highlights,
            old_child_range,
            delta,
            new_start,
        ),
        folds: merge_items(&old.folds, &sub.folds, old_child_range, delta, new_start),
        symbols: merge_items(
            &old.symbols,
            &sub.symbols,
            old_child_range,
            delta,
            new_start,
        ),
        injections: merge_items(
            &old.injections,
            &sub.injections,
            old_child_range,
            delta,
            new_start,
        ),
        brackets: merge_items(
            &old.brackets,
            &sub.brackets,
            old_child_range,
            delta,
            new_start,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::{DocumentId, RevisionId, TextSize};

    use crate::lang::markdown::MarkdownLanguage;
    use crate::lang::mini_oxygen::MiniOxygen;
    use crate::registry::LanguageRegistry;

    fn engine_with_mini_oxygen() -> Arc<SyntaxEngine> {
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MiniOxygen));
        Arc::new(SyntaxEngine::with_registry(registry))
    }

    fn engine_with_markdown() -> Arc<SyntaxEngine> {
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MarkdownLanguage));
        Arc::new(SyntaxEngine::with_registry(registry))
    }

    fn snap(text: &str) -> TextSnapshot {
        TextSnapshot::new(DocumentId(0), RevisionId(0), text)
    }

    fn session() -> SyntaxSession {
        SyntaxSession::new(engine_with_mini_oxygen(), LanguageId("mini-oxygen"))
    }

    fn markdown_session() -> SyntaxSession {
        SyntaxSession::new(engine_with_markdown(), LanguageId("markdown"))
    }

    /// every one char insert and delete in `src` gives what a full parse
    /// of the new text gives: tree, errors and features.
    fn every_small_edit_matches_full(make: fn() -> SyntaxSession, src: &str, insert: &str) {
        let mut cuts: Vec<usize> = (0..=src.len())
            .filter(|&i| src.is_char_boundary(i))
            .collect();
        cuts.dedup();
        let mut edits = Vec::new();
        for &at in &cuts {
            edits.push(TextEdit::insert(TextSize::from(at as u32), insert));
        }
        for pair in cuts.windows(2) {
            let range = TextRange::new(
                TextSize::from(pair[0] as u32),
                TextSize::from(pair[1] as u32),
            );
            edits.push(TextEdit::replace(range, ""));
        }
        for edit in edits {
            let mut incremental = make();
            incremental.parse(snap(src)).unwrap();
            let new_src = edit.apply(src);
            let got = incremental.parse_edit(&edit, snap(&new_src)).unwrap();
            let mut full = make();
            let want = full.parse(snap(&new_src)).unwrap();
            let at = format!("{src:?} {edit:?}");
            assert_eq!(got.tree.green(), want.tree.green(), "tree at {at}");
            assert_eq!(got.errors, want.errors, "errors at {at}");
            assert_eq!(got.features, want.features, "features at {at}");
        }
    }

    #[test]
    fn small_edits_match_full_parse_with_brackets_across_statements() {
        for src in [
            "let x = 1;
let y = 2);",
            "let a = (1;
let b = 2;
let c = 3);",
            "f(1);
g(2);",
        ] {
            for insert in [
                "(", ")", "x", "
",
            ] {
                every_small_edit_matches_full(session, src, insert);
            }
        }
    }

    type MakeSession = fn() -> SyntaxSession;

    fn rust_session() -> SyntaxSession {
        SyntaxSession::new(Arc::new(SyntaxEngine::new()), LanguageId("rust"))
    }

    fn toml_session() -> SyntaxSession {
        SyntaxSession::new(Arc::new(SyntaxEngine::new()), LanguageId("toml"))
    }

    fn json_session() -> SyntaxSession {
        SyntaxSession::new(Arc::new(SyntaxEngine::new()), LanguageId("json"))
    }

    fn yaml_session() -> SyntaxSession {
        SyntaxSession::new(Arc::new(SyntaxEngine::new()), LanguageId("yaml"))
    }

    #[test]
    fn small_edits_match_full_parse_in_rust_toml_json_yaml() {
        let cases: [(MakeSession, &str); 4] = [
            (
                rust_session,
                "fn a() {
    let x = \"가\";
}
// c
fn b() { 1 }
",
            ),
            (
                toml_session,
                "[a]
x = 1
y = \"s\"
[b]
z = [1, 2]
",
            ),
            (
                json_session,
                "{\"a\": [1, {\"b\": null}],
 \"c\": \"d\"}",
            ),
            (
                yaml_session,
                "a: 1
b:
  - x
  - {y: 2}
",
            ),
        ];
        for (make, src) in cases {
            for insert in [
                "{", "}", "\"", "x", "
", "/",
            ] {
                every_small_edit_matches_full(make, src, insert);
            }
        }
    }

    #[test]
    fn small_edits_match_full_parse_at_the_end_of_markdown() {
        for src in [
            "# t

para
",
            "a

```rust
fn f() {}
```
b",
        ] {
            for insert in [
                "x", "`", "
", "#",
            ] {
                every_small_edit_matches_full(markdown_session, src, insert);
            }
        }
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

    /// An edit that stays inside one token and doesn't reach the document end
    /// must take the regional path: the unchanged second statement's green
    /// node is reused by `Arc` pointer, not just structurally equal.
    #[test]
    fn parse_edit_regional_reuses_unchanged_suffix_arc() {
        let mut s = session();
        let src = "let x = 1;\nlet y = 2;";
        s.parse(snap(src)).unwrap();

        // Replace '1' (position 8) with '99' — first statement, not at EOF.
        let edit = TextEdit::replace(TextRange::new(TextSize::from(8), TextSize::from(9)), "99");
        let new_src = edit.apply(src);

        let old_root = s.result.as_ref().unwrap().tree.root();
        let old_last_child_ptr = old_root.children().last().map(|n| {
            let g = n.green();
            Arc::as_ptr(g) as usize
        });

        let result = s.parse_edit(&edit, snap(&new_src)).unwrap();
        assert_eq!(result.tree.text(), new_src);

        let new_root = s.result.as_ref().unwrap().tree.root();
        let new_last_child_ptr = new_root.children().last().map(|n| {
            let g = n.green();
            Arc::as_ptr(g) as usize
        });

        assert_eq!(
            old_last_child_ptr, new_last_child_ptr,
            "unchanged trailing statement should reuse the same Arc<GreenNode> via regional reparse"
        );
    }

    /// An edit that opens an unterminated block comment must fall back to a
    /// full parse: in isolation the dirty child's last token would extend
    /// past its old boundary, swallowing the following statement.
    #[test]
    fn parse_edit_unterminated_comment_falls_back_to_full_parse() {
        let mut s1 = session();
        let mut s2 = session();
        let src = "let x = 1;\nlet y = 2;";
        s1.parse(snap(src)).unwrap();
        s2.parse(snap(src)).unwrap();

        // Insert "/*" right after "let x = 1;" — opens a block comment that,
        // in the full document, swallows the rest of the source.
        let edit = TextEdit::insert(TextSize::from(10), "/*");
        let new_src = edit.apply(src);

        let incremental = s1.parse_edit(&edit, snap(&new_src)).unwrap();
        let full = s2.parse(snap(&new_src)).unwrap();

        assert_eq!(incremental.tree.text(), full.tree.text());
        assert_eq!(incremental.tree.green(), full.tree.green());
        assert_eq!(incremental.errors, full.errors);
        assert_eq!(incremental.features, full.features);
    }

    /// A regional edit inside a fenced code block's body must keep the
    /// Markdown heading fold (which spans both the heading and the fence)
    /// and re-derive the injected Rust highlights for the edited line.
    #[test]
    fn parse_edit_markdown_fence_body_matches_full_parse() {
        let mut s1 = markdown_session();
        let mut s2 = markdown_session();
        let src = "# Title\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n";
        s1.parse(snap(src)).unwrap();
        s2.parse(snap(src)).unwrap();

        // Replace '1' with '99' inside the fenced code block.
        let pos = src.find("1;").unwrap() as u32;
        let edit = TextEdit::replace(
            TextRange::new(TextSize::from(pos), TextSize::from(pos + 1)),
            "99",
        );
        let new_src = edit.apply(src);

        let incremental = s1.parse_edit(&edit, snap(&new_src)).unwrap();
        let full = s2.parse(snap(&new_src)).unwrap();

        assert_eq!(incremental.tree.text(), full.tree.text());
        assert_eq!(incremental.tree.green(), full.tree.green());
        assert_eq!(incremental.errors, full.errors);
        assert_eq!(incremental.features, full.features);
    }
}
