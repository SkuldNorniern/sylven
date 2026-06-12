//! Hand-written TOML lexer for Sylven.
//!
//! Produces a lossless [`TokenStream`]: concatenating every token's source
//! slice in order reproduces the original text exactly. Unknown bytes become
//! single [`SyntaxKind::ERROR`] tokens so the stream always covers the whole
//! source.
//!
//! Works on the **full document** — multi-line basic/literal strings
//! (`"""..."""`, `'''...'''`) and table-header detection are handled
//! naturally at this level.
//!
//! Classification notes:
//! - The only bare (unquoted) words allowed as TOML *values* are `true`,
//!   `false`, `inf`, and `nan` — every other bare word is a key (or a
//!   segment of a dotted key / table header). This lets the lexer classify
//!   bare words without tracking key/value position.
//! - `[` only opens a [`TomlKind::SectionHeader`] when it is the first
//!   non-whitespace byte on its line; elsewhere `[`/`]` are punctuation
//!   (array literals).

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

// ---------------------------------------------------------------------------
// TomlKind
// ---------------------------------------------------------------------------

/// Token categories produced by [`lex_toml`].
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TomlKind {
    /// A bare or quoted key segment: in `key = value`, a dotted key
    /// (`a.b.c`), or a segment of a [`TomlKind::SectionHeader`].
    Key,
    /// A basic `"..."`/`"""..."""` or literal `'...'`/`'''...'''` string.
    String,
    /// An integer or floating-point literal, including the special floats
    /// `inf` and `nan`.
    NumberLit,
    /// `true` or `false`.
    BoolLit,
    /// An RFC 3339 local date, local time, or (offset) date-time.
    DateTime,
    /// A `[table]` or `[[array of tables]]` header, brackets included.
    SectionHeader,
    /// A `#` comment to end of line (the newline is a separate
    /// [`SyntaxKind::WHITESPACE`] token).
    Comment,
    /// `=`.
    Operator,
    /// `.` `,` `{` `}` `[` `]` outside of a [`TomlKind::SectionHeader`].
    Punctuation,
}

