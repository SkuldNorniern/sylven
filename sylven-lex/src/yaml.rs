//! Hand-written YAML 1.1/1.2 (subset) lexer for Sylven.
//!
//! Produces a lossless [`TokenStream`]: concatenating every token's source
//! slice in order reproduces the original text exactly. Unknown bytes become
//! single [`SyntaxKind::ERROR`] tokens so the stream always covers the whole
//! source.
//!
//! Classification notes:
//! - A bare or quoted scalar immediately followed by `:` (then whitespace,
//!   `#`, a flow terminator, or end of line/input) is a [`YamlKind::Key`];
//!   every other scalar is classified by its content (string/number/bool/
//!   null/plain).
//! - `-` followed by whitespace or end of line/input is a block sequence
//!   marker ([`YamlKind::Operator`]); a `-` immediately followed by another
//!   character (e.g. `-1`, `-x`) is part of a scalar.
//! - `---`/`...` document markers, `&anchor`, `*alias`, and `!tag`/`!!tag`
//!   are recognized at the start of a token.
//!
//! Known limitations (documented stopgaps, not yet handled):
//! - Plain scalar **keys containing spaces** (e.g. `My Key: value`) are not
//!   supported — the bare-word lexer stops at whitespace, so only the last
//!   word before `:` is classified as the key.
//! - Block scalar bodies (`|`/`>`) are lexed as ordinary lines, not as a
//!   single literal/folded block — the indicator itself is recognized as
//!   [`YamlKind::Operator`], but its body is tokenized normally.
//! - `yes`/`no`/`on`/`off` (YAML 1.1 boolean aliases) are treated as plain
//!   scalars, not [`YamlKind::BoolLit`].

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

// ---------------------------------------------------------------------------
// YamlKind
// ---------------------------------------------------------------------------

/// Token categories produced by [`lex_yaml`].
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YamlKind {
    /// A mapping key: a bare or quoted scalar immediately followed by `:`.
    Key,
    /// A single- or double-quoted scalar value.
    String,
    /// A numeric scalar value (integer, float, hex, octal, `.inf`/`.nan`).
    NumberLit,
    /// `true`/`True`/`TRUE`/`false`/`False`/`FALSE`.
    BoolLit,
    /// `null`/`Null`/`NULL`/`~`.
    NullLit,
    /// `&anchor`.
    Anchor,
    /// `*alias`.
    Alias,
    /// `!tag` or `!!tag` or `!<uri>`.
    Tag,
    /// A `#` comment to end of line.
    Comment,
    /// A `---` or `...` document marker.
    DocumentMarker,
    /// `-` (block sequence entry), `?` (explicit key), `:` (value
    /// indicator), or `|`/`>` (block scalar indicator).
    Operator,
    /// `,` `{` `}` `[` `]` (flow collection delimiters).
    Punctuation,
    /// A bare scalar value that is not a key, bool, null, or number.
    PlainScalar,
}

