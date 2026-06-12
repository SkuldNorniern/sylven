use sylven::{Highlight, LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures};
use sylven_lex::{SyntaxKind, Token, TokenStream};
use sylven_parse::{Parser, build_tree};
use sylven_text::{TextRange, TextSize, TextSnapshot};

use crate::compile::CompiledSpec;

/// A [`LanguagePlugin`] driven entirely by a [`CompiledSpec`] — no hand-written
/// grammar. Produces a flat FILE tree (all tokens are direct children of the
/// root node) and derives highlights, folds, and bracket pairs from the token
/// stream.
pub struct RuntimePlugin {
    /// The `&'static str` we hand to `LanguageId`. Pinned via `Box::leak` at
    /// construction time so it outlives `self`.
    lang_id: LanguageId,
    spec: CompiledSpec,
}

impl RuntimePlugin {
    /// Create a new plugin from a compiled spec. Leaks the `lang_id` string
    /// once so `LanguageId(&'static str)` is satisfied.
    pub fn new(spec: CompiledSpec) -> Self {
        let leaked: &'static str = Box::leak(spec.lang_id.clone().into_boxed_str());
        RuntimePlugin {
            lang_id: LanguageId(leaked),
            spec,
        }
    }
}

impl LanguagePlugin for RuntimePlugin {
    fn id(&self) -> LanguageId {
        self.lang_id
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let source = snapshot.text();

        // 1. Lex source → Vec<Token> ending with EOF.
        let raw_tokens = runtime_lex(source, &self.spec);
        let stream = TokenStream::new(raw_tokens);
        let tokens = stream.as_slice();

        // 2. Build flat FILE tree: start FILE → bump every token → finish FILE.
        let mut parser = Parser::new(tokens);
        parser.start_node(self.spec.file_kind);
        while !parser.at_eof() {
            parser.bump();
        }
        parser.eat_trailing_trivia();
        parser.finish_node();
        let events = parser.finish();
        let (tree, errors) = build_tree(tokens, source, events);

        // 3. Derive syntax features from the token stream.
        let features = derive_features(tokens, source, &self.spec);

        ParseResult {
            tree,
            errors,
            features,
        }
    }
}

// ── Runtime lexer ─────────────────────────────────────────────────────────────

/// Tokenise `source` using the pattern matchers in `spec`. Returns a
/// `Vec<Token>` that always ends with a `SyntaxKind::EOF` token covering the
/// empty range past the end of the source.
fn runtime_lex(source: &str, spec: &CompiledSpec) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut pos: usize = 0;

    while pos < source.len() {
        let remaining = &source[pos..];
        let mut matched = false;

        for (pattern, kind, _is_trivia) in &spec.matchers {
            if let Some(len) = pattern.match_at(remaining) {
                if len == 0 {
                    continue;
                }
                let start = TextSize::from(pos as u32);
                let end = TextSize::from((pos + len) as u32);
                tokens.push(Token::new(*kind, TextRange::new(start, end)));
                pos += len;
                matched = true;
                break;
            }
        }

        if !matched {
            // Preserve one complete UTF-8 code point as an ERROR token so the
            // next source slice remains on a character boundary.
            let len = remaining
                .chars()
                .next()
                .expect("remaining source is non-empty")
                .len_utf8();
            let start = TextSize::from(pos as u32);
            let end = TextSize::from((pos + len) as u32);
            tokens.push(Token::new(SyntaxKind::ERROR, TextRange::new(start, end)));
            pos += len;
        }
    }

    // Trailing EOF (required by Parser::new).
    let eof_pos = TextSize::from(source.len() as u32);
    tokens.push(Token::new(
        SyntaxKind::EOF,
        TextRange::new(eof_pos, eof_pos),
    ));
    tokens
}

// ── Feature derivation ────────────────────────────────────────────────────────

