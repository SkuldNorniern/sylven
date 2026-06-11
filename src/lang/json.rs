//! JSON(C) language plugin for Sylven — Stage 1 (lexer-backed).
//!
//! Uses [`lex_json`] to produce a flat lossless [`SyntaxTree`] and derives
//! [`SyntaxFeatures`] (highlights, folds, bracket pairs) directly from the
//! token stream, without a grammar-driven parse yet.

use sylven_lex::SyntaxKind;
use sylven_lex::json::{JsonKind, lex_json};
use sylven_parse::{ParseEvent, TokenId, build_tree};
use sylven_text::{LineIndex, TextRange, TextSize, TextSnapshot};

use crate::{
    LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures,
    result::{Highlight, HighlightKind},
};

/// Node kind for the flat root FILE node.
const FILE: SyntaxKind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

/// The JSON [`LanguagePlugin`].
pub struct JsonLanguage;

impl LanguagePlugin for JsonLanguage {
    fn id(&self) -> LanguageId {
        LanguageId("json")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let text = snapshot.text();
        let stream = lex_json(text);
        let tokens = stream.as_slice();

        // Flat tree: FILE node wrapping every token as a direct leaf.
        // A grammar-driven parser will replace this in Stage 2.
        let mut events = Vec::with_capacity(tokens.len() + 2);
        events.push(ParseEvent::StartNode(FILE));
        for (i, _) in tokens.iter().enumerate() {
            events.push(ParseEvent::Token(TokenId(i as u32)));
        }
        events.push(ParseEvent::FinishNode);

        let (tree, errors) = build_tree(tokens, text, events);
        let features = derive_features(tokens, text);
        ParseResult {
            tree,
            errors,
            features,
        }
    }
}

// ---------------------------------------------------------------------------
// Feature derivation from the flat token stream
// ---------------------------------------------------------------------------

fn derive_features(tokens: &[sylven_lex::Token], source: &str) -> SyntaxFeatures {
    SyntaxFeatures {
        highlights: derive_highlights(tokens),
        folds: derive_folds(tokens, source),
        symbols: Vec::new(),
        injections: Vec::new(),
        brackets: derive_brackets(tokens, source),
    }
}

// ---------------------------------------------------------------------------
// Highlights — map JsonKind → HighlightKind
// ---------------------------------------------------------------------------

fn json_kind_to_highlight(k: SyntaxKind) -> Option<HighlightKind> {
    if k == JsonKind::Key.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == JsonKind::String.to_syntax() {
        return Some(HighlightKind::String);
    }
    if k == JsonKind::NumberLit.to_syntax() || k == JsonKind::BoolLit.to_syntax() {
        return Some(HighlightKind::Number);
    }
    if k == JsonKind::NullLit.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == JsonKind::Comment.to_syntax() {
        return Some(HighlightKind::Comment);
    }
    if k == JsonKind::Punctuation.to_syntax() {
        return Some(HighlightKind::Punctuation);
    }
    if k == JsonKind::Ident.to_syntax() {
        return Some(HighlightKind::Variable);
    }
    None // whitespace, EOF, ERROR
}

fn derive_highlights(tokens: &[sylven_lex::Token]) -> Vec<Highlight> {
    tokens
        .iter()
        .filter_map(|tok| {
            let kind = json_kind_to_highlight(tok.kind)?;
            Some(Highlight {
                range: tok.range,
                kind,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Folds — multi-line `{ … }` objects and `[ … ]` arrays
// ---------------------------------------------------------------------------

fn derive_folds(tokens: &[sylven_lex::Token], source: &str) -> Vec<TextRange> {
    let line_index = LineIndex::new(source);
    let mut folds = Vec::new();

    let mut curly: Vec<TextSize> = Vec::new();
    let mut square: Vec<TextSize> = Vec::new();
    for tok in tokens {
        if tok.kind != JsonKind::Punctuation.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        let e = tok.range.end().to_usize();
        if e > source.len() || s >= e {
            continue;
        }
        match source.as_bytes()[s] {
            b'{' => curly.push(tok.range.start()),
            b'[' => square.push(tok.range.start()),
            b'}' => {
                if let Some(open_start) = curly.pop() {
                    push_fold_if_multiline(&line_index, &mut folds, open_start, tok.range.end());
                }
            }
            b']' => {
                if let Some(open_start) = square.pop() {
                    push_fold_if_multiline(&line_index, &mut folds, open_start, tok.range.end());
                }
            }
            _ => {}
        }
    }

    folds
}

fn push_fold_if_multiline(
    line_index: &LineIndex,
    folds: &mut Vec<TextRange>,
    start: TextSize,
    end: TextSize,
) {
    let start_line = line_index.line_col(start).line;
    let end_line = line_index.line_col(end).line;
    if end_line > start_line {
        folds.push(TextRange::new(start, end));
    }
}

// ---------------------------------------------------------------------------
// Brackets — matching `{ }` and `[ ]` pairs
// ---------------------------------------------------------------------------

fn derive_brackets(tokens: &[sylven_lex::Token], source: &str) -> Vec<(TextRange, TextRange)> {
    let mut curly: Vec<TextRange> = Vec::new();
    let mut square: Vec<TextRange> = Vec::new();
    let mut pairs = Vec::new();

    for tok in tokens {
        if tok.kind != JsonKind::Punctuation.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        if s >= source.len() {
            continue;
        }
        match source.as_bytes()[s] {
            b'{' => curly.push(tok.range),
            b'[' => square.push(tok.range),
            b'}' => {
                if let Some(open) = curly.pop() {
                    pairs.push((open, tok.range));
                }
            }
            b']' => {
                if let Some(open) = square.pop() {
                    pairs.push((open, tok.range));
                }
            }
            _ => {}
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::{DocumentId, RevisionId};

    fn parse(source: &str) -> ParseResult {
        let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
        JsonLanguage.parse(&snap)
    }

    fn features(source: &str) -> SyntaxFeatures {
        parse(source).features
    }

    #[test]
    fn id_is_json() {
        assert_eq!(JsonLanguage.id(), LanguageId("json"));
    }

    #[test]
    fn lossless_tree() {
        let src = "{\n  \"a\": 1\n}\n";
        let r = parse(src);
        assert_eq!(r.tree.text(), src);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn highlights_contain_key_and_string_and_number() {
        let f = features(r#"{"a": "b", "n": 1}"#);
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::String));
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::Number));
    }

    #[test]
    fn fold_for_multiline_object() {
        let src = "{\n  \"a\": 1\n}\n";
        let f = features(src);
        assert!(!f.folds.is_empty(), "expected fold for multi-line object");
    }

    #[test]
    fn fold_for_multiline_array() {
        let src = "[\n  1,\n  2\n]\n";
        let f = features(src);
        assert!(!f.folds.is_empty(), "expected fold for multi-line array");
    }

    #[test]
    fn no_fold_for_single_line_object() {
        let f = features(r#"{"a": 1}"#);
        assert!(f.folds.is_empty());
    }

    #[test]
    fn brackets_matched_for_nested_object_and_array() {
        let f = features(r#"{"a": [1, 2]}"#);
        assert_eq!(f.brackets.len(), 2);
    }

    #[test]
    fn no_symbols() {
        let f = features(r#"{"a": 1}"#);
        assert!(f.symbols.is_empty());
    }
}