impl TomlKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<TomlKind> for SyntaxKind {
    fn from(k: TomlKind) -> SyntaxKind {
        k.to_syntax()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lex `text` into a lossless [`TokenStream`].
pub fn lex_toml(text: &str) -> TokenStream {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut at_line_start = true;

    while pos < bytes.len() {
        let start = pos;
        let kind = lex_one(bytes, text, &mut pos, &mut at_line_start);
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

fn lex_one(bytes: &[u8], text: &str, pos: &mut usize, at_line_start: &mut bool) -> SyntaxKind {
    let b = bytes[*pos];

    // Newline — resets line-start tracking.
    if b == b'\n' {
        *pos += 1;
        *at_line_start = true;
        return SyntaxKind::WHITESPACE;
    }

    // Other whitespace — doesn't affect line-start tracking.
    if matches!(b, b' ' | b'\t' | b'\r') {
        while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\r') {
            *pos += 1;
        }
        return SyntaxKind::WHITESPACE;
    }

    let was_line_start = *at_line_start;
    *at_line_start = false;

    // Comment: `#` to end of line.
    if b == b'#' {
        while *pos < bytes.len() && bytes[*pos] != b'\n' {
            *pos += 1;
        }
        return TomlKind::Comment.into();
    }

    // Table / array-of-tables header: `[` as the first byte on a line.
    if b == b'[' && was_line_start {
        return lex_section_header(bytes, pos);
    }

    // Strings
    if b == b'"' {
        return lex_basic_string(bytes, pos);
    }
    if b == b'\'' {
        return lex_literal_string(bytes, pos);
    }

    // Numbers and date/time literals.
    if b.is_ascii_digit() {
        return lex_number_or_datetime(bytes, pos);
    }
    if matches!(b, b'+' | b'-') && bytes.get(*pos + 1).is_some_and(u8::is_ascii_digit) {
        return lex_number_or_datetime(bytes, pos);
    }

    // Bare key / `true` / `false` / `inf` / `nan`.
    if b.is_ascii_alphabetic() || b == b'_' {
        let start = *pos;
        while *pos < bytes.len()
            && matches!(bytes[*pos], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
        {
            *pos += 1;
        }
        let word = &text[start..*pos];
        return match word {
            "true" | "false" => TomlKind::BoolLit.into(),
            "inf" | "nan" => TomlKind::NumberLit.into(),
            _ => TomlKind::Key.into(),
        };
    }

    // `=`
    if b == b'=' {
        *pos += 1;
        return TomlKind::Operator.into();
    }

    // Punctuation: `.` `,` `{` `}` `[` `]`
    if matches!(b, b'.' | b',' | b'{' | b'}' | b'[' | b']') {
        *pos += 1;
        return TomlKind::Punctuation.into();
    }

    // Bare `+`/`-` not followed by a digit (invalid TOML, but keep lossless).
    if matches!(b, b'+' | b'-') {
        *pos += 1;
        return TomlKind::Operator.into();
    }

    // Unknown / non-ASCII: emit one UTF-8 codepoint as ERROR.
    let len = utf8_len(b).min(bytes.len() - *pos);
    *pos += len;
    SyntaxKind::ERROR
}

/// Lex a `[table]` or `[[array of tables]]` header, brackets included.
/// Quoted key segments are skipped over so a `]`/`'`/`"` inside them doesn't
/// end the header early.
fn lex_section_header(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    *pos += 1; // first `[`
    let is_array = bytes.get(*pos) == Some(&b'[');
    if is_array {
        *pos += 1;
    }
    while *pos < bytes.len() && bytes[*pos] != b'\n' {
        match bytes[*pos] {
            b'"' => skip_basic_string_body(bytes, pos),
            b'\'' => skip_literal_string_body(bytes, pos),
            b']' => {
                *pos += 1;
                if is_array && bytes.get(*pos) == Some(&b']') {
                    *pos += 1;
                }
                break;
            }
            _ => *pos += 1,
        }
    }
    TomlKind::SectionHeader.into()
}

/// Advance past a `"..."` quoted-key body (handling `\"` escapes), without
/// emitting a token. Stops at an unescaped `"`, a newline, or EOF.
fn skip_basic_string_body(bytes: &[u8], pos: &mut usize) {
    *pos += 1; // opening `"`
    while *pos < bytes.len() && bytes[*pos] != b'\n' {
        match bytes[*pos] {
            b'\\' if *pos + 1 < bytes.len() => *pos += 2,
            b'"' => {
                *pos += 1;
                return;
            }
            _ => *pos += 1,
        }
    }
}

/// Advance past a `'...'` quoted-key body, without emitting a token. Stops at
/// `'`, a newline, or EOF.
fn skip_literal_string_body(bytes: &[u8], pos: &mut usize) {
    *pos += 1; // opening `'`
    while *pos < bytes.len() && bytes[*pos] != b'\n' && bytes[*pos] != b'\'' {
        *pos += 1;
    }
    if bytes.get(*pos) == Some(&b'\'') {
        *pos += 1;
    }
}

/// Lex a `"..."` or `"""..."""` basic string.
fn lex_basic_string(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    if bytes.get(*pos + 1) == Some(&b'"') && bytes.get(*pos + 2) == Some(&b'"') {
        *pos += 3;
        while *pos < bytes.len() {
            if bytes[*pos] == b'\\' && *pos + 1 < bytes.len() {
                *pos += 2;
                continue;
            }
            if bytes[*pos] == b'"'
                && bytes.get(*pos + 1) == Some(&b'"')
                && bytes.get(*pos + 2) == Some(&b'"')
            {
                *pos += 3;
                break;
            }
            *pos += 1;
        }
        return TomlKind::String.into();
    }

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
    TomlKind::String.into()
}

/// Lex a `'...'` or `'''...'''` literal string (no escapes).
fn lex_literal_string(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    if bytes.get(*pos + 1) == Some(&b'\'') && bytes.get(*pos + 2) == Some(&b'\'') {
        *pos += 3;
        while *pos < bytes.len() {
            if bytes[*pos] == b'\''
                && bytes.get(*pos + 1) == Some(&b'\'')
                && bytes.get(*pos + 2) == Some(&b'\'')
            {
                *pos += 3;
                break;
            }
            *pos += 1;
        }
        return TomlKind::String.into();
    }

    *pos += 1; // opening `'`
    while *pos < bytes.len() && bytes[*pos] != b'\'' && bytes[*pos] != b'\n' {
        *pos += 1;
    }
    if bytes.get(*pos) == Some(&b'\'') {
        *pos += 1;
    }
    TomlKind::String.into()
}

/// Lex a numeric literal or an RFC 3339 date/time literal.
fn lex_number_or_datetime(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    let has_sign = matches!(bytes[*pos], b'+' | b'-');
    if has_sign {
        *pos += 1;
    }

    // Local date, optionally followed by a time component.
    if !has_sign && is_date_at(bytes, *pos) {
        *pos += 10; // YYYY-MM-DD
        if *pos < bytes.len()
            && matches!(bytes[*pos], b'T' | b't' | b' ')
            && is_time_at(bytes, *pos + 1)
        {
            *pos += 1; // `T` / `t` / ` `
            consume_time_and_offset(bytes, pos);
        }
        return TomlKind::DateTime.into();
    }

    // Local time on its own.
    if !has_sign && is_time_at(bytes, *pos) {
        consume_time_and_offset(bytes, pos);
        return TomlKind::DateTime.into();
    }

    // Hex / octal / binary integers (no sign allowed by TOML).
    if !has_sign && bytes[*pos] == b'0' {
        match bytes.get(*pos + 1) {
            Some(&b'x') | Some(&b'X') => {
                *pos += 2;
                while *pos < bytes.len() && (bytes[*pos].is_ascii_hexdigit() || bytes[*pos] == b'_')
                {
                    *pos += 1;
                }
                return TomlKind::NumberLit.into();
            }
            Some(&b'o') | Some(&b'O') => {
                *pos += 2;
                while *pos < bytes.len()
                    && (matches!(bytes[*pos], b'0'..=b'7') || bytes[*pos] == b'_')
                {
                    *pos += 1;
                }
                return TomlKind::NumberLit.into();
            }
            Some(&b'b') | Some(&b'B') => {
                *pos += 2;
                while *pos < bytes.len()
                    && (bytes[*pos] == b'0' || bytes[*pos] == b'1' || bytes[*pos] == b'_')
                {
                    *pos += 1;
                }
                return TomlKind::NumberLit.into();
            }
            _ => {}
        }
    }

    // Decimal integer or float.
    while *pos < bytes.len() && (bytes[*pos].is_ascii_digit() || bytes[*pos] == b'_') {
        *pos += 1;
    }
    if bytes.get(*pos) == Some(&b'.') && bytes.get(*pos + 1).is_some_and(u8::is_ascii_digit) {
        *pos += 1;
        while *pos < bytes.len() && (bytes[*pos].is_ascii_digit() || bytes[*pos] == b'_') {
            *pos += 1;
        }
    }
    if matches!(bytes.get(*pos), Some(&b'e') | Some(&b'E')) {
        *pos += 1;
        if matches!(bytes.get(*pos), Some(&b'+') | Some(&b'-')) {
            *pos += 1;
        }
        while *pos < bytes.len() && (bytes[*pos].is_ascii_digit() || bytes[*pos] == b'_') {
            *pos += 1;
        }
    }
    TomlKind::NumberLit.into()
}

fn is_digit_at(bytes: &[u8], p: usize) -> bool {
    bytes.get(p).is_some_and(u8::is_ascii_digit)
}

/// Does `bytes[p..p+10]` look like `YYYY-MM-DD`?
fn is_date_at(bytes: &[u8], p: usize) -> bool {
    is_digit_at(bytes, p)
        && is_digit_at(bytes, p + 1)
        && is_digit_at(bytes, p + 2)
        && is_digit_at(bytes, p + 3)
        && bytes.get(p + 4) == Some(&b'-')
        && is_digit_at(bytes, p + 5)
        && is_digit_at(bytes, p + 6)
        && bytes.get(p + 7) == Some(&b'-')
        && is_digit_at(bytes, p + 8)
        && is_digit_at(bytes, p + 9)
}

/// Does `bytes[p..p+8]` look like `HH:MM:SS`?
fn is_time_at(bytes: &[u8], p: usize) -> bool {
    is_digit_at(bytes, p)
        && is_digit_at(bytes, p + 1)
        && bytes.get(p + 2) == Some(&b':')
        && is_digit_at(bytes, p + 3)
        && is_digit_at(bytes, p + 4)
        && bytes.get(p + 5) == Some(&b':')
        && is_digit_at(bytes, p + 6)
        && is_digit_at(bytes, p + 7)
}

/// Consume `HH:MM:SS`, an optional `.fraction`, and an optional `Z`/`±HH:MM`
/// offset.
fn consume_time_and_offset(bytes: &[u8], pos: &mut usize) {
    *pos += 8; // HH:MM:SS
    if bytes.get(*pos) == Some(&b'.') && is_digit_at(bytes, *pos + 1) {
        *pos += 1;
        while is_digit_at(bytes, *pos) {
            *pos += 1;
        }
    }
    match bytes.get(*pos) {
        Some(&b'Z') | Some(&b'z') => *pos += 1,
        Some(&b'+') | Some(&b'-')
            if is_digit_at(bytes, *pos + 1)
                && is_digit_at(bytes, *pos + 2)
                && bytes.get(*pos + 3) == Some(&b':')
                && is_digit_at(bytes, *pos + 4)
                && is_digit_at(bytes, *pos + 5) =>
        {
            *pos += 6;
        }
        _ => {}
    }
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
        lex_toml(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| t.kind)
            .collect()
    }

    /// Non-trivia token texts — whitespace tokens are filtered out so value
    /// positions have stable indices regardless of surrounding spacing.
    fn token_texts(text: &str) -> Vec<&str> {
        lex_toml(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| &text[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "# header\n[pkg]\nname = \"ozone\"\nversion = '0.1.0'\n";
        let stream = lex_toml(text);
        let mut rebuilt = String::new();
        for tok in stream.as_slice() {
            if tok.kind != SyntaxKind::EOF {
                rebuilt.push_str(&text[tok.range.start().to_usize()..tok.range.end().to_usize()]);
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn key_value_pair() {
        let ks = non_trivia_kinds(r#"name = "ozone""#);
        assert_eq!(
            ks,
            vec![
                TomlKind::Key.into(),
                TomlKind::Operator.into(),
                TomlKind::String.into(),
            ]
        );
    }

    #[test]
    fn dotted_key() {
        let ks = non_trivia_kinds("a.b.c = 1");
        assert_eq!(
            ks,
            vec![
                TomlKind::Key.into(),
                TomlKind::Punctuation.into(),
                TomlKind::Key.into(),
                TomlKind::Punctuation.into(),
                TomlKind::Key.into(),
                TomlKind::Operator.into(),
                TomlKind::NumberLit.into(),
            ]
        );
    }

    #[test]
    fn table_header() {
        let ks = non_trivia_kinds("[dependencies]");
        assert_eq!(ks, vec![TomlKind::SectionHeader.into()]);
        let txts = token_texts("[dependencies]");
        assert_eq!(txts, vec!["[dependencies]"]);
    }

    #[test]
    fn array_table_header() {
        let ks = non_trivia_kinds("[[bin]]");
        assert_eq!(ks, vec![TomlKind::SectionHeader.into()]);
    }

    #[test]
    fn dotted_table_header() {
        let txts = token_texts("[a.b.c]");
        assert_eq!(txts, vec!["[a.b.c]"]);
    }

    #[test]
    fn quoted_key_in_header_with_bracket() {
        // A `]` inside a quoted header segment must not end the header early.
        let txts = token_texts("[\"a]b\".c]");
        assert_eq!(txts, vec!["[\"a]b\".c]"]);
    }

    #[test]
    fn bracket_only_at_line_start_is_header() {
        // `[` mid-line (after `=`) is an array literal, not a header.
        let ks = non_trivia_kinds("nums = [1, 2, 3]");
        assert_eq!(ks[0], TomlKind::Key.into());
        assert_eq!(ks[1], TomlKind::Operator.into());
        assert_eq!(ks[2], TomlKind::Punctuation.into()); // `[`
        assert!(ks.contains(&TomlKind::NumberLit.into()));
        assert_eq!(*ks.last().unwrap(), TomlKind::Punctuation.into()); // `]`
    }

    #[test]
    fn comment_to_end_of_line() {
        let txts = token_texts("# a comment\nkey = 1");
        assert_eq!(txts[0], "# a comment");
    }

    #[test]
    fn booleans() {
        let ks = non_trivia_kinds("a = true\nb = false");
        assert!(ks.contains(&TomlKind::BoolLit.into()));
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == TomlKind::BoolLit.into())
                .count(),
            2
        );
    }

    #[test]
    fn integers_and_floats() {
        let ks =
            non_trivia_kinds("a = 42\nb = -17\nc = 3.14\nd = 1e10\ne = 0xFF\nf = 0o17\ng = 0b101");
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == TomlKind::NumberLit.into())
                .count(),
            7
        );
    }

    #[test]
    fn special_floats() {
        let ks = non_trivia_kinds("a = inf\nb = nan\nc = -inf");
        assert!(
            ks.iter()
                .filter(|&&k| k == TomlKind::NumberLit.into())
                .count()
                >= 3
        );
    }

    #[test]
    fn local_date() {
        let ks = non_trivia_kinds("d = 1979-05-27");
        assert!(ks.contains(&TomlKind::DateTime.into()));
        let txts = token_texts("d = 1979-05-27");
        assert_eq!(txts[2], "1979-05-27");
    }

    #[test]
    fn local_time() {
        let txts = token_texts("d = 07:32:00");
        assert_eq!(txts[2], "07:32:00");
    }

    #[test]
    fn offset_date_time() {
        let txts = token_texts("d = 1979-05-27T07:32:00Z");
        assert_eq!(txts[2], "1979-05-27T07:32:00Z");
    }

    #[test]
    fn offset_date_time_with_fraction_and_offset() {
        let txts = token_texts("d = 1979-05-27T00:32:00.999-07:00");
        assert_eq!(txts[2], "1979-05-27T00:32:00.999-07:00");
    }

    #[test]
    fn multiline_basic_string() {
        let text = "s = \"\"\"\nline1\nline2\"\"\"";
        let txts = token_texts(text);
        assert_eq!(txts[2], "\"\"\"\nline1\nline2\"\"\"");
    }

    #[test]
    fn multiline_literal_string() {
        let text = "s = '''\nraw\\path'''";
        let txts = token_texts(text);
        assert_eq!(txts[2], "'''\nraw\\path'''");
    }

    #[test]
    fn basic_string_with_escapes() {
        let ks = non_trivia_kinds(r#"s = "a\"b""#);
        assert_eq!(
            ks,
            vec![
                TomlKind::Key.into(),
                TomlKind::Operator.into(),
                TomlKind::String.into(),
            ]
        );
    }

    #[test]
    fn inline_table_and_array() {
        let ks = non_trivia_kinds("point = { x = 1, y = 2 }");
        assert!(ks.contains(&TomlKind::Key.into()));
        assert!(ks.contains(&TomlKind::Punctuation.into()));
        assert!(ks.contains(&TomlKind::NumberLit.into()));
    }
}
