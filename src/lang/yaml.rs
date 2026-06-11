//! YAML language plugin for Sylven — Stage 1 (lexer-backed).
//!
//! Uses [`lex_yaml`] to produce a flat lossless [`SyntaxTree`] and derives
//! [`SyntaxFeatures`] (highlights, folds, symbols, bracket pairs) directly
//! from the token stream, without a grammar-driven parse yet.

use sylven_lex::SyntaxKind;
use sylven_lex::yaml::{YamlKind, lex_yaml};
use sylven_parse::{ParseEvent, TokenId, build_tree};
use sylven_text::{LineIndex, TextRange, TextSize, TextSnapshot};

use crate::{
    LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures,
    result::{Highlight, HighlightKind, SymbolInfo, SymbolKind},
};

/// Node kind for the flat root FILE node.
const FILE: SyntaxKind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

/// The YAML [`LanguagePlugin`].
pub struct YamlLanguage;

impl LanguagePlugin for YamlLanguage {
    fn id(&self) -> LanguageId {
        LanguageId("yaml")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let text = snapshot.text();
        let stream = lex_yaml(text);
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
        symbols: derive_symbols(tokens, source),
        injections: Vec::new(),
        brackets: derive_brackets(tokens, source),
    }
}

// ---------------------------------------------------------------------------
// Highlights — map YamlKind → HighlightKind
// ---------------------------------------------------------------------------

fn yaml_kind_to_highlight(k: SyntaxKind) -> Option<HighlightKind> {
    if k == YamlKind::Key.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == YamlKind::String.to_syntax() {
        return Some(HighlightKind::String);
    }
    if k == YamlKind::NumberLit.to_syntax() || k == YamlKind::BoolLit.to_syntax() {
        return Some(HighlightKind::Number);
    }
    if k == YamlKind::NullLit.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == YamlKind::Anchor.to_syntax() {
        return Some(HighlightKind::Attribute);
    }
    if k == YamlKind::Alias.to_syntax() {
        return Some(HighlightKind::Variable);
    }
    if k == YamlKind::Tag.to_syntax() {
        return Some(HighlightKind::Type);
    }
    if k == YamlKind::Comment.to_syntax() {
        return Some(HighlightKind::Comment);
    }
    if k == YamlKind::DocumentMarker.to_syntax() || k == YamlKind::Operator.to_syntax() {
        return Some(HighlightKind::Operator);
    }
    if k == YamlKind::Punctuation.to_syntax() {
        return Some(HighlightKind::Punctuation);
    }
    None // PlainScalar, whitespace, EOF, ERROR
}