fn derive_features(tokens: &[Token], source: &str, spec: &CompiledSpec) -> SyntaxFeatures {
    let mut highlights: Vec<Highlight> = Vec::new();
    let mut brackets: Vec<(TextRange, TextRange)> = Vec::new();
    let mut folds: Vec<TextRange> = Vec::new();

    // Stack of (open_kind, open_range) for bracket matching.
    let mut open_stack: Vec<(SyntaxKind, TextRange)> = Vec::new();

    for tok in tokens {
        if tok.kind == SyntaxKind::EOF {
            break;
        }

        // Token-level highlights.
        if let Some(&hk) = spec.highlights.get(&tok.kind) {
            highlights.push(Highlight {
                range: tok.range,
                kind: hk,
            });
        }

        if spec.fold_brackets {
            // Check if this is an open bracket.
            for &(open_kind, _close_kind) in &spec.bracket_pairs {
                if tok.kind == open_kind {
                    open_stack.push((open_kind, tok.range));
                    break;
                }
            }

            // Check if this is a close bracket that matches the top of the stack.
            for &(open_kind, close_kind) in &spec.bracket_pairs {
                if tok.kind == close_kind {
                    // Pop the most recent matching open.
                    if let Some(pos) = open_stack.iter().rposition(|(k, _)| *k == open_kind) {
                        let (_, open_range) = open_stack.remove(pos);
                        brackets.push((open_range, tok.range));

                        // Emit a fold only when they span multiple lines.
                        if crosses_newline(source, open_range, tok.range) {
                            folds.push(TextRange::new(open_range.start(), tok.range.end()));
                        }
                    }
                    break;
                }
            }
        }
    }

    SyntaxFeatures {
        highlights,
        folds,
        brackets,
        ..Default::default()
    }
}

fn crosses_newline(source: &str, open: TextRange, close: TextRange) -> bool {
    let start = open.start().to_usize();
    let end = close.end().to_usize().min(source.len());
    source[start..end].contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_dsl::parse_spec;
    use sylven_text::{DocumentId, RevisionId};

    use crate::compile::compile;

    const MINI_OX: &str = r#"
language { id "mini-oxygen" extensions [".oxy"] comment.line "//" }
tokens {
  keyword ["fn", "let", "if", "else", "return"]
  ident      /[a-zA-Z_][a-zA-Z0-9_]*/
  int.number /[0-9]+/
  string     /"([^"\\]|\\.)*"/
  line.comment "//" to newline
  whitespace /\s+/ trivia
  "(" lparen
  ")" rparen
  "{" lbrace
  "}" rbrace
  "+" plus
  ";" semicolon
}
highlight {
  token keyword    -> keyword
  token string     -> string
  token int.number -> number
}
fold { Block when multiline }
"#;

    fn make_plugin() -> RuntimePlugin {
        let spec = parse_spec(MINI_OX).unwrap();
        RuntimePlugin::new(compile(&spec))
    }

    fn snap(source: &str) -> TextSnapshot {
        TextSnapshot::new(DocumentId(0), RevisionId(0), source)
    }

    #[test]
    fn id_matches_spec() {
        let p = make_plugin();
        assert_eq!(p.id(), LanguageId("mini-oxygen"));
    }

    #[test]
    fn parse_produces_lossless_tree() {
        let src = "let x = 42;";
        let result = make_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn parse_empty_source() {
        let result = make_plugin().parse(&snap(""));
        assert_eq!(result.tree.text(), "");
    }

    #[test]
    fn keyword_highlight_emitted() {
        let result = make_plugin().parse(&snap("fn foo"));
        assert!(result.features.highlights.iter().any(|h| {
            use sylven::HighlightKind;
            h.kind == HighlightKind::Keyword
        }));
    }

    #[test]
    fn bracket_pair_detected() {
        let src = "fn foo() {}";
        let result = make_plugin().parse(&snap(src));
        assert!(!result.features.brackets.is_empty());
    }

    #[test]
    fn multiline_block_produces_fold() {
        let src = "fn foo() {\n  let x = 1;\n}";
        let result = make_plugin().parse(&snap(src));
        assert!(!result.features.folds.is_empty());
    }

    #[test]
    fn single_line_block_no_fold() {
        let src = "fn foo() {}";
        let result = make_plugin().parse(&snap(src));
        assert!(result.features.folds.is_empty());
    }

    #[test]
    fn unknown_char_becomes_error_token_not_panic() {
        let src = "let x @ 1;";
        let result = make_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn unknown_unicode_char_is_lossless() {
        let src = "let cafe = \u{2615};";
        let result = make_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn unknown_unicode_between_recognized_tokens_does_not_panic() {
        let src = "fn \u{03bb}() {}";
        let result = make_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|highlight| highlight.kind == sylven::HighlightKind::Keyword)
        );
    }

    #[test]
    fn tree_root_kind_is_file_kind() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        let file_kind = cs.file_kind;
        let plugin = RuntimePlugin::new(cs);
        let result = plugin.parse(&snap("let x = 1;"));
        assert_eq!(result.tree.root().kind(), file_kind);
    }
}
