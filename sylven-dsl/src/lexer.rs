/// Token kinds for the `.sylven` DSL surface syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TK {
    /// Identifier or keyword: `[a-zA-Z_][a-zA-Z0-9_\.]*`
    Word,
    /// Quoted string (content stored, delimiters stripped): `"..."`
    Str,
    /// Regex pattern (content stored, slashes stripped): `/pat/`
    Regex,
    /// Decimal integer literal: `[0-9]+`
    Number,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    /// `->` arrow used in highlight/symbols/fold rules.
    Arrow,
    Colon,
    Comma,
    Invalid,
    Eof,
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub kind: TK,
    /// Text content (for Str/Regex: stripped of delimiters; for others: raw text).
    pub text: String,
    pub offset: usize,
}

/// Tokenise a `.sylven` source string. Whitespace and `#`-comments are silently
/// dropped. The last token is always `TK::Eof`.
pub(crate) fn lex(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Skip `#` line comments.
        if bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        let start = i;
        match bytes[i] {
            b'{' => {
                out.push(tok(TK::LBrace, "{", start));
                i += 1;
            }
            b'}' => {
                out.push(tok(TK::RBrace, "}", start));
                i += 1;
            }
            b'[' => {
                out.push(tok(TK::LBracket, "[", start));
                i += 1;
            }
            b']' => {
                out.push(tok(TK::RBracket, "]", start));
                i += 1;
            }
            b':' => {
                out.push(tok(TK::Colon, ":", start));
                i += 1;
            }
            b',' => {
                out.push(tok(TK::Comma, ",", start));
                i += 1;
            }
            // `->` arrow; bare `-` becomes Invalid.
            b'-' => {
                if bytes.get(i + 1) == Some(&b'>') {
                    out.push(tok(TK::Arrow, "->", start));
                    i += 2;
                } else {
                    out.push(tok(TK::Invalid, "-", start));
                    i += 1;
                }
            }
            // Quoted string.
            b'"' => {
                i += 1;
                let mut s = String::new();
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                        if i < bytes.len() {
                            match bytes[i] {
                                b'"' => s.push('"'),
                                b'\\' => s.push('\\'),
                                b'n' => s.push('\n'),
                                b't' => s.push('\t'),
                                c => {
                                    s.push('\\');
                                    s.push(c as char);
                                }
                            }
                            i += 1;
                        }
                    } else {
                        s.push(bytes[i] as char);
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1; // closing `"`
                }
                out.push(tok(TK::Str, &s, start));
            }
            // Regex `/pat/`.
            b'/' => {
                i += 1;
                let mut s = String::new();
                while i < bytes.len() && bytes[i] != b'/' && bytes[i] != b'\n' {
                    s.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'/' {
                    i += 1; // closing `/`
                }
                out.push(tok(TK::Regex, &s, start));
            }
            // Decimal integer.
            b if b.is_ascii_digit() => {
                let mut n = String::new();
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    n.push(bytes[i] as char);
                    i += 1;
                }
                out.push(tok(TK::Number, &n, start));
            }
            // Word: `[a-zA-Z_][a-zA-Z0-9_\.]*` — dots allowed for names like
            // `comment.line`, `int.number`, `FnDecl.name`.
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let mut w = String::new();
                while i < bytes.len() {
                    let c = bytes[i];
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
                        w.push(c as char);
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(tok(TK::Word, &w, start));
            }
            _ => {
                out.push(tok(TK::Invalid, &(bytes[i] as char).to_string(), start));
                i += 1;
            }
        }
    }

    out.push(tok(TK::Eof, "", source.len()));
    out
}

fn tok(kind: TK, text: &str, offset: usize) -> Token {
    Token {
        kind,
        text: text.to_owned(),
        offset,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TK> {
        lex(src).iter().map(|t| t.kind).collect()
    }

    #[test]
    fn skips_whitespace_and_hash_comments() {
        let toks = lex("  # comment\nfoo");
        assert_eq!(toks[0].kind, TK::Word);
        assert_eq!(toks[0].text, "foo");
    }

    #[test]
    fn single_char_punctuation() {
        let ks = kinds("{ } [ ] : ,");
        assert_eq!(
            ks,
            vec![
                TK::LBrace,
                TK::RBrace,
                TK::LBracket,
                TK::RBracket,
                TK::Colon,
                TK::Comma,
                TK::Eof
            ]
        );
    }

    #[test]
    fn arrow_vs_invalid_minus() {
        let ks = kinds("-> -");
        assert_eq!(ks, vec![TK::Arrow, TK::Invalid, TK::Eof]);
    }

    #[test]
    fn quoted_string_strips_delimiters() {
        let toks = lex(r#""hello world""#);
        assert_eq!(toks[0].kind, TK::Str);
        assert_eq!(toks[0].text, "hello world");
    }

    #[test]
    fn quoted_string_escape_sequences() {
        let toks = lex(r#""a\"b\\c""#);
        assert_eq!(toks[0].text, "a\"b\\c");
    }

    #[test]
    fn regex_strips_slashes() {
        let toks = lex("/[a-z]+/");
        assert_eq!(toks[0].kind, TK::Regex);
        assert_eq!(toks[0].text, "[a-z]+");
    }

    #[test]
    fn word_allows_dots() {
        let toks = lex("comment.line int.number");
        assert_eq!(toks[0].text, "comment.line");
        assert_eq!(toks[1].text, "int.number");
    }

    #[test]
    fn number() {
        let toks = lex("42");
        assert_eq!(toks[0].kind, TK::Number);
        assert_eq!(toks[0].text, "42");
    }

    #[test]
    fn eof_always_last() {
        let toks = lex("foo");
        assert_eq!(toks.last().unwrap().kind, TK::Eof);
    }
}
