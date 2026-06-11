//! Hand-written JSON(C) lexer for Sylven.
//!
//! Produces a lossless [`TokenStream`]: concatenating every token's source
//! slice in order reproduces the original text exactly. Unknown bytes become
//! single [`SyntaxKind::ERROR`] tokens so the stream always covers the whole
//! source.
//!
//! Also accepts the common JSONC extensions (`//` and `/* */` comments),
//! matching the existing Layer-0 scanner so `.json` files with comments keep
//! highlighting correctly.
//!
//! Classification notes:
//! - A `"..."` string immediately followed by (optional whitespace and) `:`
//!   is an object [`JsonKind::Key`]; every other string is a
//!   [`JsonKind::String`] value.
//! - `true`/`false` lex as [`JsonKind::BoolLit`], `null` as
//!   [`JsonKind::NullLit`]; any other bare word becomes [`JsonKind::Ident`]
//!   (not valid JSON, but kept for lossless JSON5-ish input).

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

// ---------------------------------------------------------------------------
// JsonKind
// ---------------------------------------------------------------------------

/// Token categories produced by [`lex_json`].
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKind {
    /// An object key: a `"..."` string immediately followed by `:`.
    Key,
    /// A `"..."` string value.
    String,
    /// A JSON number literal.
    NumberLit,
    /// `true` or `false`.
    BoolLit,
    /// `null`.
    NullLit,
    /// A bare word that is none of the above (not valid JSON).
    Ident,
    /// A `//` or `/* */` comment (JSONC extension).
    Comment,
    /// `{` `}` `[` `]` `:` `,`.
    Punctuation,
}

impl JsonKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<JsonKind> for SyntaxKind {
    fn from(k: JsonKind) -> SyntaxKind {
        k.to_syntax()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lex `text` into a lossless [`TokenStream`].
pub fn lex_json(text: &str) -> TokenStream {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let start = pos;
        let kind = lex_one(bytes, text, &mut pos);
        debug_assert!(pos > start, "lex_one must always advance");
        tokens.push(Token::new(
            kind,
            TextRange::new(TextSize::from(start as u32), TextSize::from(pos as u32)),
        ));
    }

    let eof = TextSize::from(bytes.len() as u32);
    tokens.push(Token::new(SyntaxKind::EOF, TextRange::at(eof)));
    TokenStream::new(tokens)
}

// ---------------------------------------------------------------------------
// Lexer internals
// ---------------------------------------------------------------------------

fn lex_one(bytes: &[u8], text: &str, pos: &mut usize) -> SyntaxKind {
    let b = bytes[*pos];

    // Whitespace (including newlines — JSON has no line-sensitive constructs).
    if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
        while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\r' | b'\n') {
            *pos += 1;
        }
        return SyntaxKind::WHITESPACE;
    }

    // `//` line comment.
    if b == b'/' && bytes.get(*pos + 1) == Some(&b'/') {
        while *pos < bytes.len() && bytes[*pos] != b'\n' {
            *pos += 1;
        }
        return JsonKind::Comment.into();
    }

    // `/* ... */` block comment (runs to EOF if unterminated).
    if b == b'/' && bytes.get(*pos + 1) == Some(&b'*') {
        *pos += 2;
        while *pos < bytes.len() {
            if bytes[*pos] == b'*' && bytes.get(*pos + 1) == Some(&b'/') {
                *pos += 2;
                break;
            }
            *pos += 1;
        }
        return JsonKind::Comment.into();
    }

    // String — object key if the next non-space/tab byte is `:`.
    if b == b'"' {
        return lex_string(bytes, pos);
    }

    // Number.
    if b.is_ascii_digit() || (b == b'-' && bytes.get(*pos + 1).is_some_and(u8::is_ascii_digit)) {
        return lex_number(bytes, pos);
    }

    // Bare word: `true` / `false` / `null` / other identifier.
    if b.is_ascii_alphabetic() || b == b'_' {
        let start = *pos;
        while *pos < bytes.len() && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_') {
            *pos += 1;
        }
        return match &text[start..*pos] {
            "true" | "false" => JsonKind::BoolLit.into(),
            "null" => JsonKind::NullLit.into(),
            _ => JsonKind::Ident.into(),
        };
    }

    // Structural punctuation.
    if matches!(b, b'{' | b'}' | b'[' | b']' | b':' | b',') {
        *pos += 1;
        return JsonKind::Punctuation.into();
    }

    // Unknown / non-ASCII: emit one UTF-8 codepoint as ERROR.
    let len = utf8_len(b).min(bytes.len() - *pos);
    *pos += len;
    SyntaxKind::ERROR
}

