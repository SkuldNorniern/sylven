//! Hand-written Markdown lexer for Sylven.
//!
//! Produces a lossless [`TokenStream`]: concatenating every token's source
//! slice in order reproduces the original text exactly. Newlines are emitted
//! as [`SyntaxKind::WHITESPACE`] trivia, separate from the line content.
//!
//! Works on the **full document**, line by line, so fenced code blocks
//! (``` / ~~~) are tracked across lines naturally.
//!
//! Block-level classification (decided per line, on the first non-blank
//! byte):
//! - 3+ backticks/tildes toggle a fenced code block ([`MarkdownKind::CodeFenceDelim`]);
//!   lines inside the fence become [`MarkdownKind::CodeBlockBody`] (no inline
//!   scanning — an embedded-language injection range covers the whole block).
//! - `#`..`#` (1-6) followed by a space or end of line is an ATX heading
//!   ([`MarkdownKind::Heading`]) — the whole line.
//! - `>` starts a block quote ([`MarkdownKind::BlockQuote`]) — the whole line.
//! - `-`/`*`/`+ ` or `N.`/`N) ` at the start is a list marker
//!   ([`MarkdownKind::ListMarker`]), followed by inline-scanned content.
//! - Anything else is inline-scanned.
//!
//! Inline scanning recognizes `` `code spans` `` ([`MarkdownKind::CodeSpan`])
//! and `[text](url)` links ([`MarkdownKind::LinkText`] +
//! [`MarkdownKind::LinkUrl`]); everything else is [`MarkdownKind::Text`].

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

// ---------------------------------------------------------------------------
// MarkdownKind
// ---------------------------------------------------------------------------

/// Token categories produced by [`lex_markdown`].
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownKind {
    /// An ATX heading line (`#` … `######`), brackets-of-`#` included.
    Heading,
    /// A block-quote line (`> …`).
    BlockQuote,
    /// A list marker (`-`, `*`, `+`, `N.`, `N)`), the trailing space excluded.
    ListMarker,
    /// A fenced-code-block delimiter line (3+ `` ` `` or `~`).
    CodeFenceDelim,
    /// One line of content inside a fenced code block.
    CodeBlockBody,
    /// An inline `` `code span` ``, backticks included.
    CodeSpan,
    /// The `[text]` portion of a `[text](url)` link.
    LinkText,
    /// The `(url)` portion of a `[text](url)` link.
    LinkUrl,
    /// Plain prose text (including indentation).
    Text,
}

