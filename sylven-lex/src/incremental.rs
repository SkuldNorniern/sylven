use sylven_text::{TextEdit, TextRange, TextSize};

use crate::{SyntaxKind, Token, TokenStream};

/// A cached token stream that can be updated incrementally after a single
/// [`TextEdit`], avoiding a full re-lex of the entire source on every
/// keystroke.
///
/// ## Algorithm (Stage 6 — line-boundary expansion)
///
/// 1. Expand the edit's dirty byte range to the nearest line boundaries in
///    the **old** source (backward to `\n`+1 or BOF; forward to `\n` or EOF).
/// 2. Partition the old token stream into three non-overlapping groups:
///    - **prefix** — tokens that end before the relex start: kept unchanged.
///    - **dirty** — tokens that overlap the expanded dirty range: discarded.
///    - **suffix** — tokens that start at or after the relex end: kept, but
///      their byte offsets are shifted by the delta introduced by the edit.
/// 3. Re-lex only `new_source[relex_start .. relex_end_new]` using the
///    supplied per-language lexer closure, then offset the resulting token
///    ranges by `relex_start`.
/// 4. Concatenate `prefix + middle + shifted_suffix + EOF`.
///
/// **Known limitation (to be addressed in a later stage):** Multi-line string
/// literals or block comments that start before the edited line will produce
/// incorrect token kinds if the expansion stops at a line boundary inside
/// them. Full incremental correctness for such constructs requires tracking
/// "lex mode" boundaries — planned for Stage 6.5+.
#[derive(Debug, Clone)]
pub struct IncrementalLexer {
    /// The full token stream for the current source, always ending with EOF.
    tokens: Vec<Token>,
    /// The source text that produced `tokens`.
    source: String,
}

impl IncrementalLexer {
    /// Perform a full lex of `source` using `lexer` and return a primed cache.
    /// `lexer` must produce a token stream ending with `SyntaxKind::EOF`.
    pub fn prime<F>(source: &str, lexer: F) -> Self
    where
        F: Fn(&str) -> TokenStream,
    {
        IncrementalLexer {
            tokens: lexer(source).into_iter().collect(),
            source: source.to_owned(),
        }
    }