fn derive_highlights(tokens: &[sylven_lex::Token]) -> Vec<Highlight> {
    tokens
        .iter()
        .filter_map(|tok| {
            let kind = yaml_kind_to_highlight(tok.kind)?;
            Some(Highlight {
                range: tok.range,
                kind,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Folds — multi-line flow collections, and block-mapping bodies
// ---------------------------------------------------------------------------

fn derive_folds(tokens: &[sylven_lex::Token], source: &str) -> Vec<TextRange> {
    let line_index = LineIndex::new(source);
    let mut folds = Vec::new();

    // Flow-collection folds: `{ … }` and `[ … ]` spanning at least one line
    // boundary.
    let mut curly: Vec<TextSize> = Vec::new();
    let mut square: Vec<TextSize> = Vec::new();
    for tok in tokens {
        if tok.kind != YamlKind::Punctuation.to_syntax() {
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

    // Block-mapping folds: a `key:` with no inline value folds the following
    // more-indented lines (its body), up to the first line that dedents back
    // to (or past) the key's own indentation.
    let lines = line_infos(source);
    for (i, tok) in tokens.iter().enumerate() {
        if tok.kind != YamlKind::Key.to_syntax() {
            continue;
        }
        let mut j = i + 1;
        while tokens
            .get(j)
            .is_some_and(|t| t.kind == SyntaxKind::WHITESPACE)
        {
            j += 1;
        }
        let Some(colon) = tokens.get(j) else { continue };
        if colon.kind != YamlKind::Operator.to_syntax() {
            continue;
        }
        let cs = colon.range.start().to_usize();
        if cs >= source.len() || source.as_bytes()[cs] != b':' {
            continue;
        }
        if line_has_inline_value_after(tokens, j + 1, source) {
            continue;
        }

        let key_line = line_index.line_col(tok.range.start()).line as usize;
        let Some(key_indent) = lines.get(key_line).and_then(|&(_, indent)| indent) else {
            continue;
        };

        let mut last_body_line = key_line;
        for (idx, &(_, indent)) in lines.iter().enumerate().skip(key_line + 1) {
            match indent {
                Some(ind) if ind > key_indent => last_body_line = idx,
                Some(_) => break,
                None => continue, // blank/comment-only line: tentative
            }
        }
        if last_body_line == key_line {
            continue;
        }

        let line_start = lines[last_body_line].0;
        let line_end = lines
            .get(last_body_line + 1)
            .map(|&(s, _)| s - 1) // exclude the trailing '\n'
            .unwrap_or(source.len())
            .min(source.len());
        let trimmed_len = source[line_start..line_end].trim_end().len();
        let body_end = TextSize::from((line_start + trimmed_len) as u32);
        folds.push(TextRange::new(tok.range.start(), body_end));
    }

    folds
}

/// Per-line `(start_byte, indent)`. `indent` is `None` for blank or
/// comment-only lines (their indentation doesn't end a block).
fn line_infos(source: &str) -> Vec<(usize, Option<usize>)> {
    let mut infos = Vec::new();
    let mut start = 0usize;
    for line in source.split('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let info = if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some(indent)
        };
        infos.push((start, info));
        start += line.len() + 1;
    }
    infos
}

/// Starting at token index `from`, is there a non-trivia, non-comment token
/// before the next newline (i.e. an inline value after `key:`)?
fn line_has_inline_value_after(tokens: &[sylven_lex::Token], from: usize, source: &str) -> bool {
    for tok in &tokens[from.min(tokens.len())..] {
        match tok.kind {
            SyntaxKind::WHITESPACE => {
                let s = tok.range.start().to_usize();
                let e = tok.range.end().to_usize();
                if e <= source.len() && source[s..e].contains('\n') {
                    return false;
                }
            }
            SyntaxKind::EOF => return false,
            k if k == YamlKind::Comment.to_syntax() => return false,
            _ => return true,
        }
    }
    false
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
// Symbols — one Section symbol per top-level (column 0) mapping key
// ---------------------------------------------------------------------------

fn derive_symbols(tokens: &[sylven_lex::Token], source: &str) -> Vec<SymbolInfo> {
    let line_index = LineIndex::new(source);
    let mut symbols = Vec::new();
    for tok in tokens {
        if tok.kind != YamlKind::Key.to_syntax() {
            continue;
        }
        let pos = line_index.line_col(tok.range.start());
        if pos.col != 0 {
            continue;
        }
        let s = tok.range.start().to_usize();
        let e = tok.range.end().to_usize();
        if e > source.len() || s >= e {
            continue;
        }
        let text = &source[s..e];
        let unquoted = strip_quotes(text);
        if unquoted.is_empty() {
            continue;
        }
        let quote_len = (text.len() - unquoted.len()) / 2;
        let name_start = s + quote_len;
        let name_end = name_start + unquoted.len();
        symbols.push(SymbolInfo {
            name: unquoted.to_string(),
            name_range: TextRange::new(
                TextSize::from(name_start as u32),
                TextSize::from(name_end as u32),
            ),
            decl_range: tok.range,
            kind: SymbolKind::Section,
        });
    }
    symbols
}

/// Strip a single matching pair of surrounding `'...'` or `"..."` quotes, if
/// present.
fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &text[1..text.len() - 1];
        }
    }
    text
}

// ---------------------------------------------------------------------------
// Brackets — matching `{ }` and `[ ]` pairs (flow collections)
// ---------------------------------------------------------------------------

fn derive_brackets(tokens: &[sylven_lex::Token], source: &str) -> Vec<(TextRange, TextRange)> {
    let mut curly: Vec<TextRange> = Vec::new();
    let mut square: Vec<TextRange> = Vec::new();
    let mut pairs = Vec::new();

    for tok in tokens {
        if tok.kind != YamlKind::Punctuation.to_syntax() {
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
        YamlLanguage.parse(&snap)
    }

    fn features(source: &str) -> SyntaxFeatures {
        parse(source).features
    }

    #[test]
    fn id_is_yaml() {
        assert_eq!(YamlLanguage.id(), LanguageId("yaml"));
    }

    #[test]
    fn lossless_tree() {
        let src = "name: ozone\nlist:\n  - a\n  - b\n";
        let r = parse(src);
        assert_eq!(r.tree.text(), src);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn highlights_contain_key_string_and_number() {
        let f = features("name: \"ozone\"\nversion: 1\n");
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::String));
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::Number));
    }

    #[test]
    fn symbol_for_top_level_key() {
        let f = features("name: ozone\njobs:\n  build:\n    runs-on: ubuntu\n");
        let names: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["name", "jobs"]);
    }

    #[test]
    fn quoted_top_level_key_symbol_is_unquoted() {
        let f = features("\"on\":\n  push: {}\n");
        assert!(f.symbols.iter().any(|s| s.name == "on"));
    }

    #[test]
    fn nested_keys_are_not_symbols() {
        let f = features("jobs:\n  build:\n    steps: []\n");
        let names: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["jobs"]);
    }

    #[test]
    fn fold_for_block_mapping_body() {
        let src = "jobs:\n  build:\n    runs-on: ubuntu\n  test:\n    runs-on: ubuntu\n";
        let f = features(src);
        assert!(
            f.folds.iter().any(|r| r.start() == TextSize::from(0)),
            "expected fold for `jobs:` body"
        );
    }

    #[test]
    fn no_fold_for_key_with_inline_value() {
        let f = features("name: ozone\nversion: 1\n");
        assert!(f.folds.is_empty());
    }

    #[test]
    fn no_fold_for_key_with_no_body() {
        let src = "a:\nb: 1\n";
        let f = features(src);
        assert!(!f.folds.iter().any(|r| r.start() == TextSize::from(0)));
    }

    #[test]
    fn fold_for_multiline_flow_sequence() {
        let src = "items: [\n  1,\n  2,\n]\n";
        let f = features(src);
        assert!(
            !f.folds.is_empty(),
            "expected fold for multi-line flow sequence"
        );
    }

    #[test]
    fn brackets_matched_for_flow_mapping() {
        let f = features("a: {b: 1, c: [1, 2]}\n");
        assert_eq!(f.brackets.len(), 2);
    }

    #[test]
    fn no_symbols_for_empty_document() {
        let f = features("");
        assert!(f.symbols.is_empty());
    }
}
