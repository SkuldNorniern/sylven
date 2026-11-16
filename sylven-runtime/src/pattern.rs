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
    /// A name: a letter of any script, `_` or one of `start`, then letters,
    /// digits of any script, `_` or one of `more`. From `[a-zA-Z_][a-zA-Z0-9_']*`
    /// the extras are what the classes list beyond ranges and `_`: here `'`.
    Word { start: Vec<char>, more: Vec<char> },
    /// One or more ASCII digits: `[0-9]+`.
    Digits,
    /// Floating-point literal: `[0-9]+\.[0-9]+`.
    Float,
    /// Quoted string with backslash escapes (delimiter = `"`, `'` or a
    /// backtick). One that leaves `\n` out of its class stops at a line end.
    QuotedStr { delim: u8, single_line: bool },
    /// A sigil then an identifier, like `$name` in shell, PHP or PowerShell.
    Sigil(u8),
    /// An identifier that may hold `-`, like CSS properties: `font-size`.
    IdentDash,
    /// `0x` then hex digits.
    Hex,
    /// A lifetime or label: `'a`, but not the char `'a'`.
    Lifetime,
    /// ASCII whitespace run: `[ \t\r\n]+`.
    Whitespace,
    /// A line comment: everything from `prefix` to the next `\n` (exclusive).
    LineComment(String),
    /// A block comment delimited by `open` … `close`.
    BlockComment(String, String),
}

/// how much of `rest` a number's tail takes: an exponent (`e-3`) and then a
/// suffix of letters and digits (`u8`, `f32`, `UL`).
fn suffix(rest: &[u8]) -> usize {
    let mut i = 0;
    if matches!(rest.first(), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(rest.get(1), Some(b'+' | b'-')));
        let digits = rest[1 + sign..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if digits > 0 {
            i = 1 + sign + digits;
        }
    }
    if rest.get(i).is_some_and(u8::is_ascii_alphabetic) {
        i += rest[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
            .count();
    }
    i
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
                        // Keyword must not be a prefix of a longer name, in
                        // any script
                        let mut after = src[kw.len()..].chars();
                        let next = after.next();
                        // `auto-fit` is one name, not `auto` then `-fit`
                        let dashed = next == Some('-')
                            && after.next().is_some_and(|c| c.is_alphabetic() || c == '_');
                        let joined = next.is_some_and(|c| c.is_alphanumeric() || c == '_');
                        if !joined && !dashed {
                            return Some(kw.len());
                        }
                    }
                }
                None
            }
            Pattern::Word { start, more } => {
                let mut chars = src.char_indices();
                let (_, first) = chars.next()?;
                if !(first.is_alphabetic() || first == '_' || start.contains(&first)) {
                    return None;
                }
                let mut end = first.len_utf8();
                for (i, c) in chars {
                    if c.is_alphanumeric() || c == '_' || more.contains(&c) {
                        end = i + c.len_utf8();
                    } else {
                        break;
                    }
                }
                Some(end)
            }
            Pattern::Digits => {
                if !b.first().is_some_and(u8::is_ascii_digit) {
                    return None;
                }
                // `1_000`, then a suffix or exponent run on: `10u8`, `1e9`, `10UL`
                let len = b
                    .iter()
                    .take_while(|c| c.is_ascii_digit() || **c == b'_')
                    .count();
                Some(len + suffix(&b[len..]))
            }
            Pattern::Lifetime => {
                if b.first() != Some(&b'\'') {
                    return None;
                }
                let first = *b.get(1)?;
                if !first.is_ascii_alphabetic() && first != b'_' {
                    return None;
                }
                let len = 1 + b[1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                    .count();
                // `'a'` is a char
                (b.get(len) != Some(&b'\'')).then_some(len)
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
                    .take_while(|c| c.is_ascii_digit() || **c == b'_')
                    .count();
                if frac_part == 0 || !b[int_part + 1].is_ascii_digit() {
                    return None;
                }
                let len = int_part + 1 + frac_part;
                Some(len + suffix(&b[len..]))
            }
            Pattern::QuotedStr { delim, single_line } => {
                if b.first() != Some(delim) {
                    return None;
                }
                let mut i = 1;
                while i < b.len() {
                    if *single_line && b[i] == b'\n' {
                        // unterminated on its line: the line end stays out
                        return Some(i);
                    }
                    if b[i] == b'\\' {
                        // skip the escaped char; a backslash at the very
                        // end has nothing to skip
                        i = (i + 2).min(b.len());
                    } else if &b[i] == delim {
                        i += 1; // include closing delimiter
                        return Some(i);
                    } else {
                        i += 1;
                    }
                }
                Some(i) // unterminated — consume to end
            }
            Pattern::Sigil(sigil) => {
                if b.first() != Some(sigil) {
                    return None;
                }
                let len = b[1..]
                    .iter()
                    .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
                    .count();
                (len > 0).then_some(1 + len)
            }
            Pattern::IdentDash => {
                let first = *b.first()?;
                let starts = first.is_ascii_alphabetic()
                    || first == b'_'
                    || (first == b'-'
                        && b.get(1)
                            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'-'));
                if !starts {
                    return None;
                }
                let len = 1 + b[1..]
                    .iter()
                    .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    .count();
                Some(len)
            }
            Pattern::Hex => {
                if !(src.starts_with("0x") || src.starts_with("0X")) {
                    return None;
                }
                let len = b[2..]
                    .iter()
                    .take_while(|c| c.is_ascii_hexdigit() || **c == b'_')
                    .count();
                (len > 0).then_some(2 + len)
            }
            Pattern::Whitespace => {
                let len: usize = src
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(char::len_utf8)
                    .sum();
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
    /// Common patterns are recognized; anything else becomes a plain `Word`
    /// as a safe fallback (it will be wrong, but won't panic).
    pub fn from_regex(pat: &str) -> Self {
        // Whitespace: \s+
        if pat.contains("\\s") {
            return Pattern::Whitespace;
        }
        // Quoted string: starts with `"` or `'` inside a character class or literally
        let single_line = pat.contains("\\n");
        if pat.starts_with('"') || pat.contains(r#"([^"\\]"#) {
            return Pattern::QuotedStr {
                delim: b'"',
                single_line,
            };
        }
        if pat.starts_with("'[a-zA-Z") {
            return Pattern::Lifetime;
        }
        if pat.starts_with('\'') || pat.contains("([^'\\\\]") {
            return Pattern::QuotedStr {
                delim: b'\'',
                single_line,
            };
        }
        if pat.starts_with('`') || pat.contains("([^`") {
            return Pattern::QuotedStr {
                delim: b'`',
                single_line,
            };
        }
        // `\$` then an identifier class
        if pat
            .strip_prefix("\\$")
            .is_some_and(|rest| rest.starts_with('['))
        {
            return Pattern::Sigil(b'$');
        }
        if pat.starts_with("@[") {
            return Pattern::Sigil(b'@');
        }
        if pat.starts_with("0x") {
            return Pattern::Hex;
        }
        if pat.starts_with("[a-zA-Z_-]") || pat.starts_with("[-a-zA-Z_]") {
            return Pattern::IdentDash;
        }
        // Float before Digits (more specific)
        if pat.contains("[0-9]") && pat.contains('.') {
            return Pattern::Float;
        }
        // Digits: [0-9]+
        if pat.starts_with("[0-9]") || pat.starts_with("[0-9a-fA-F]") {
            return Pattern::Digits;
        }
        // a name: its first class says how it starts, the next how it goes on
        let mut classes = classes(pat).into_iter();
        let start = classes.next().map(|c| extras(&c)).unwrap_or_default();
        let more = classes.next().map(|c| extras(&c)).unwrap_or_default();
        Pattern::Word { start, more }
    }
}