impl YamlKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<YamlKind> for SyntaxKind {
    fn from(k: YamlKind) -> SyntaxKind {
        k.to_syntax()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lex `text` into a lossless [`TokenStream`].
pub fn lex_yaml(text: &str) -> TokenStream {
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

    // Comment: `#` at line start, or preceded by whitespace, runs to EOL.
    if b == b'#' && (was_line_start || matches!(bytes[*pos - 1], b' ' | b'\t')) {
        while *pos < bytes.len() && bytes[*pos] != b'\n' {
            *pos += 1;
        }
        return YamlKind::Comment.into();
    }

    // Document markers: `---` / `...` as the first token on a line.
    if was_line_start && bytes.len() >= *pos + 3 {
        let three = &bytes[*pos..*pos + 3];
        if (three == b"---" || three == b"...") && is_colon_terminator(bytes.get(*pos + 3).copied())
        {
            *pos += 3;
            return YamlKind::DocumentMarker.into();
        }
    }

    // Block sequence entry: `-` followed by whitespace/EOL/EOF.
    if b == b'-' && is_colon_terminator(bytes.get(*pos + 1).copied()) {
        *pos += 1;
        return YamlKind::Operator.into();
    }

    // Explicit key: `?` followed by whitespace/EOL/EOF.
    if b == b'?' && is_colon_terminator(bytes.get(*pos + 1).copied()) {
        *pos += 1;
        return YamlKind::Operator.into();
    }

    // Standalone value indicator: `:` followed by whitespace/EOL/EOF/flow
    // terminator (a `:` with no preceding scalar on this token boundary).
    if b == b':' && is_colon_terminator(bytes.get(*pos + 1).copied()) {
        *pos += 1;
        return YamlKind::Operator.into();
    }

    // Flow collection delimiters.
    if matches!(b, b',' | b'{' | b'}' | b'[' | b']') {
        *pos += 1;
        return YamlKind::Punctuation.into();
    }

    // Anchor: `&name`.
    if b == b'&' {
        *pos += 1;
        consume_plain_run(bytes, pos);
        return YamlKind::Anchor.into();
    }

    // Alias: `*name`.
    if b == b'*' {
        *pos += 1;
        consume_plain_run(bytes, pos);
        return YamlKind::Alias.into();
    }

    // Tag: `!tag`, `!!tag`, or `!<uri>`.
    if b == b'!' {
        *pos += 1;
        if bytes.get(*pos) == Some(&b'!') {
            *pos += 1;
        }
        if bytes.get(*pos) == Some(&b'<') {
            *pos += 1;
            while *pos < bytes.len() && bytes[*pos] != b'>' && bytes[*pos] != b'\n' {
                *pos += 1;
            }
            if bytes.get(*pos) == Some(&b'>') {
                *pos += 1;
            }
        } else {
            consume_plain_run(bytes, pos);
        }
        return YamlKind::Tag.into();
    }

    // Block scalar indicator: `|` or `>`, with optional chomping (`+`/`-`)
    // and indentation digit, in either order.
    if matches!(b, b'|' | b'>') {
        *pos += 1;
        for _ in 0..2 {
            match bytes.get(*pos) {
                Some(b'+') | Some(b'-') => *pos += 1,
                Some(d) if d.is_ascii_digit() => *pos += 1,
                _ => break,
            }
        }
        return YamlKind::Operator.into();
    }

    // Quoted scalars.
    if b == b'\'' {
        return lex_single_quoted(bytes, pos);
    }
    if b == b'"' {
        return lex_double_quoted(bytes, pos);
    }

    // Everything else: a plain scalar word.
    lex_plain_scalar(bytes, text, pos)
}

/// Is `b` (the byte after a `:`/`-`/`?`/document-marker candidate) a valid
/// terminator — i.e. does it make the preceding byte(s) a standalone
/// indicator rather than part of a scalar?
fn is_colon_terminator(b: Option<u8>) -> bool {
    matches!(
        b,
        None | Some(b' ' | b'\t' | b'\r' | b'\n' | b'#' | b',' | b'}' | b']')
    )
}

/// Consume a run of bytes suitable for an anchor/alias/tag name: anything
/// but whitespace and flow indicators.
fn consume_plain_run(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len()
        && !matches!(
            bytes[*pos],
            b' ' | b'\t' | b'\r' | b'\n' | b',' | b'{' | b'}' | b'[' | b']'
        )
    {
        *pos += 1;
    }
}

/// Lex a `'...'` single-quoted scalar (`''` is an escaped literal `'`).
/// Stops at an unescaped `'`, a newline, or EOF (unterminated strings stay
/// lossless). Classifies as [`YamlKind::Key`] if followed by a value-position
/// `:`.
fn lex_single_quoted(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    *pos += 1; // opening `'`
    while *pos < bytes.len() {
        match bytes[*pos] {
            b'\'' if bytes.get(*pos + 1) == Some(&b'\'') => *pos += 2,
            b'\'' => {
                *pos += 1;
                break;
            }
            b'\n' => break, // unterminated — stop before the newline
            _ => *pos += 1,
        }
    }
    if peek_is_key_colon(bytes, *pos) {
        YamlKind::Key.into()
    } else {
        YamlKind::String.into()
    }
}

/// Lex a `"..."` double-quoted scalar (`\"` and other backslash escapes).
/// Stops at an unescaped `"`, a newline, or EOF (unterminated strings stay
/// lossless). Classifies as [`YamlKind::Key`] if followed by a value-position
/// `:`.
fn lex_double_quoted(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
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
    if peek_is_key_colon(bytes, *pos) {
        YamlKind::Key.into()
    } else {
        YamlKind::String.into()
    }
}

/// Lex a bare (plain) scalar word: a maximal run of bytes that aren't
/// whitespace, flow delimiters, a comment-introducing `#`, or a value-
/// position `:`. Classifies the result as [`YamlKind::Key`] if followed by a
/// value-position `:`, otherwise by content (bool/null/number/plain).
fn lex_plain_scalar(bytes: &[u8], text: &str, pos: &mut usize) -> SyntaxKind {
    let start = *pos;
    while *pos < bytes.len() {
        let b = bytes[*pos];
        if matches!(
            b,
            b' ' | b'\t' | b'\r' | b'\n' | b',' | b'{' | b'}' | b'[' | b']'
        ) {
            break;
        }
        if b == b'#' && *pos > start && matches!(bytes[*pos - 1], b' ' | b'\t') {
            break;
        }
        if b == b':' && is_colon_terminator(bytes.get(*pos + 1).copied()) {
            break;
        }
        *pos += 1;
    }
    if *pos == start {
        // First byte didn't fit any other rule and can't start a plain
        // scalar either (e.g. a lone `:` followed by a non-terminator,
        // which already advanced nothing) — emit it as ERROR so the lexer
        // always makes progress.
        *pos += 1;
        return SyntaxKind::ERROR;
    }

    let word = &text[start..*pos];
    if peek_is_key_colon(bytes, *pos) {
        return YamlKind::Key.into();
    }
    classify_plain_word(word)
}

/// Does the input at `p` (after skipping spaces/tabs) start with a
/// value-position `:` — i.e. should the scalar ending at `p` be classified
/// as [`YamlKind::Key`]?
fn peek_is_key_colon(bytes: &[u8], mut p: usize) -> bool {
    while p < bytes.len() && matches!(bytes[p], b' ' | b'\t') {
        p += 1;
    }
    bytes.get(p) == Some(&b':') && is_colon_terminator(bytes.get(p + 1).copied())
}

/// Classify a plain scalar word by content: bool / null / number / plain.
fn classify_plain_word(word: &str) -> SyntaxKind {
    match word {
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => YamlKind::BoolLit.into(),
        "null" | "Null" | "NULL" | "~" => YamlKind::NullLit.into(),
        _ if looks_like_number(word) => YamlKind::NumberLit.into(),
        _ => YamlKind::PlainScalar.into(),
    }
}

/// Does `word` look like a YAML core-schema number: a decimal integer or
/// float (with optional sign and exponent), `0x`/`0o` integer, or
/// `[+-]?.inf`/`.nan` (any case)?
fn looks_like_number(word: &str) -> bool {
    let body = word.strip_prefix(['+', '-']).unwrap_or(word);
    if body.is_empty() {
        return false;
    }
    if matches!(body, ".inf" | ".Inf" | ".INF" | ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        return !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(oct) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        return !oct.is_empty() && oct.bytes().all(|b| (b'0'..=b'7').contains(&b));
    }

    let bytes = body.as_bytes();
    let mut i = 0;
    let mut saw_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        saw_digit = true;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            saw_digit = true;
        }
    }
    if !saw_digit {
        return false;
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false; // `e`/`E` with no exponent digits
        }
    }
    i == bytes.len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn non_trivia_kinds(text: &str) -> Vec<SyntaxKind> {
        lex_yaml(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| t.kind)
            .collect()
    }

    fn token_texts(text: &str) -> Vec<&str> {
        lex_yaml(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| &text[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "# header\nname: ozone\nlist:\n  - a\n  - b\nflow: [1, 2, 3]\n";
        let stream = lex_yaml(text);
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
        let ks = non_trivia_kinds("name: ozone\n");
        assert_eq!(
            ks,
            vec![
                YamlKind::Key.into(),
                YamlKind::Operator.into(),
                YamlKind::PlainScalar.into(),
            ]
        );
    }

    #[test]
    fn quoted_key_and_string_value() {
        let ks = non_trivia_kinds(r#""a key": 'a value'"#);
        assert_eq!(
            ks,
            vec![
                YamlKind::Key.into(),
                YamlKind::Operator.into(),
                YamlKind::String.into(),
            ]
        );
    }

    #[test]
    fn key_with_no_value() {
        let ks = non_trivia_kinds("jobs:\n");
        assert_eq!(ks, vec![YamlKind::Key.into(), YamlKind::Operator.into()]);
    }

    #[test]
    fn block_sequence() {
        let ks = non_trivia_kinds("list:\n  - a\n  - b\n");
        assert_eq!(
            ks,
            vec![
                YamlKind::Key.into(),
                YamlKind::Operator.into(),
                YamlKind::Operator.into(),
                YamlKind::PlainScalar.into(),
                YamlKind::Operator.into(),
                YamlKind::PlainScalar.into(),
            ]
        );
    }

    #[test]
    fn negative_number_is_not_a_sequence_marker() {
        let ks = non_trivia_kinds("n: -5\n");
        assert_eq!(
            ks,
            vec![
                YamlKind::Key.into(),
                YamlKind::Operator.into(),
                YamlKind::NumberLit.into(),
            ]
        );
        let txts = token_texts("n: -5\n");
        assert_eq!(txts[2], "-5");
    }

    #[test]
    fn flow_mapping_and_sequence() {
        let ks = non_trivia_kinds("a: {b: 1, c: [1, 2]}\n");
        assert!(ks.contains(&YamlKind::Punctuation.into()));
        assert!(ks.iter().filter(|&&k| k == YamlKind::Key.into()).count() >= 2);
        assert!(ks.contains(&YamlKind::NumberLit.into()));
    }

    #[test]
    fn booleans_and_null() {
        let ks = non_trivia_kinds("a: true\nb: false\nc: null\nd: ~\n");
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == YamlKind::BoolLit.into())
                .count(),
            2
        );
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == YamlKind::NullLit.into())
                .count(),
            2
        );
    }

    #[test]
    fn numbers_int_float_hex_inf_nan() {
        let ks = non_trivia_kinds("a: 42\nb: 3.14\nc: 0xFF\nd: -.inf\ne: .nan\nf: 1e10\n");
        assert_eq!(
            ks.iter()
                .filter(|&&k| k == YamlKind::NumberLit.into())
                .count(),
            6
        );
    }

    #[test]
    fn comment_to_end_of_line() {
        let txts = token_texts("# top comment\nkey: value # trailing\n");
        assert_eq!(txts[0], "# top comment");
        assert!(txts.contains(&"# trailing"));
    }

    #[test]
    fn hash_in_value_without_leading_space_is_not_a_comment() {
        let txts = token_texts("key: a#b\n");
        assert_eq!(txts[2], "a#b");
    }

    #[test]
    fn document_markers() {
        let ks = non_trivia_kinds("---\nname: ozone\n...\n");
        assert_eq!(ks[0], YamlKind::DocumentMarker.into());
        assert_eq!(*ks.last().unwrap(), YamlKind::DocumentMarker.into());
    }

    #[test]
    fn anchor_alias_and_tag() {
        let ks = non_trivia_kinds("base: &anchor !!str value\nref: *anchor\n");
        assert!(ks.contains(&YamlKind::Anchor.into()));
        assert!(ks.contains(&YamlKind::Tag.into()));
        assert!(ks.contains(&YamlKind::Alias.into()));
    }

    #[test]
    fn block_scalar_indicators() {
        let txts = token_texts("a: |\n  line1\n  line2\nb: >-\n  folded\n");
        assert_eq!(txts[2], "|");
        assert!(txts.contains(&">-"));
    }

    #[test]
    fn url_colon_is_not_a_value_indicator() {
        let txts = token_texts("url: http://example.com\n");
        assert_eq!(txts[2], "http://example.com");
    }
}
