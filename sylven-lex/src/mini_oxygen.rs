//! Lexer for "Mini-Oxygen", the small C/JS-like subset of the
//! [Oxygen](https://github.com/SkuldNorniern/ozone) scripting language used
//! as Sylven's Stage 1 proof-of-concept (see `plan.md` §14).
//!
//! This is a hand-written, table-free lexer. Later stages move per-language
//! lexers like this one to `sylven-langs`, generated from `sylven-dsl`.

use sylven_text::{TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

/// Mini-Oxygen's token and tree-node kinds.
///
/// Discriminants follow declaration order; [`MiniOxygenKind::to_syntax`]
/// shifts them past [`SyntaxKind::LANG_KIND_BASE`] so they never collide with
/// the kinds [`SyntaxKind`] reserves for itself.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniOxygenKind {
    // --- tokens ---
    Ident,
    IntNumber,
    String,
    FnKw,
    LetKw,
    IfKw,
    ElseKw,
    ReturnKw,
    WhileKw,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Eq,
    EqEq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,

    // --- tree nodes ---
    File,
    FnDecl,
    ParamList,
    Param,
    Block,
    LetStmt,
    ReturnStmt,
    IfStmt,
    ExprStmt,
    CallExpr,
    ArgList,
    BinaryExpr,
    PrefixExpr,
    ParenExpr,
    Name,
    NameRef,
    Literal,
    ErrorNode,
}

impl MiniOxygenKind {
    pub const fn to_syntax(self) -> SyntaxKind {
        SyntaxKind(SyntaxKind::LANG_KIND_BASE + self as u16)
    }
}

impl From<MiniOxygenKind> for SyntaxKind {
    fn from(kind: MiniOxygenKind) -> SyntaxKind {
        kind.to_syntax()
    }
}

fn keyword(ident: &str) -> Option<MiniOxygenKind> {
    Some(match ident {
        "fn" => MiniOxygenKind::FnKw,
        "let" => MiniOxygenKind::LetKw,
        "if" => MiniOxygenKind::IfKw,
        "else" => MiniOxygenKind::ElseKw,
        "return" => MiniOxygenKind::ReturnKw,
        "while" => MiniOxygenKind::WhileKw,
        _ => return None,
    })
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Length in bytes of the UTF-8 sequence starting with `first_byte`.
fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// Lex `text` into a lossless [`TokenStream`]: concatenating every token's
/// source slice (in order) reproduces `text` exactly. Never fails — bytes
/// the lexer doesn't recognize become single [`SyntaxKind::ERROR`] tokens, so
/// the parser always has a complete stream to recover from.
pub fn lex(text: &str) -> TokenStream {
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

fn lex_one(bytes: &[u8], text: &str, pos: &mut usize) -> SyntaxKind {
    let b = bytes[*pos];
    match b {
        b' ' | b'\t' | b'\r' | b'\n' => {
            while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\r' | b'\n') {
                *pos += 1;
            }
            SyntaxKind::WHITESPACE
        }
        b'/' if bytes.get(*pos + 1) == Some(&b'/') => {
            while *pos < bytes.len() && bytes[*pos] != b'\n' {
                *pos += 1;
            }
            SyntaxKind::COMMENT
        }
        b'0'..=b'9' => {
            while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
                *pos += 1;
            }
            MiniOxygenKind::IntNumber.into()
        }
        b'"' => {
            *pos += 1;
            while *pos < bytes.len() {
                match bytes[*pos] {
                    b'"' => {
                        *pos += 1;
                        break;
                    }
                    b'\\' if *pos + 1 < bytes.len() => *pos += 2,
                    b'\n' => break, // unterminated: stop at end of line
                    _ => *pos += 1,
                }
            }
            MiniOxygenKind::String.into()
        }
        b if is_ident_start(b) => {
            let start = *pos;
            while *pos < bytes.len() && is_ident_continue(bytes[*pos]) {
                *pos += 1;
            }
            let ident = &text[start..*pos];
            keyword(ident)
                .map(MiniOxygenKind::to_syntax)
                .unwrap_or_else(|| MiniOxygenKind::Ident.into())
        }
        b'=' if bytes.get(*pos + 1) == Some(&b'=') => {
            *pos += 2;
            MiniOxygenKind::EqEq.into()
        }
        b'!' if bytes.get(*pos + 1) == Some(&b'=') => {
            *pos += 2;
            MiniOxygenKind::Neq.into()
        }
        b'<' if bytes.get(*pos + 1) == Some(&b'=') => {
            *pos += 2;
            MiniOxygenKind::Le.into()
        }
        b'>' if bytes.get(*pos + 1) == Some(&b'=') => {
            *pos += 2;
            MiniOxygenKind::Ge.into()
        }
        b'(' => single(pos, MiniOxygenKind::LParen),
        b')' => single(pos, MiniOxygenKind::RParen),
        b'{' => single(pos, MiniOxygenKind::LBrace),
        b'}' => single(pos, MiniOxygenKind::RBrace),
        b',' => single(pos, MiniOxygenKind::Comma),
        b';' => single(pos, MiniOxygenKind::Semicolon),
        b'=' => single(pos, MiniOxygenKind::Eq),
        b'<' => single(pos, MiniOxygenKind::Lt),
        b'>' => single(pos, MiniOxygenKind::Gt),
        b'+' => single(pos, MiniOxygenKind::Plus),
        b'-' => single(pos, MiniOxygenKind::Minus),
        b'*' => single(pos, MiniOxygenKind::Star),
        b'/' => single(pos, MiniOxygenKind::Slash),
        b'!' => single(pos, MiniOxygenKind::Bang),
        _ => {
            let len = utf8_len(b).min(bytes.len() - *pos);
            *pos += len;
            SyntaxKind::ERROR
        }
    }
}

