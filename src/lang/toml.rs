//! TOML language plugin for Sylven — Stage 1 (lexer-backed).
//!
//! Uses [`lex_toml`] to produce a flat lossless [`SyntaxTree`] and derives
//! [`SyntaxFeatures`] (highlights, folds, symbols, bracket pairs) directly
//! from the token stream, without a grammar-driven parse yet.

use sylven_lex::SyntaxKind;
use sylven_lex::toml::{TomlKind, lex_toml};
use sylven_parse::{ParseEvent, TokenId, build_tree};
use sylven_text::{LineIndex, TextRange, TextSize, TextSnapshot};

use crate::{
    LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures,
    result::{Highlight, HighlightKind, SymbolInfo, SymbolKind},
};

/// Node kind for the flat root FILE node.
const FILE: SyntaxKind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

/// The TOML [`LanguagePlugin`].
pub struct TomlLanguage;

impl LanguagePlugin for TomlLanguage {
    fn id(&self) -> LanguageId {
        LanguageId("toml")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let text = snapshot.text();
        let stream = lex_toml(text);
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
// Highlights — map TomlKind → HighlightKind
// ---------------------------------------------------------------------------

fn toml_kind_to_highlight(k: SyntaxKind) -> Option<HighlightKind> {
    if k == TomlKind::Key.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == TomlKind::String.to_syntax() {
        return Some(HighlightKind::String);
    }
    if k == TomlKind::NumberLit.to_syntax()
        || k == TomlKind::BoolLit.to_syntax()
        || k == TomlKind::DateTime.to_syntax()
    {
        return Some(HighlightKind::Number);
    }
    if k == TomlKind::SectionHeader.to_syntax() {
        return Some(HighlightKind::SectionHeader);
    }
    if k == TomlKind::Comment.to_syntax() {
        return Some(HighlightKind::Comment);
    }
    if k == TomlKind::Operator.to_syntax() {
        return Some(HighlightKind::Operator);
    }
    if k == TomlKind::Punctuation.to_syntax() {
        return Some(HighlightKind::Punctuation);
    }
    None // whitespace, EOF, ERROR
}

fn derive_highlights(tokens: &[sylven_lex::Token]) -> Vec<Highlight> {
    tokens
        .iter()
        .filter_map(|tok| {
            let kind = toml_kind_to_highlight(tok.kind)?;
            Some(Highlight {
                range: tok.range,
                kind,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Folds — multi-line arrays/inline tables, and table-header sections
// ---------------------------------------------------------------------------

fn derive_folds(tokens: &[sylven_lex::Token], source: &str) -> Vec<TextRange> {
    let line_index = LineIndex::new(source);
    let mut folds = Vec::new();

    // Bracket-based folds: `[ … ]` arrays and `{ … }` inline tables that span
    // at least one line boundary.
    let mut curly: Vec<TextSize> = Vec::new();
    let mut square: Vec<TextSize> = Vec::new();
    for tok in tokens {
        if tok.kind != TomlKind::Punctuation.to_syntax() {
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

    // Section folds: each `[table]`/`[[array of tables]]` header folds its
    // body, up to (but not including) the next header or EOF.
    let headers: Vec<TextRange> = tokens
        .iter()
        .filter(|t| t.kind == TomlKind::SectionHeader.to_syntax())
        .map(|t| t.range)
        .collect();
    for (i, header) in headers.iter().enumerate() {
        let body_end = headers
            .get(i + 1)
            .map(|next| next.start().to_usize())
            .unwrap_or(source.len())
            .min(source.len());
        let trimmed_end = source[..body_end].trim_end().len();
        push_fold_if_multiline(
            &line_index,
            &mut folds,
            header.start(),
            TextSize::from(trimmed_end as u32),
        );
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
// Symbols — one Section symbol per table header
// ---------------------------------------------------------------------------

fn derive_symbols(tokens: &[sylven_lex::Token], source: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    for tok in tokens {
        if tok.kind != TomlKind::SectionHeader.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        let e = tok.range.end().to_usize();
        if e > source.len() || s >= e {
            continue;
        }
        let text = &source[s..e];
        let bytes = text.as_bytes();

        let mut open = 0;
        while bytes.get(open) == Some(&b'[') {
            open += 1;
        }
        let mut close = bytes.len();
        while close > open && bytes[close - 1] == b']' {
            close -= 1;
        }
        let inner = &text[open..close];
        let trimmed = inner.trim_start();
        let lead_ws = inner.len() - trimmed.len();
        let trimmed = trimmed.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        let name_start = s + open + lead_ws;
        let name_end = name_start + trimmed.len();
        symbols.push(SymbolInfo {
            name: trimmed.to_string(),
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

// ---------------------------------------------------------------------------
// Brackets — matching `{ }` and `[ ]` pairs (outside section headers)
// ---------------------------------------------------------------------------

fn derive_brackets(tokens: &[sylven_lex::Token], source: &str) -> Vec<(TextRange, TextRange)> {
    let mut curly: Vec<TextRange> = Vec::new();
    let mut square: Vec<TextRange> = Vec::new();
    let mut pairs = Vec::new();

    for tok in tokens {
        if tok.kind != TomlKind::Punctuation.to_syntax() {
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
        TomlLanguage.parse(&snap)
    }

    fn features(source: &str) -> SyntaxFeatures {
        parse(source).features
    }

    #[test]
    fn id_is_toml() {
        assert_eq!(TomlLanguage.id(), LanguageId("toml"));
    }

    #[test]
    fn lossless_tree() {
        let src = "[pkg]\nname = \"ozone\"\n";
        let r = parse(src);
        assert_eq!(r.tree.text(), src);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn highlights_contain_key_and_string() {
        let f = features("name = \"ozone\"\n");
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::String));
    }

    #[test]
    fn highlights_contain_section_header() {
        let f = features("[pkg]\n");
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::SectionHeader)
        );
    }

    #[test]
    fn symbol_for_table_header() {
        let f = features("[dependencies]\nfoo = \"1\"\n");
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "dependencies" && s.kind == SymbolKind::Section)
        );
    }

    #[test]
    fn symbol_for_array_table_header() {
        let f = features("[[bin]]\nname = \"ozone\"\n");
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "bin" && s.kind == SymbolKind::Section)
        );
    }

    #[test]
    fn symbol_for_dotted_header() {
        let f = features("[a.b.c]\n");
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "a.b.c" && s.kind == SymbolKind::Section)
        );
    }

    #[test]
    fn fold_for_multiline_array() {
        let src = "nums = [\n    1,\n    2,\n]\n";
        let f = features(src);
        assert!(!f.folds.is_empty(), "expected fold for multi-line array");
    }

    #[test]
    fn no_fold_for_single_line_array() {
        let f = features("nums = [1, 2, 3]\n");
        assert!(f.folds.is_empty());
    }

    #[test]
    fn fold_for_table_section_body() {
        let src = "[pkg]\nname = \"ozone\"\nversion = \"0.1.0\"\n[deps]\nfoo = \"1\"\n";
        let f = features(src);
        // [pkg] section spans 3 lines (header + 2 keys) -> foldable.
        assert!(f.folds.iter().any(|r| r.start() == TextSize::from(0)));
    }

    #[test]
    fn no_fold_for_single_line_section() {
        // Last section with no body: header line only.
        let src = "[pkg]\nname = \"x\"\n[empty]\n";
        let f = features(src);
        // [empty] is the last header with nothing after it -> no fold.
        let empty_start = src.find("[empty]").unwrap() as u32;
        assert!(
            !f.folds
                .iter()
                .any(|r| r.start() == TextSize::from(empty_start))
        );
    }

    #[test]
    fn brackets_matched_for_array() {
        let f = features("nums = [1, 2, 3]\n");
        assert!(!f.brackets.is_empty());
    }
}