/// Lex a `"..."` string (handling `\"` escapes), classifying it as
/// [`JsonKind::Key`] if followed by `:` (skipping spaces/tabs), or
/// [`JsonKind::String`] otherwise. Stops at an unescaped `"`, a newline, or
/// EOF (unterminated strings stay lossless).
fn lex_string(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    *pos += 1; // opening `"`
    while *pos < bytes.len() {
        match bytes[*pos] {
            b'\\' if *pos + 1 < bytes.len() => *pos += 2,
            b'"' => {
                *pos += 1;
                break;
            }
            b'\n' => break, // unterminated — stop before the newline
            _ => *pos += 1,
        }
    }

    let mut j = *pos;
    while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
        j += 1;
    }
    if bytes.get(j) == Some(&b':') {
        JsonKind::Key.into()
    } else {
        JsonKind::String.into()
    }
}

/// Lex a JSON number: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
fn lex_number(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    if bytes[*pos] == b'-' {
        *pos += 1;
    }
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if bytes.get(*pos) == Some(&b'.') && bytes.get(*pos + 1).is_some_and(u8::is_ascii_digit) {
        *pos += 1;
        while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
    }
    if matches!(bytes.get(*pos), Some(&b'e') | Some(&b'E')) {
        *pos += 1;
        if matches!(bytes.get(*pos), Some(&b'+') | Some(&b'-')) {
            *pos += 1;
        }
        while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
    }
    JsonKind::NumberLit.into()
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn non_trivia_kinds(text: &str) -> Vec<SyntaxKind> {
        lex_json(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| t.kind)
            .collect()
    }

    fn token_texts(text: &str) -> Vec<&str> {
        lex_json(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| &text[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "{\n  \"a\": 1,\n  \"b\": [true, false, null]\n}\n";
        let stream = lex_json(text);
        let mut rebuilt = String::new();
        for tok in stream.as_slice() {
            if tok.kind != SyntaxKind::EOF {
                rebuilt.push_str(&text[tok.range.start().to_usize()..tok.range.end().to_usize()]);
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn key_vs_string_value() {
        let ks = non_trivia_kinds(r#"{"name": "ozone"}"#);
        assert_eq!(
            ks,
            vec![
                JsonKind::Punctuation.into(), // {
                JsonKind::Key.into(),         // "name"
                JsonKind::Punctuation.into(), // :
                JsonKind::String.into(),      // "ozone"
                JsonKind::Punctuation.into(), // }
            ]
        );
    }

    #[test]
    fn numbers() {
        let ks = non_trivia_kinds(r#"[1, -2, 3.14, 1e10, -1.5e-3]"#);
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == JsonKind::NumberLit.into())
                .count(),
            5
        );
    }

    #[test]
    fn booleans_and_null() {
        let ks = non_trivia_kinds(r#"[true, false, null]"#);
        assert_eq!(
            ks,
            vec![
                JsonKind::Punctuation.into(),
                JsonKind::BoolLit.into(),
                JsonKind::Punctuation.into(),
                JsonKind::BoolLit.into(),
                JsonKind::Punctuation.into(),
                JsonKind::NullLit.into(),
                JsonKind::Punctuation.into(),
            ]
        );
    }

    #[test]
    fn line_comment() {
        let txts = token_texts("// hello\n{}");
        assert_eq!(txts[0], "// hello");
    }

    #[test]
    fn block_comment() {
        let txts = token_texts("/* a\nb */{}");
        assert_eq!(txts[0], "/* a\nb */");
    }

    #[test]
    fn unterminated_block_comment_runs_to_eof() {
        let txts = token_texts("/* unterminated");
        assert_eq!(txts, vec!["/* unterminated"]);
    }

    #[test]
    fn nested_object_and_array() {
        let ks = non_trivia_kinds(r#"{"a": {"b": [1, 2]}}"#);
        assert!(ks.contains(&JsonKind::Key.into()));
        assert!(
            ks.iter()
                .filter(|&&k| k == JsonKind::Punctuation.into())
                .count()
                >= 6
        );
    }

    #[test]
    fn escaped_quote_in_string() {
        let ks = non_trivia_kinds(r#""a\"b": 1"#);
        assert_eq!(ks[0], JsonKind::Key.into());
    }

    #[test]
    fn bare_identifier_is_ident() {
        let ks = non_trivia_kinds("undefined");
        assert_eq!(ks, vec![JsonKind::Ident.into()]);
    }
}