/// the `[...]` classes of a regex, in order, without their brackets.
fn classes(pat: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = pat.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' => {
                let mut class = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            class.push('\\');
                            if let Some(e) = chars.next() {
                                class.push(e);
                            }
                        }
                        ']' => break,
                        c => class.push(c),
                    }
                }
                out.push(class);
            }
            _ => {}
        }
    }
    out
}

/// what a class lists beyond ascii ranges and `_`: `a-zA-Z0-9_'!?` gives
/// `'`, `!`, `?`. a negated class gives nothing.
fn extras(class: &str) -> Vec<char> {
    if class.starts_with('^') {
        return Vec::new();
    }
    let chars: Vec<char> = class.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some() {
            // a range like a-z: letters and digits already count
            i += 3;
            continue;
        }
        let c = if c == '\\' {
            i += 1;
            match chars.get(i) {
                Some(e) => *e,
                None => break,
            }
        } else {
            c
        };
        if c != '_' && !c.is_alphanumeric() && !out.contains(&c) {
            out.push(c);
        }
        i += 1;
    }
    out
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
        assert_eq!(kw.match_at("let-x"), None, "a dashed name");
        assert_eq!(kw.match_at("let-1"), Some(3));
        assert_eq!(kw.match_at("let--"), Some(3));
    }

    #[test]
    fn ident_like() {
        let word = Pattern::from_regex("[a-zA-Z_][a-zA-Z0-9_]*");
        assert_eq!(word.match_at("hello world"), Some(5));
        assert_eq!(word.match_at("_foo bar"), Some(4));
        assert_eq!(word.match_at("123"), None);
        assert_eq!(
            word.match_at("h₁ x"),
            Some(4),
            "subscript digits go on a name"
        );
        assert_eq!(word.match_at("αβ x"), Some(4), "letters of any script");
        assert_eq!(word.match_at("h' x"), Some(1), "no prime unless listed");
        let lean = Pattern::from_regex("[a-zA-Z_][a-zA-Z0-9_'!?]*");
        assert_eq!(lean.match_at("ih' x"), Some(3));
        assert_eq!(lean.match_at("get! x"), Some(4));
        assert_eq!(lean.match_at("simp? x"), Some(5));
        assert_eq!(lean.match_at("'a'"), None);
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
        let p = Pattern::QuotedStr {
            delim: b'"',
            single_line: false,
        };
        assert_eq!(p.match_at(r#""hello" rest"#), Some(7));
        assert_eq!(p.match_at(r#""a\"b" rest"#), Some(6));
        assert_eq!(p.match_at("'nope'"), None);
        assert_eq!(p.match_at("\"a\nb\""), Some(5), "multi-line unless told");
        let char_lit = Pattern::from_regex("'([^'\\\\\\n]|\\\\.)'");
        assert_eq!(char_lit.match_at("'a' x"), Some(3));
        assert_eq!(
            char_lit.match_at("' open\nnext"),
            Some(6),
            "stops at the line end"
        );
    }

    #[test]
    fn keywords_and_whitespace_past_ascii() {
        let kw = Pattern::Keywords(vec!["fun".into(), "λ".into()]);
        assert_eq!(kw.match_at("funα"), None, "a longer name");
        assert_eq!(kw.match_at("λ x"), Some(2));
        assert_eq!(Pattern::Literal("→".into()).match_at("→ B"), Some(3));
        assert_eq!(Pattern::Whitespace.match_at("　 x"), Some(5));
    }

    #[test]
    fn numbers_take_their_tails_and_lifetimes_stop_short_of_chars() {
        assert_eq!(Pattern::Digits.match_at("1_000u32;"), Some(8));
        assert_eq!(Pattern::Digits.match_at("1e-9 x"), Some(4));
        assert_eq!(Pattern::Digits.match_at("10UL)"), Some(4));
        assert_eq!(Pattern::Digits.match_at("0..5"), Some(1));
        assert_eq!(Pattern::Float.match_at("1.5f32 x"), Some(6));
        assert_eq!(Pattern::Float.match_at("2.0e10,"), Some(6));
        assert_eq!(Pattern::Float.match_at("1._5"), None);
        assert_eq!(Pattern::Lifetime.match_at("'a>"), Some(2));
        assert_eq!(Pattern::Lifetime.match_at("'static str"), Some(7));
        assert_eq!(Pattern::Lifetime.match_at("'a'"), None, "a char");
        assert!(matches!(
            Pattern::from_regex("'[a-zA-Z_][a-zA-Z0-9_]*"),
            Pattern::Lifetime
        ));
    }

    #[test]
    fn sigils_dashes_hex_and_backticks() {
        assert_eq!(Pattern::Sigil(b'$').match_at("$HOME/x"), Some(5));
        assert_eq!(Pattern::Sigil(b'$').match_at("$ x"), None);
        assert_eq!(Pattern::IdentDash.match_at("font-size: 1"), Some(9));
        assert_eq!(Pattern::IdentDash.match_at("--var x"), Some(5));
        assert_eq!(Pattern::IdentDash.match_at("- 1"), None);
        assert_eq!(Pattern::Hex.match_at("0xFF_00;"), Some(7));
        assert_eq!(Pattern::Hex.match_at("0x;"), None);
        let tick = Pattern::QuotedStr {
            delim: b'`',
            single_line: false,
        };
        assert_eq!(tick.match_at("`a${b}` x"), Some(7));
        assert!(matches!(
            Pattern::from_regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*"),
            Pattern::Sigil(b'$')
        ));
        assert!(matches!(
            Pattern::from_regex("[a-zA-Z_-][a-zA-Z0-9_-]*"),
            Pattern::IdentDash
        ));
        assert!(matches!(
            Pattern::from_regex("0x[0-9a-fA-F_]+"),
            Pattern::Hex
        ));
        assert!(matches!(
            Pattern::from_regex("`([^`\\\\]|\\\\.)*`"),
            Pattern::QuotedStr { delim: b'`', .. }
        ));
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
            Pattern::Word { start, more } if start.is_empty() && more.is_empty()
        ));
        assert!(matches!(Pattern::from_regex("[0-9]+"), Pattern::Digits));
    }
}