    /// The current cached token stream (always ends with `SyntaxKind::EOF`).
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The source text that produced the current token stream.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Incrementally update the cache after a single `edit` is applied to
    /// produce `new_source`.
    ///
    /// `lexer` is the same per-language lexer passed to [`prime`](Self::prime).
    /// It will be called on a sub-slice of `new_source` rather than the full
    /// text, so it **must** produce correct tokens when given any well-formed
    /// prefix starting at a line boundary.
    pub fn relex<F>(&mut self, edit: &TextEdit, new_source: &str, lexer: F)
    where
        F: Fn(&str) -> TokenStream,
    {
        let edit_start = edit.delete.start().to_usize();
        let edit_end_old = edit.delete.end().to_usize();
        let edit_end_new = edit_start + edit.insert.len();

        // 1. Compute relex boundaries.
        let relex_start = line_start_before(&self.source, edit_start);
        let relex_end_old = line_end_after(&self.source, edit_end_old);
        let relex_end_new = line_end_after(new_source, edit_end_new);

        // 2. Partition old token stream (strip EOF first).
        let old_len = self.tokens.len();
        let non_eof = &self.tokens[..old_len.saturating_sub(1)];

        let relex_start_size = TextSize::from(relex_start as u32);
        let relex_end_old_size = TextSize::from(relex_end_old as u32);

        // prefix: tokens whose end <= relex_start
        let prefix_end = non_eof.partition_point(|t| t.range.end() <= relex_start_size);
        // suffix: tokens whose start >= relex_end_old
        let suffix_start = non_eof.partition_point(|t| t.range.start() < relex_end_old_size);

        let prefix = non_eof[..prefix_end].to_vec();
        let suffix_raw = non_eof[suffix_start..].to_vec();

        // 3. Re-lex the dirty slice and offset token ranges.
        let slice = &new_source[relex_start..relex_end_new];
        let delta: isize = edit_end_new as isize - edit_end_old as isize;

        let middle: Vec<Token> = lexer(slice)
            .into_iter()
            .filter(|t| t.kind != SyntaxKind::EOF)
            .map(|t| offset_token(t, relex_start as isize))
            .collect();

        // 4. Shift suffix offsets by the edit delta.
        let suffix: Vec<Token> = suffix_raw
            .into_iter()
            .map(|t| offset_token(t, delta))
            .collect();

        // 5. Rebuild with a fresh EOF at the new source end.
        let eof_pos = TextSize::from(new_source.len() as u32);
        let eof = Token::new(SyntaxKind::EOF, TextRange::new(eof_pos, eof_pos));

        self.tokens = Vec::with_capacity(prefix.len() + middle.len() + suffix.len() + 1);
        self.tokens.extend_from_slice(&prefix);
        self.tokens.extend(middle);
        self.tokens.extend(suffix);
        self.tokens.push(eof);
        self.source = new_source.to_owned();

        debug_assert!(
            self.tokens.last().unwrap().kind == SyntaxKind::EOF,
            "IncrementalLexer: stream must end with EOF"
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find the byte offset of the start of the line that contains `pos` in
/// `source` (i.e. just after the preceding `\n`, or 0 if none).
fn line_start_before(source: &str, pos: usize) -> usize {
    let clamp = pos.min(source.len());
    match source[..clamp].rfind('\n') {
        Some(nl) => nl + 1,
        None => 0,
    }
}

/// Find the byte offset just past the end of the line that contains `pos` in
/// `source` (i.e. just after the next `\n`, or `source.len()` if none).
fn line_end_after(source: &str, pos: usize) -> usize {
    let clamp = pos.min(source.len());
    match source[clamp..].find('\n') {
        Some(rel) => clamp + rel + 1,
        None => source.len(),
    }
}

/// Return a copy of `token` with its range shifted by `delta` bytes.
fn offset_token(t: Token, delta: isize) -> Token {
    let start = (t.range.start().to_usize() as isize + delta) as usize;
    let end = (t.range.end().to_usize() as isize + delta) as usize;
    Token::new(
        t.kind,
        TextRange::new(TextSize::from(start as u32), TextSize::from(end as u32)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::TextSize;

    use crate::mini_oxygen::lex;

    // ── helpers ────────────────────────────────────────────────────────────

    /// Collect the text slice for each non-EOF token from `source`.
    fn token_texts(cache: &IncrementalLexer) -> Vec<&str> {
        let src = cache.source();
        cache
            .tokens()
            .iter()
            .filter(|t| t.kind != SyntaxKind::EOF)
            .map(|t| &src[t.range.start().to_usize()..t.range.end().to_usize()])
            .collect()
    }

    /// Full-relex reference: prime on `source`, return token texts.
    fn full_texts(source: &str) -> Vec<String> {
        let stream = lex(source);
        stream
            .as_slice()
            .iter()
            .filter(|t| t.kind != SyntaxKind::EOF)
            .map(|t| source[t.range.start().to_usize()..t.range.end().to_usize()].to_owned())
            .collect()
    }

    fn make(source: &str) -> IncrementalLexer {
        IncrementalLexer::prime(source, lex)
    }

    fn apply_and_relex(cache: &mut IncrementalLexer, edit: TextEdit) -> String {
        let new_source = edit.apply(cache.source());
        cache.relex(&edit, &new_source, lex);
        new_source
    }

    // ── tests ──────────────────────────────────────────────────────────────

    #[test]
    fn prime_matches_full_lex() {
        let src = "let x = 42;";
        let cache = make(src);
        assert_eq!(token_texts(&cache), full_texts(src));
    }

    #[test]
    fn stream_ends_with_eof() {
        let cache = make("fn main() {}");
        assert_eq!(cache.tokens().last().unwrap().kind, SyntaxKind::EOF);
    }

    #[test]
    fn insert_single_char() {
        let src = "let x = 1;";
        let mut cache = make(src);
        // Insert '2' before '1' → "let x = 21;"
        let edit = TextEdit::insert(TextSize::from(8), "2");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn delete_single_char() {
        let src = "let xy = 1;";
        let mut cache = make(src);
        // Delete 'y' → "let x = 1;"
        let edit = TextEdit::delete(TextRange::new(TextSize::from(5), TextSize::from(6)));
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn replace_identifier() {
        let src = "let foo = 1;";
        let mut cache = make(src);
        // Replace 'foo' with 'bar'
        let edit = TextEdit::replace(TextRange::new(TextSize::from(4), TextSize::from(7)), "bar");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn insert_at_start() {
        let src = "let x = 1;";
        let mut cache = make(src);
        let edit = TextEdit::insert(TextSize::from(0), "// comment\n");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn insert_at_end() {
        let src = "let x = 1;";
        let mut cache = make(src);
        let edit = TextEdit::insert(TextSize::from(src.len() as u32), "\nlet y = 2;");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn insert_new_line_between_statements() {
        let src = "let x = 1;\nlet y = 2;";
        let mut cache = make(src);
        // Insert a blank line between the two statements.
        let edit = TextEdit::insert(TextSize::from(11), "let z = 3;\n");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn delete_entire_line() {
        let src = "let x = 1;\nlet y = 2;\nlet z = 3;";
        let mut cache = make(src);
        // Delete the second line including its newline.
        let edit = TextEdit::delete(TextRange::new(TextSize::from(11), TextSize::from(22)));
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn sequential_edits_stay_consistent() {
        let src = "let x = 1;";
        let mut cache = make(src);

        // First edit: rename 'x' → 'abc'
        let e1 = TextEdit::replace(TextRange::new(TextSize::from(4), TextSize::from(5)), "abc");
        let s1 = apply_and_relex(&mut cache, e1);
        assert_eq!(token_texts(&cache), full_texts(&s1));

        // Second edit: rename 'abc' → 'z'
        let e2 = TextEdit::replace(TextRange::new(TextSize::from(4), TextSize::from(7)), "z");
        let s2 = apply_and_relex(&mut cache, e2);
        assert_eq!(token_texts(&cache), full_texts(&s2));
    }

    #[test]
    fn replace_spanning_line_boundary() {
        let src = "let x = 1;\nlet y = 2;";
        let mut cache = make(src);
        // Replace "1;\nlet" with "99"
        let edit = TextEdit::replace(TextRange::new(TextSize::from(8), TextSize::from(15)), "99");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }

    #[test]
    fn prefix_tokens_unchanged_after_edit() {
        let src = "let a = 1;\nlet b = 2;";
        let cache0 = make(src);
        let mut cache1 = make(src);

        // Edit only the second line.
        let edit = TextEdit::replace(TextRange::new(TextSize::from(15), TextSize::from(16)), "99");
        apply_and_relex(&mut cache1, edit);

        // Tokens on the first line must be byte-for-byte identical.
        let prefix_len = cache0
            .tokens()
            .iter()
            .take_while(|t| t.range.end() <= TextSize::from(11))
            .count();
        assert!(prefix_len > 0);
        assert_eq!(
            &cache0.tokens()[..prefix_len],
            &cache1.tokens()[..prefix_len]
        );
    }

    #[test]
    fn empty_source_prime_and_edit() {
        let mut cache = make("");
        let edit = TextEdit::insert(TextSize::from(0), "let x = 1;");
        let new_src = apply_and_relex(&mut cache, edit);
        assert_eq!(token_texts(&cache), full_texts(&new_src));
    }
}