fn single(pos: &mut usize, kind: MiniOxygenKind) -> SyntaxKind {
    *pos += 1;
    kind.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<SyntaxKind> {
        lex(text).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lossless_round_trip() {
        let text = "fn fib(n) {\n    return n;\n}\n";
        let stream = lex(text);
        let mut rebuilt = String::new();
        for token in &stream {
            rebuilt.push_str(stream.text(*token, text));
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn keywords_and_punctuation() {
        let kinds = kinds("fn main() {}");
        assert_eq!(
            kinds,
            vec![
                MiniOxygenKind::FnKw.into(),
                SyntaxKind::WHITESPACE,
                MiniOxygenKind::Ident.into(),
                MiniOxygenKind::LParen.into(),
                MiniOxygenKind::RParen.into(),
                SyntaxKind::WHITESPACE,
                MiniOxygenKind::LBrace.into(),
                MiniOxygenKind::RBrace.into(),
                SyntaxKind::EOF,
            ]
        );
    }

    #[test]
    fn multi_char_operators() {
        let kinds = kinds("a == b != c <= d >= e");
        assert!(kinds.contains(&MiniOxygenKind::EqEq.into()));
        assert!(kinds.contains(&MiniOxygenKind::Neq.into()));
        assert!(kinds.contains(&MiniOxygenKind::Le.into()));
        assert!(kinds.contains(&MiniOxygenKind::Ge.into()));
    }

    #[test]
    fn string_and_comment() {
        let stream = lex("\"hi\" // trailing\n");
        let kinds: Vec<_> = stream.as_slice().iter().map(|t| t.kind).collect();
        assert_eq!(kinds[0], MiniOxygenKind::String.into());
        assert_eq!(kinds[2], SyntaxKind::COMMENT);
    }

    #[test]
    fn unknown_byte_becomes_error_token() {
        let kinds = kinds("a $ b");
        assert!(kinds.contains(&SyntaxKind::ERROR));
    }
}
