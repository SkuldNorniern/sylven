//! Markdown language plugin for Sylven — Stage 1 (lexer-backed).
//!
//! Uses [`lex_markdown`] to produce a flat lossless [`SyntaxTree`] and derives
//! [`SyntaxFeatures`] (highlights, folds, symbols, fenced-code injections)
//! directly from the token stream, without a grammar-driven parse yet.

use sylven_lex::SyntaxKind;
use sylven_lex::markdown::{MarkdownKind, lex_markdown};
use sylven_parse::{ParseEvent, TokenId, build_tree};
use sylven_text::{LineIndex, TextRange, TextSize, TextSnapshot};

use crate::{
    LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures,
    result::{Highlight, HighlightKind, Injection, SymbolInfo, SymbolKind},
};

/// Node kind for the flat root FILE node.
const FILE: SyntaxKind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

/// The Markdown [`LanguagePlugin`].
pub struct MarkdownLanguage;

impl LanguagePlugin for MarkdownLanguage {
    fn id(&self) -> LanguageId {
        LanguageId("markdown")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let text = snapshot.text();
        let stream = lex_markdown(text);
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
        injections: derive_injections(tokens, source),
        brackets: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Highlights — map MarkdownKind → HighlightKind
// ---------------------------------------------------------------------------

fn markdown_kind_to_highlight(k: SyntaxKind) -> Option<HighlightKind> {
    if k == MarkdownKind::Heading.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == MarkdownKind::BlockQuote.to_syntax() || k == MarkdownKind::CodeFenceDelim.to_syntax() {
        return Some(HighlightKind::Comment);
    }
    if k == MarkdownKind::ListMarker.to_syntax() {
        return Some(HighlightKind::Operator);
    }
    if k == MarkdownKind::CodeSpan.to_syntax() || k == MarkdownKind::LinkUrl.to_syntax() {
        return Some(HighlightKind::String);
    }
    if k == MarkdownKind::LinkText.to_syntax() {
        return Some(HighlightKind::Function);
    }
    None // Text, CodeBlockBody, whitespace, EOF, ERROR
}

fn derive_highlights(tokens: &[sylven_lex::Token]) -> Vec<Highlight> {
    tokens
        .iter()
        .filter_map(|tok| {
            let kind = markdown_kind_to_highlight(tok.kind)?;
            Some(Highlight {
                range: tok.range,
                kind,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Folds — fenced code blocks, and heading sections
// ---------------------------------------------------------------------------

fn derive_folds(tokens: &[sylven_lex::Token], source: &str) -> Vec<TextRange> {
    let line_index = LineIndex::new(source);
    let mut folds = Vec::new();

    // Fenced code blocks: a fold spans the opening delimiter through the
    // closing one, but only if the fence has at least one body line — an
    // empty fence has nothing useful to hide.
    let mut in_fence = false;
    let mut fence_open: Option<TextRange> = None;
    let mut fence_has_body = false;
    for tok in tokens {
        if tok.kind == MarkdownKind::CodeFenceDelim.to_syntax() {
            if in_fence {
                if fence_has_body && let Some(open) = fence_open {
                    push_fold_if_multiline(&line_index, &mut folds, open.start(), tok.range.end());
                }
                in_fence = false;
            } else {
                in_fence = true;
                fence_open = Some(tok.range);
                fence_has_body = false;
            }
        } else if in_fence && tok.kind == MarkdownKind::CodeBlockBody.to_syntax() {
            fence_has_body = true;
        }
    }

    // Heading sections: each heading folds its body, up to (but not
    // including) the next heading of the same or a shallower level, or EOF.
    let headings: Vec<(TextRange, u8)> = tokens
        .iter()
        .filter(|t| t.kind == MarkdownKind::Heading.to_syntax())
        .map(|t| (t.range, heading_level(source, t.range)))
        .collect();
    for (i, &(range, level)) in headings.iter().enumerate() {
        let body_end = headings[i + 1..]
            .iter()
            .find(|&&(_, l)| l <= level)
            .map(|&(r, _)| r.start().to_usize())
            .unwrap_or(source.len())
            .min(source.len());
        let trimmed_end = source[..body_end].trim_end().len();
        push_fold_if_multiline(
            &line_index,
            &mut folds,
            range.start(),
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

/// Number of leading `#`s in a [`MarkdownKind::Heading`] token's text
/// (after any leading indentation).
fn heading_level(source: &str, range: TextRange) -> u8 {
    let s = range.start().to_usize();
    let e = range.end().to_usize();
    if e > source.len() || s >= e {
        return 0;
    }
    source[s..e]
        .trim_start()
        .bytes()
        .take_while(|&b| b == b'#')
        .count() as u8
}

// ---------------------------------------------------------------------------
// Symbols — one Heading symbol per ATX heading
// ---------------------------------------------------------------------------

fn derive_symbols(tokens: &[sylven_lex::Token], source: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    for tok in tokens {
        if tok.kind != MarkdownKind::Heading.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        let e = tok.range.end().to_usize();
        if e > source.len() || s >= e {
            continue;
        }
        let text = &source[s..e];
        let bytes = text.as_bytes();

        let mut after_hashes = 0;
        while after_hashes < bytes.len() && matches!(bytes[after_hashes], b' ' | b'\t') {
            after_hashes += 1;
        }
        while after_hashes < bytes.len() && bytes[after_hashes] == b'#' {
            after_hashes += 1;
        }

        let rest = &text[after_hashes..];
        let trimmed = rest.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lead_ws = rest.len() - rest.trim_start().len();
        let name_start = s + after_hashes + lead_ws;
        let name_end = name_start + trimmed.len();
        symbols.push(SymbolInfo {
            name: trimmed.to_string(),
            name_range: TextRange::new(
                TextSize::from(name_start as u32),
                TextSize::from(name_end as u32),
            ),
            decl_range: tok.range,
            kind: SymbolKind::Heading,
        });
    }
    symbols
}

// ---------------------------------------------------------------------------
// Injections — fenced code block bodies
// ---------------------------------------------------------------------------

fn derive_injections(tokens: &[sylven_lex::Token], source: &str) -> Vec<Injection> {
    let mut injections = Vec::new();
    let mut in_fence = false;
    let mut language: Option<String> = None;
    let mut body: Option<(TextSize, TextSize)> = None;

    for tok in tokens {
        if tok.kind == MarkdownKind::CodeFenceDelim.to_syntax() {
            if in_fence {
                if let Some((start, end)) = body.take() {
                    injections.push(Injection {
                        language: language.take(),
                        range: TextRange::new(start, end),
                    });
                }
                language = None;
                in_fence = false;
            } else {
                in_fence = true;
                language = fence_language(source, tok.range);
            }
        } else if in_fence && tok.kind == MarkdownKind::CodeBlockBody.to_syntax() {
            body = Some(match body {
                Some((start, _)) => (start, tok.range.end()),
                None => (tok.range.start(), tok.range.end()),
            });
        }
    }

    injections
}

/// Extracts the language tag from a fenced code block's opening
/// [`MarkdownKind::CodeFenceDelim`] token, e.g. `` ```rust `` -> `"rust"`.
/// Returns `None` for a bare fence with no language tag.
fn fence_language(source: &str, range: TextRange) -> Option<String> {
    let s = range.start().to_usize();
    let e = range.end().to_usize();
    if e > source.len() || s >= e {
        return None;
    }
    let text = source[s..e].trim_start();
    let after_fence = text.trim_start_matches(['`', '~']);
    let lang = after_fence.split_whitespace().next()?;
    Some(lang.to_string())
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
        MarkdownLanguage.parse(&snap)
    }

    fn features(source: &str) -> SyntaxFeatures {
        parse(source).features
    }

    #[test]
    fn id_is_markdown() {
        assert_eq!(MarkdownLanguage.id(), LanguageId("markdown"));
    }

    #[test]
    fn lossless_tree() {
        let src = "# Title\n\nSome text with `code` and a [link](http://x).\n";
        let r = parse(src);
        assert_eq!(r.tree.text(), src);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn highlights_contain_heading_and_code_span() {
        let f = features("# Title\n\nsee `code` here\n");
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::String));
    }

    #[test]
    fn highlights_contain_link() {
        let f = features("[label](http://example.com)\n");
        assert!(
            f.highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Function)
        );
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::String));
    }

    #[test]
    fn symbol_for_heading() {
        let f = features("# Title\n\nbody\n");
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "Title" && s.kind == SymbolKind::Heading)
        );
    }

    #[test]
    fn headings_inside_fence_are_not_symbols() {
        let src = "# Title\n```\n# not a heading\n```\n## Sub\n";
        let f = features(src);
        let names: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Title", "Sub"]);
    }

    #[test]
    fn fold_for_multiline_fence() {
        let src = "```rust\nlet x = 1;\nlet y = 2;\n```\n";
        let f = features(src);
        assert!(!f.folds.is_empty(), "expected fold for multi-line fence");
    }

    #[test]
    fn no_fold_for_empty_fence() {
        let src = "```\n```\n";
        let f = features(src);
        assert!(f.folds.is_empty());
    }

    #[test]
    fn fold_for_heading_section() {
        let src = "# A\nbody1\nbody2\n## B\nbody3\n";
        let f = features(src);
        // "# A" section spans 3 lines (heading + body1 + body2) -> foldable.
        assert!(f.folds.iter().any(|r| r.start() == TextSize::from(0)));
    }

    #[test]
    fn no_fold_for_heading_with_no_body() {
        // "# A" is immediately followed by a sibling heading at the same
        // level, so it has no body to fold.
        let src = "# A\n# B\nbody\n";
        let f = features(src);
        assert!(!f.folds.iter().any(|r| r.start() == TextSize::from(0)));
    }

    #[test]
    fn injection_for_fenced_code() {
        let src = "```rust\nlet x = 1;\nlet y = 2;\n```\n";
        let f = features(src);
        assert_eq!(f.injections.len(), 1);
        let inj = &f.injections[0];
        assert_eq!(inj.language.as_deref(), Some("rust"));
        let r = inj.range;
        assert_eq!(
            &src[r.start().to_usize()..r.end().to_usize()],
            "let x = 1;\nlet y = 2;"
        );
    }

    #[test]
    fn injection_language_none_for_bare_fence() {
        let src = "```\nplain text\n```\n";
        let f = features(src);
        assert_eq!(f.injections.len(), 1);
        assert_eq!(f.injections[0].language, None);
    }

    #[test]
    fn no_injection_for_empty_fence() {
        let src = "```\n```\n";
        let f = features(src);
        assert!(f.injections.is_empty());
    }
}
