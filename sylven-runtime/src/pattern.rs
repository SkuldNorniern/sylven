/// How a runtime token is matched against source text.
///
/// Each `Pattern` can answer `match_at(src) -> Option<usize>` — returns the
/// length of the match starting at the beginning of `src`, or `None`.
/// Patterns are applied in declaration order; the first match wins.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// A fixed set of keyword strings. Must be checked before `IdentLike` so
    /// that `fn` is a keyword rather than an identifier.
    Keywords(Vec<String>),
    /// A single exact literal, e.g. `(`, `==`, `->`.
    Literal(String),
    /// C-style identifier: `[a-zA-Z_][a-zA-Z0-9_]*`.
    IdentLike,
    /// One or more ASCII digits: `[0-9]+`.
    Digits,
    /// Floating-point literal: `[0-9]+\.[0-9]+`.
    Float,
    /// Quoted string with backslash escapes (delimiter = `"` or `'`).
    QuotedStr(u8),
    /// ASCII whitespace run: `[ \t\r\n]+`.
    Whitespace,
    /// A line comment: everything from `prefix` to the next `\n` (exclusive).
    LineComment(String),
    /// A block comment delimited by `open` … `close`.
    BlockComment(String, String),
}

impl Pattern {
    /// Try to match at the start of `src`. Returns the byte length consumed,
    /// or `None` if the pattern does not match at this position.
    pub fn match_at(&self, src: &str) -> Option<usize> {
        let b = src.as_bytes();
        match self {
            Pattern::Literal(s) => src.starts_with(s.as_str()).then_some(s.len()),
            Pattern::Keywords(kws) => {
                for kw in kws {
                    if src.starts_with(kw.as_str()) {
                        // Keyword must not be a prefix of a longer ident.
                        let after = &b[kw.len()..];
                        let next = after.first().copied().unwrap_or(0);
                        if !next.is_ascii_alphanumeric() && next != b'_' {
                            return Some(kw.len());
                        }
                    }
                }
                None
            }
            Pattern::IdentLike => {
                let first = *b.first()?;
                if !first.is_ascii_alphabetic() && first != b'_' {
                    return None;
                }
                let len = 1 + b[1..]
                    .iter()
                    .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
                    .count();
                Some(len)
            }
            Pattern::Digits => {
                let len = b.iter().take_while(|c| c.is_ascii_digit()).count();
                (len > 0).then_some(len)
            }
            Pattern::Float => {
                let int_part = b.iter().take_while(|c| c.is_ascii_digit()).count();
                if int_part == 0 {
                    return None;
                }
                if b.get(int_part) != Some(&b'.') {
                    return None;
                }
                let frac_part = b[int_part + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                if frac_part == 0 {
                    return None;
                }
                Some(int_part + 1 + frac_part)
            }
            Pattern::QuotedStr(delim) => {
                if b.first() != Some(delim) {
                    return None;
                }
                let mut i = 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2; // skip escaped char
                    } else if &b[i] == delim {
                        i += 1; // include closing delimiter
                        return Some(i);
                    } else {
                        i += 1;
                    }
                }
                Some(i) // unterminated — consume to end
            }
            Pattern::Whitespace => {
                let len = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
                (len > 0).then_some(len)
            }
            Pattern::LineComment(prefix) => {
                if !src.starts_with(prefix.as_str()) {
                    return None;
                }
                let len = b.iter().take_while(|&&c| c != b'\n').count();
                Some(len)
            }
            Pattern::BlockComment(open, close) => {
                if !src.starts_with(open.as_str()) {
                    return None;
                }
                match src[open.len()..].find(close.as_str()) {
                    Some(off) => Some(open.len() + off + close.len()),
                    None => Some(src.len()), // unterminated
                }
            }
        }
    }

    /// Heuristically infer a `Pattern` from a regex string stored in the DSL.
    /// Common patterns are recognized; anything else becomes `IdentLike` as a
    /// safe fallback (it will be wrong, but won't panic).
    pub fn from_regex(pat: &str) -> Self {
        // Whitespace: \s+
        if pat.contains("\\s") {
            return Pattern::Whitespace;
        }
        // Quoted string: starts with `"` or `'` inside a character class or literally
        if pat.starts_with('"') || pat.contains(r#"([^"\\]"#) {
            return Pattern::QuotedStr(b'"');
        }
        if pat.starts_with('\'') || pat.contains("([^'\\\\]") {
            return Pattern::QuotedStr(b'\'');
        }
        // Float before Digits (more specific)
        if pat.contains("[0-9]") && pat.contains('.') {
            return Pattern::Float;
        }
        // Digits: [0-9]+
        if pat.starts_with("[0-9]") || pat.starts_with("[0-9a-fA-F]") {
            return Pattern::Digits;
        }
        // Ident-like: starts with alpha/underscore class
        if pat.starts_with("[a-zA-Z") || pat.starts_with("[a-z") || pat.starts_with("[A-Z") {
            return Pattern::IdentLike;
        }
        // Default fallback
        Pattern::IdentLike
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_matches_exactly() {
        assert_eq!(Pattern::Literal("==".into()).match_at("== foo"), Some(2));
        assert_eq!(Pattern::Literal("==".into()).match_at("= foo"), None);
    }

    #[test]
    fn keywords_require_word_boundary() {
        let kw = Pattern::Keywords(vec!["fn".into(), "let".into()]);
        assert_eq!(kw.match_at("fn main"), Some(2));
        assert_eq!(kw.match_at("fnx"), None); // prefix of identifier
        assert_eq!(kw.match_at("let x"), Some(3));
    }

    #[test]
    fn ident_like() {
        assert_eq!(Pattern::IdentLike.match_at("hello world"), Some(5));
        assert_eq!(Pattern::IdentLike.match_at("_foo bar"), Some(4));
        assert_eq!(Pattern::IdentLike.match_at("123"), None);
    }

    #[test]
    fn digits() {
        assert_eq!(Pattern::Digits.match_at("42 rest"), Some(2));
        assert_eq!(Pattern::Digits.match_at("abc"), None);
    }

    #[test]
    fn float_requires_decimal() {
        assert_eq!(Pattern::Float.match_at("3.14 end"), Some(4));
        assert_eq!(Pattern::Float.match_at("3 end"), None);
        assert_eq!(Pattern::Float.match_at("3. end"), None); // no frac part
    }

    #[test]
    fn quoted_str() {
        let p = Pattern::QuotedStr(b'"');
        assert_eq!(p.match_at(r#""hello" rest"#), Some(7));
        assert_eq!(p.match_at(r#""a\"b" rest"#), Some(6));
        assert_eq!(p.match_at("'nope'"), None);
    }

    #[test]
    fn whitespace() {
        assert_eq!(Pattern::Whitespace.match_at("  \t\nfoo"), Some(4));
        assert_eq!(Pattern::Whitespace.match_at("abc"), None);
    }

    #[test]
    fn line_comment() {
        let p = Pattern::LineComment("//".into());
        assert_eq!(p.match_at("// hello\nafter"), Some(8));
        assert_eq!(p.match_at("/* not */"), None);
    }

    #[test]
    fn block_comment() {
        let p = Pattern::BlockComment("/*".into(), "*/".into());
        assert_eq!(p.match_at("/* hello */rest"), Some(11));
        assert_eq!(p.match_at("// line"), None);
    }

    #[test]
    fn from_regex_infers_common_patterns() {
        assert!(matches!(Pattern::from_regex(r"\s+"), Pattern::Whitespace));
        assert!(matches!(
            Pattern::from_regex("[a-zA-Z_][a-zA-Z0-9_]*"),
            Pattern::IdentLike
        ));
        assert!(matches!(Pattern::from_regex("[0-9]+"), Pattern::Digits));
    }
}