impl MarkdownKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<MarkdownKind> for SyntaxKind {
    fn from(k: MarkdownKind) -> SyntaxKind {
        k.to_syntax()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lex `text` into a lossless [`TokenStream`].
pub fn lex_markdown(text: &str) -> TokenStream {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut in_fence = false;

    while pos < bytes.len() {
        let line_start = pos;
        let mut line_end = pos;
        while line_end < bytes.len() && bytes[line_end] != b'\n' {
            line_end += 1;
        }

        let mut ts = line_start;
        while ts < line_end && matches!(bytes[ts], b' ' | b'\t') {
            ts += 1;
        }
        let is_fence_line = is_fence_marker(bytes, ts, line_end);

        if in_fence {
            if is_fence_line {
                push(
                    &mut tokens,
                    MarkdownKind::CodeFenceDelim,
                    line_start,
                    line_end,
                );
                in_fence = false;
            } else if line_end > line_start {
                push(
                    &mut tokens,
                    MarkdownKind::CodeBlockBody,
                    line_start,
                    line_end,
                );
            }
        } else if is_fence_line {
            push(
                &mut tokens,
                MarkdownKind::CodeFenceDelim,
                line_start,
                line_end,
            );
            in_fence = true;
        } else if atx_heading_level(bytes, ts, line_end).is_some() {
            push(&mut tokens, MarkdownKind::Heading, line_start, line_end);
        } else if ts < line_end && bytes[ts] == b'>' {
            push(&mut tokens, MarkdownKind::BlockQuote, line_start, line_end);
        } else {
            let mut i = line_start;
            if i < ts {
                push(&mut tokens, MarkdownKind::Text, i, ts);
                i = ts;
            }
            if let Some(marker_end) = list_marker_end(bytes, ts, line_end) {
                push(&mut tokens, MarkdownKind::ListMarker, ts, marker_end);
                i = marker_end;
            }
            lex_inline(bytes, i, line_end, &mut tokens);
        }

        pos = line_end;
        if pos < bytes.len() {
            // The newline itself.
            push_kind(&mut tokens, SyntaxKind::WHITESPACE, pos, pos + 1);
            pos += 1;
        }
    }

    let eof = TextSize::from(bytes.len() as u32);
    tokens.push(Token::new(SyntaxKind::EOF, TextRange::at(eof)));
    TokenStream::new(tokens)
}

// ---------------------------------------------------------------------------
// Block-level helpers
// ---------------------------------------------------------------------------

/// Is `bytes[ts..line_end]` a fenced-code-block delimiter — 3+ of the same
/// `` ` `` or `~`?
fn is_fence_marker(bytes: &[u8], ts: usize, line_end: usize) -> bool {
    ts + 2 < line_end
        && matches!(bytes[ts], b'`' | b'~')
        && bytes[ts + 1] == bytes[ts]
        && bytes[ts + 2] == bytes[ts]
}

/// If `bytes[ts..line_end]` starts with 1-6 `#`s followed by a space or
/// end-of-line, return the heading level.
fn atx_heading_level(bytes: &[u8], ts: usize, line_end: usize) -> Option<u8> {
    if ts >= line_end || bytes[ts] != b'#' {
        return None;
    }
    let mut h = ts;
    while h < line_end && bytes[h] == b'#' {
        h += 1;
    }
    let level = h - ts;
    if (1..=6).contains(&level) && (h >= line_end || bytes[h] == b' ') {
        Some(level as u8)
    } else {
        None
    }
}

/// If `bytes[ts..line_end]` starts with a list marker (`-`/`*`/`+` or
/// `N.`/`N)`) followed by a space, return the byte offset just past the
/// marker (the space is not included).
fn list_marker_end(bytes: &[u8], ts: usize, line_end: usize) -> Option<usize> {
    if ts >= line_end {
        return None;
    }
    if matches!(bytes[ts], b'-' | b'*' | b'+') && ts + 1 < line_end && bytes[ts + 1] == b' ' {
        return Some(ts + 1);
    }
    if bytes[ts].is_ascii_digit() {
        let mut k = ts;
        while k < line_end && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k < line_end
            && matches!(bytes[k], b'.' | b')')
            && k + 1 < line_end
            && bytes[k + 1] == b' '
        {
            return Some(k + 1);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Inline scanning
// ---------------------------------------------------------------------------

/// Scan `bytes[start..end]` for code spans and links, emitting
/// [`MarkdownKind::Text`] runs for everything else.
fn lex_inline(bytes: &[u8], start: usize, end: usize, tokens: &mut Vec<Token>) {
    let mut i = start;
    let mut run_start = start;

    while i < end {
        match bytes[i] {
            b'`' => {
                if run_start < i {
                    push(tokens, MarkdownKind::Text, run_start, i);
                }
                let span_start = i;
                i += 1;
                while i < end && bytes[i] != b'`' {
                    i += 1;
                }
                if i < end {
                    i += 1; // closing backtick
                }
                push(tokens, MarkdownKind::CodeSpan, span_start, i);
                run_start = i;
            }
            b'[' => {
                let mut k = i + 1;
                while k < end && bytes[k] != b']' {
                    k += 1;
                }
                if k + 1 < end && bytes[k] == b']' && bytes[k + 1] == b'(' {
                    let mut u = k + 2;
                    while u < end && bytes[u] != b')' {
                        u += 1;
                    }
                    if u < end {
                        if run_start < i {
                            push(tokens, MarkdownKind::Text, run_start, i);
                        }
                        push(tokens, MarkdownKind::LinkText, i, k + 1);
                        push(tokens, MarkdownKind::LinkUrl, k + 1, u + 1);
                        i = u + 1;
                        run_start = i;
                        continue;
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }

    if run_start < end {
        push(tokens, MarkdownKind::Text, run_start, end);
    }
}

// ---------------------------------------------------------------------------
// Token push helpers
// ---------------------------------------------------------------------------

fn push(tokens: &mut Vec<Token>, kind: MarkdownKind, start: usize, end: usize) {
    push_kind(tokens, kind.into(), start, end);
}

fn push_kind(tokens: &mut Vec<Token>, kind: SyntaxKind, start: usize, end: usize) {
    if end > start {
        tokens.push(Token::new(
            kind,
            TextRange::new(TextSize::from(start as u32), TextSize::from(end as u32)),
        ));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn non_trivia_kinds(text: &str) -> Vec<SyntaxKind> {
        lex_markdown(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| t.kind)
            .collect()
    }

    fn token_texts(text: &str) -> Vec<&str> {
        lex_markdown(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| &text[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "# Title\n\nSome *text* with `code` and a [link](http://x).\n\n```rust\nlet x = 1;\n```\n";
        let stream = lex_markdown(text);
        let mut rebuilt = String::new();
        for tok in stream.as_slice() {
            if tok.kind != SyntaxKind::EOF {
                rebuilt.push_str(&text[tok.range.start().to_usize()..tok.range.end().to_usize()]);
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn atx_heading() {
        let ks = non_trivia_kinds("## Title");
        assert_eq!(ks, vec![MarkdownKind::Heading.into()]);
        let txts = token_texts("## Title");
        assert_eq!(txts, vec!["## Title"]);
    }

    #[test]
    fn hash_without_space_is_not_heading() {
        let ks = non_trivia_kinds("#tag");
        assert_eq!(ks, vec![MarkdownKind::Text.into()]);
    }

    #[test]
    fn block_quote() {
        let ks = non_trivia_kinds("> quoted");
        assert_eq!(ks, vec![MarkdownKind::BlockQuote.into()]);
    }

    #[test]
    fn dash_list_marker() {
        let ks = non_trivia_kinds("- item one");
        assert_eq!(ks[0], MarkdownKind::ListMarker.into());
        assert_eq!(ks[1], MarkdownKind::Text.into());
        let txts = token_texts("- item one");
        assert_eq!(txts[0], "-");
    }

    #[test]
    fn ordered_list_marker() {
        let txts = token_texts("12) item");
        assert_eq!(txts[0], "12)");
    }

    #[test]
    fn code_span_inline() {
        let ks = non_trivia_kinds("see `code` here");
        assert!(ks.contains(&MarkdownKind::CodeSpan.into()));
        let txts = token_texts("see `code` here");
        assert!(txts.contains(&"`code`"));
    }

    #[test]
    fn link_text_and_url() {
        let txts = token_texts("[label](http://example.com)");
        assert_eq!(txts, vec!["[label]", "(http://example.com)"]);
        let ks = non_trivia_kinds("[label](http://example.com)");
        assert_eq!(
            ks,
            vec![MarkdownKind::LinkText.into(), MarkdownKind::LinkUrl.into()]
        );
    }

    #[test]
    fn fenced_code_block() {
        let text = "```rust\nlet x = 1;\n```\n";
        let ks = non_trivia_kinds(text);
        assert_eq!(
            ks,
            vec![
                MarkdownKind::CodeFenceDelim.into(),
                MarkdownKind::CodeBlockBody.into(),
                MarkdownKind::CodeFenceDelim.into(),
            ]
        );
        let txts = token_texts(text);
        assert_eq!(txts[0], "```rust");
        assert_eq!(txts[1], "let x = 1;");
        assert_eq!(txts[2], "```");
    }

    #[test]
    fn code_inside_fence_not_inline_scanned() {
        // Backticks/links inside a fence are not treated as inline code/links.
        let text = "```\n`not a span` [no](link)\n```\n";
        let ks = non_trivia_kinds(text);
        assert_eq!(
            ks,
            vec![
                MarkdownKind::CodeFenceDelim.into(),
                MarkdownKind::CodeBlockBody.into(),
                MarkdownKind::CodeFenceDelim.into(),
            ]
        );
    }

    #[test]
    fn tilde_fence() {
        let ks = non_trivia_kinds("~~~\ncode\n~~~\n");
        assert_eq!(
            ks,
            vec![
                MarkdownKind::CodeFenceDelim.into(),
                MarkdownKind::CodeBlockBody.into(),
                MarkdownKind::CodeFenceDelim.into(),
            ]
        );
    }

    #[test]
    fn plain_paragraph_is_text() {
        let ks = non_trivia_kinds("Just a plain paragraph.");
        assert_eq!(ks, vec![MarkdownKind::Text.into()]);
    }
}
