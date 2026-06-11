//! Hand-written Rust lexer for Sylven.
//!
//! Produces a lossless [`TokenStream`]: concatenating every token's source
//! slice in order reproduces the original text exactly. Unknown bytes become
//! single [`SyntaxKind::ERROR`] tokens so the stream always covers the whole
//! source.
//!
//! Works on the **full document** — no per-line state needed. Nested block
//! comments, multi-line raw strings, and `'`-disambiguation (lifetime vs char
//! literal) are all handled naturally at this level.

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

// ---------------------------------------------------------------------------
// RustKind
// ---------------------------------------------------------------------------

/// Token categories produced by [`lex_rust`].
///
/// Tree-node kinds are not defined here — the lexer only produces a flat token
/// stream. A parser would add node kinds later (Stage 2+).
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustKind {
    // --- literal tokens ---
    /// A plain identifier or a primitive type that isn't a keyword.
    Ident,
    /// A keyword: `fn`, `let`, `struct`, `impl`, `pub`, `use`, etc.
    Keyword,
    /// A control-flow keyword: `if`, `else`, `while`, `for`, `loop`, `match`,
    /// `return`, `break`, `continue`, `yield`.
    KeywordControl,
    /// A primitive type name: `bool`, `i32`, `str`, `char`, …
    PrimitiveType,
    /// A well-known stdlib type in PascalCase: `String`, `Vec`, `Option`, …
    StdType,
    /// `true` or `false`.
    BoolLit,
    /// A lifetime: `'a`, `'static`.
    Lifetime,
    /// A double-quoted string, raw string, byte string, or byte raw string.
    StringLit,
    /// A single-quoted character literal.
    CharLit,
    /// An integer or floating-point numeric literal.
    NumberLit,
    /// A `//` or `///` or `//!` line comment (including the newline if present).
    LineComment,
    /// A `/* … */` block comment (possibly nested, possibly multi-line).
    BlockComment,
    /// An identifier immediately followed by `!`: `println!`, `vec!`, …
    /// The `!` is included in this token.
    MacroIdent,
    /// An `#[…]` or `#![…]` attribute span (the closing `]` is included).
    Attribute,
    /// An identifier immediately followed by `(`: a function-like name.
    /// The `(` is **not** included.
    FunctionIdent,
    /// A PascalCase identifier that is not a keyword or well-known type name
    /// (typically a struct / enum / trait / type-param name).
    PascalIdent,
    /// An operator character: `+`, `-`, `*`, `/`, `%`, `=`, `!`, `<`, `>`,
    /// `&`, `|`, `^`, `~`, `@`.
    Operator,
    /// A punctuation character that isn't an operator: `{`, `}`, `(`, `)`,
    /// `[`, `]`, `;`, `:`, `,`, `.`, `?`, `_`.
    Punctuation,
}

impl RustKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<RustKind> for SyntaxKind {
    fn from(k: RustKind) -> SyntaxKind {
        k.to_syntax()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lex `text` into a lossless [`TokenStream`].
pub fn lex_rust(text: &str) -> TokenStream {
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

    // Whitespace
    if b.is_ascii_whitespace() {
        while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        return SyntaxKind::WHITESPACE;
    }

    // Comments: `//` and `/* */`
    if b == b'/' {
        // Line comment
        if bytes.get(*pos + 1) == Some(&b'/') {
            while *pos < bytes.len() && bytes[*pos] != b'\n' {
                *pos += 1;
            }
            if *pos < bytes.len() {
                *pos += 1; // consume the newline
            }
            return RustKind::LineComment.into();
        }
        // Block comment (nested)
        if bytes.get(*pos + 1) == Some(&b'*') {
            *pos += 2;
            let mut depth: u32 = 1;
            while *pos < bytes.len() && depth > 0 {
                if bytes.get(*pos) == Some(&b'/') && bytes.get(*pos + 1) == Some(&b'*') {
                    depth += 1;
                    *pos += 2;
                } else if bytes.get(*pos) == Some(&b'*') && bytes.get(*pos + 1) == Some(&b'/') {
                    depth -= 1;
                    *pos += 2;
                } else {
                    *pos += 1;
                }
            }
            return RustKind::BlockComment.into();
        }
        // `/` as operator
        *pos += 1;
        return RustKind::Operator.into();
    }

    // Attribute: #[ or #![
    if b == b'#'
        && (bytes.get(*pos + 1) == Some(&b'[')
            || (bytes.get(*pos + 1) == Some(&b'!') && bytes.get(*pos + 2) == Some(&b'[')))
    {
        *pos += 1;
        // Advance to the opening `[`
        while *pos < bytes.len() && bytes[*pos] != b'[' {
            *pos += 1;
        }
        // Consume nested brackets
        let mut depth = 0usize;
        while *pos < bytes.len() {
            match bytes[*pos] {
                b'[' => {
                    depth += 1;
                    *pos += 1;
                }
                b']' => {
                    depth -= 1;
                    *pos += 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {
                    *pos += 1;
                }
            }
        }
        return RustKind::Attribute.into();
    }

    // Raw string / raw byte string: r"", r#""#, b"", br""
    if (b == b'r' || b == b'b')
        && (bytes.get(*pos + 1) == Some(&b'"')
            || bytes.get(*pos + 1) == Some(&b'#')
            || (b == b'b'
                && bytes.get(*pos + 1) == Some(&b'r')
                && (bytes.get(*pos + 2) == Some(&b'"') || bytes.get(*pos + 2) == Some(&b'#'))))
    {
        return lex_string_like(bytes, text, pos);
    }

    // Normal double-quoted string
    if b == b'"' {
        return lex_double_quoted(bytes, pos);
    }

    // Single-quoted: lifetime or char literal
    if b == b'\'' {
        return lex_quote(bytes, pos);
    }

    // Number
    if b.is_ascii_digit() {
        return lex_number(bytes, pos);
    }

    // Identifier / keyword / macro
    if b.is_ascii_alphabetic() || b == b'_' {
        let start = *pos;
        while *pos < bytes.len()
            && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_')
        {
            *pos += 1;
        }
        let word = &text[start..*pos];

        // Macro: ident! (include the `!`)
        if bytes.get(*pos) == Some(&b'!') {
            *pos += 1;
            return RustKind::MacroIdent.into();
        }

        // Function call: ident( — no advancement, just classification
        let is_call = bytes.get(*pos) == Some(&b'(');

        return classify_ident(word, is_call).into();
    }

    // Operators
    if matches!(
        b,
        b'+' | b'-' | b'*' | b'%' | b'=' | b'!' | b'<' | b'>' | b'&' | b'|' | b'^' | b'~'
            | b'@'
    ) {
        *pos += 1;
        return RustKind::Operator.into();
    }

    // Punctuation
    if matches!(b, b'{' | b'}' | b'(' | b')' | b'[' | b']' | b';' | b':' | b',' | b'.' | b'?')
    {
        *pos += 1;
        return RustKind::Punctuation.into();
    }

    // Unknown / non-ASCII: emit one UTF-8 codepoint as ERROR
    let len = utf8_len(b).min(bytes.len() - *pos);
    *pos += len;
    SyntaxKind::ERROR
}

/// Lex a `"..."` double-quoted string (no raw prefix).
fn lex_double_quoted(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    *pos += 1; // opening `"`
    while *pos < bytes.len() {
        match bytes[*pos] {
            b'\\' => {
                *pos += 2; // escape sequence — skip one extra byte
            }
            b'"' => {
                *pos += 1;
                break;
            }
            _ => {
                *pos += 1;
            }
        }
    }
    RustKind::StringLit.into()
}

/// Lex `b"..."`, `r"..."`, `r#"..."#`, `br"..."`, `br#"..."#`.
fn lex_string_like(bytes: &[u8], _text: &str, pos: &mut usize) -> SyntaxKind {
    // Consume the prefix letters (`b`, `r`, `br`)
    while *pos < bytes.len() && (bytes[*pos] == b'b' || bytes[*pos] == b'r') {
        *pos += 1;
    }
    // Count hash characters before the opening `"`
    let hash_count = {
        let mut n = 0;
        while bytes.get(*pos + n) == Some(&b'#') {
            n += 1;
        }
        n
    };
    *pos += hash_count; // consume opening hashes

    if bytes.get(*pos) == Some(&b'"') {
        *pos += 1; // opening `"`

        if hash_count == 0 {
            // Simple b"..." or r"..." — same as double-quoted but no escape processing
            while *pos < bytes.len() {
                if bytes[*pos] == b'"' {
                    *pos += 1;
                    break;
                }
                *pos += 1;
            }
        } else {
            // Raw string r#"..."# — close is `"` followed by exactly `hash_count` `#`s
            'outer: while *pos < bytes.len() {
                if bytes[*pos] == b'"' {
                    let close_start = *pos;
                    *pos += 1;
                    let mut h = 0;
                    while bytes.get(*pos) == Some(&b'#') {
                        h += 1;
                        *pos += 1;
                    }
                    if h == hash_count {
                        break 'outer;
                    }
                    // Didn't match enough hashes — not the close delimiter; backtrack to after
                    // the `"` and keep scanning
                    let _ = close_start; // already advanced past the hashes; continue
                } else {
                    *pos += 1;
                }
            }
        }
    }
    RustKind::StringLit.into()
}

/// Lex `'...'`: lifetime or char literal.
fn lex_quote(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    *pos += 1; // consume `'`

    if *pos >= bytes.len() {
        return SyntaxKind::ERROR;
    }

    let next = bytes[*pos];

    // Escape char: '\n', '\t', '\\', '\'', '\u{…}', etc.
    if next == b'\\' {
        *pos += 1;
        while *pos < bytes.len() && bytes[*pos] != b'\'' {
            *pos += 1;
        }
        if *pos < bytes.len() {
            *pos += 1; // closing `'`
        }
        return RustKind::CharLit.into();
    }

    // Single ASCII char: 'x' — only if immediately followed by `'`
    if bytes.get(*pos + 1) == Some(&b'\'') && (next.is_ascii() || next >= 0x80) {
        let char_len = utf8_len(next).min(bytes.len() - *pos);
        *pos += char_len; // the char itself
        *pos += 1; // closing `'`
        return RustKind::CharLit.into();
    }

    // Lifetime: starts with alphabetic or `_`, not followed by `'`
    if next.is_ascii_alphabetic() || next == b'_' {
        while *pos < bytes.len()
            && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_')
        {
            *pos += 1;
        }
        // If the identifier is followed by `'` it's a char literal like 'ab' (invalid Rust,
        // but we shouldn't misclassify it as a lifetime).
        if bytes.get(*pos) == Some(&b'\'') {
            *pos += 1;
            return RustKind::CharLit.into();
        }
        return RustKind::Lifetime.into();
    }

    // Non-ASCII or other char literal
    let char_len = utf8_len(next).min(bytes.len() - *pos);
    *pos += char_len;
    if bytes.get(*pos) == Some(&b'\'') {
        *pos += 1;
    }
    RustKind::CharLit.into()
}

/// Lex a numeric literal.
fn lex_number(bytes: &[u8], pos: &mut usize) -> SyntaxKind {
    // Hex / binary / octal prefix
    if bytes[*pos] == b'0' {
        match bytes.get(*pos + 1) {
            Some(&b'x') | Some(&b'X') => {
                *pos += 2;
                while *pos < bytes.len()
                    && (bytes[*pos].is_ascii_hexdigit() || bytes[*pos] == b'_')
                {
                    *pos += 1;
                }
                // Optional type suffix
                lex_num_suffix(bytes, pos);
                return RustKind::NumberLit.into();
            }
            Some(&b'b') | Some(&b'B') => {
                *pos += 2;
                while *pos < bytes.len() && (bytes[*pos] == b'0' || bytes[*pos] == b'1' || bytes[*pos] == b'_') {
                    *pos += 1;
                }
                lex_num_suffix(bytes, pos);
                return RustKind::NumberLit.into();
            }
            Some(&b'o') | Some(&b'O') => {
                *pos += 2;
                while *pos < bytes.len()
                    && (matches!(bytes[*pos], b'0'..=b'7') || bytes[*pos] == b'_')
                {
                    *pos += 1;
                }
                lex_num_suffix(bytes, pos);
                return RustKind::NumberLit.into();
            }
            _ => {}
        }
    }
    // Decimal integer or float
    while *pos < bytes.len() && (bytes[*pos].is_ascii_digit() || bytes[*pos] == b'_') {
        *pos += 1;
    }
    // Optional fractional part / exponent
    if bytes.get(*pos) == Some(&b'.') && bytes.get(*pos + 1).map_or(false, |b| b.is_ascii_digit()) {
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
    lex_num_suffix(bytes, pos);
    RustKind::NumberLit.into()
}

/// Consume an optional type suffix after a number literal: `u32`, `i64`, `f64`, etc.
fn lex_num_suffix(bytes: &[u8], pos: &mut usize) {
    if *pos < bytes.len() && bytes[*pos].is_ascii_alphabetic() {
        while *pos < bytes.len() && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_') {
            *pos += 1;
        }
    }
}

/// Map an identifier string to its [`RustKind`].
fn classify_ident(word: &str, is_call: bool) -> RustKind {
    match word {
        "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue" | "return"
        | "yield" => RustKind::KeywordControl,

        "fn" | "let" | "mut" | "const" | "static" | "struct" | "enum" | "union" | "trait"
        | "impl" | "type" | "where" | "pub" | "use" | "mod" | "extern" | "crate" | "super"
        | "self" | "Self" | "in" | "as" | "move" | "async" | "await" | "dyn" | "unsafe"
        | "ref" | "box" | "pub(crate)" => RustKind::Keyword,

        "bool" | "char" | "str" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8"
        | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64" => RustKind::PrimitiveType,

        "String" | "Vec" | "Option" | "Result" | "Box" | "Rc" | "Arc" | "HashMap" | "HashSet"
        | "BTreeMap" | "BTreeSet" | "Mutex" | "RwLock" | "PathBuf" | "Path" | "Cow" | "Pin"
        | "Error" => RustKind::StdType,

        "true" | "false" => RustKind::BoolLit,

        _ => {
            if is_call {
                return RustKind::FunctionIdent;
            }
            // PascalCase → type-like ident
            if word.len() > 1 && word.chars().next().map_or(false, |c| c.is_uppercase()) {
                RustKind::PascalIdent
            } else {
                RustKind::Ident
            }
        }
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

    fn kinds(text: &str) -> Vec<SyntaxKind> {
        lex_rust(text)
            .as_slice()
            .iter()
            .map(|t| t.kind)
            .collect()
    }

    fn non_trivia_kinds(text: &str) -> Vec<SyntaxKind> {
        lex_rust(text)
            .as_slice()
            .iter()
            .filter(|t| !t.is_trivia() && t.kind != SyntaxKind::EOF)
            .map(|t| t.kind)
            .collect()
    }

    fn token_texts<'a>(text: &'a str) -> Vec<&'a str> {
        lex_rust(text)
            .as_slice()
            .iter()
            .filter(|t| t.kind != SyntaxKind::EOF)
            .map(|t| &text[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "fn main() {\n    println!(\"hi\");\n}\n";
        let stream = lex_rust(text);
        let mut rebuilt = String::new();
        for tok in stream.as_slice() {
            if tok.kind != SyntaxKind::EOF {
                rebuilt.push_str(&text[tok.range.start().to_usize()..tok.range.end().to_usize()]);
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn keywords_and_control_flow() {
        let k = RustKind::Keyword.to_syntax();
        let c = RustKind::KeywordControl.to_syntax();
        let ks = non_trivia_kinds("fn if let");
        assert_eq!(ks, vec![k, c, k]);
    }

    #[test]
    fn string_literal() {
        let ks = non_trivia_kinds(r#""hello world""#);
        assert_eq!(ks, vec![RustKind::StringLit.into()]);
    }

    #[test]
    fn raw_string_literal() {
        let ks = non_trivia_kinds(r###"r#"raw "string" here"#"###);
        assert_eq!(ks, vec![RustKind::StringLit.into()]);
    }

    #[test]
    fn byte_string_literal() {
        let ks = non_trivia_kinds(r#"b"bytes""#);
        assert_eq!(ks, vec![RustKind::StringLit.into()]);
    }

    #[test]
    fn char_literal() {
        let ks = non_trivia_kinds("'a'");
        assert_eq!(ks, vec![RustKind::CharLit.into()]);
    }

    #[test]
    fn escape_char_literal() {
        let ks = non_trivia_kinds(r"'\n'");
        assert_eq!(ks, vec![RustKind::CharLit.into()]);
    }

    #[test]
    fn lifetime() {
        let ks = non_trivia_kinds("'a 'static");
        assert_eq!(ks, vec![RustKind::Lifetime.into(), RustKind::Lifetime.into()]);
    }

    #[test]
    fn numeric_literals() {
        let ks = non_trivia_kinds("42 3.14 0xFF 0b1010 0o77 1_000_u64");
        assert!(ks.iter().all(|&k| k == RustKind::NumberLit.into()));
        assert_eq!(ks.len(), 6);
    }

    #[test]
    fn line_comment() {
        let ks = non_trivia_kinds("// this is a comment\n");
        assert_eq!(ks, vec![RustKind::LineComment.into()]);
    }

    #[test]
    fn block_comment_nested() {
        let ks = non_trivia_kinds("/* outer /* inner */ still outer */");
        assert_eq!(ks, vec![RustKind::BlockComment.into()]);
    }

    #[test]
    fn macro_ident() {
        let ks = non_trivia_kinds("println! vec!");
        assert_eq!(ks, vec![RustKind::MacroIdent.into(), RustKind::MacroIdent.into()]);
    }

    #[test]
    fn attribute() {
        let ks = non_trivia_kinds("#[derive(Debug)]");
        assert_eq!(ks, vec![RustKind::Attribute.into()]);
    }

    #[test]
    fn inner_attribute() {
        let ks = non_trivia_kinds("#![allow(dead_code)]");
        assert_eq!(ks, vec![RustKind::Attribute.into()]);
    }

    #[test]
    fn function_ident() {
        let ks = non_trivia_kinds("foo(");
        // `foo` is FunctionIdent, `(` is Punctuation
        assert_eq!(ks[0], RustKind::FunctionIdent.into());
        assert_eq!(ks[1], RustKind::Punctuation.into());
    }

    #[test]
    fn primitive_and_std_types() {
        let ks = non_trivia_kinds("bool i32 String Vec");
        assert_eq!(
            ks,
            vec![
                RustKind::PrimitiveType.into(),
                RustKind::PrimitiveType.into(),
                RustKind::StdType.into(),
                RustKind::StdType.into(),
            ]
        );
    }

    #[test]
    fn multiline_block_comment_tokens() {
        let text = "/* line1\nline2\n*/done";
        let txts = token_texts(text);
        assert_eq!(txts[0], "/* line1\nline2\n*/");
        assert_eq!(txts[1], "done");
    }

    #[test]
    fn block_comment_unclosed() {
        // Unclosed block comment: lexer consumes everything to EOF
        let ks = non_trivia_kinds("/* unclosed");
        assert_eq!(ks, vec![RustKind::BlockComment.into()]);
    }
}
