use sylven::{Highlight, LanguageId, LanguagePlugin, ParseResult, SymbolInfo, SyntaxFeatures};
use sylven_lex::{SyntaxKind, Token, TokenStream};
use sylven_parse::{Parser, build_tree};
use sylven_text::{TextRange, TextSize, TextSnapshot};
use sylven_tree::{SyntaxElement, SyntaxTree};

use crate::compile::{CompiledItem, CompiledRule, CompiledSpec, FirstToken};

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

        // 2. Build the syntax tree.
        let mut parser = Parser::new(tokens);
        if self.spec.nodes.is_empty() {
            // Flat FILE tree: all tokens are direct children of the root.
            parser.start_node(self.spec.file_kind);
            while !parser.at_eof() {
                parser.bump();
            }
            parser.eat_trailing_trivia();
            parser.finish_node();
        } else {
            // Grammar-driven tree: root is the first declared node.
            grammar_parse_node(&mut parser, &self.spec, 0, source);
        }
        let events = parser.finish();
        let (tree, errors) = build_tree(tokens, source, events);

        // 3. Derive syntax features from the token stream and tree.
        let mut features = derive_features(tokens, source, &self.spec);
        features.symbols = extract_symbols(&tree, &self.spec);

        ParseResult {
            tree,
            errors,
            features,
        }
    }
}

// ── Grammar-driven tree builder ───────────────────────────────────────────────

/// Parse a single grammar node (by index into `spec.nodes`), emitting
/// `start_node` / items / `finish_node` events into `parser`.
fn grammar_parse_node(parser: &mut Parser, spec: &CompiledSpec, node_idx: usize, source: &str) {
    let kind = spec.nodes[node_idx].kind;
    // Clone the rule to avoid holding a reference into spec while we mutate parser.
    let rule = spec.nodes[node_idx].rule.clone();
    parser.start_node(kind);
    match rule {
        CompiledRule::Sequence(items) => {
            for item in &items {
                grammar_execute_item(parser, spec, item, source);
            }
        }
    }
    parser.finish_node();
}

fn grammar_execute_item(
    parser: &mut Parser,
    spec: &CompiledSpec,
    item: &CompiledItem,
    source: &str,
) {
    match item {
        CompiledItem::Token(kind) => {
            parser.expect(*kind);
        }
        CompiledItem::Keyword { kind, text } => {
            if parser.current() == *kind && current_text(parser, source) == text {
                parser.bump();
            } else {
                parser.error(format!("expected keyword `{text}`"));
            }
        }
        CompiledItem::Node(idx) => {
            grammar_parse_node(parser, spec, *idx, source);
        }
        CompiledItem::Choice(alts) => {
            let cur_kind = parser.current();
            let cur_text = current_text(parser, source);
            let matched = alts
                .iter()
                .copied()
                .find(|&alt| first_tokens_match(&spec.nodes[alt].first_tokens, cur_kind, cur_text));
            if let Some(alt) = matched {
                grammar_parse_node(parser, spec, alt, source);
            } else if !parser.at_eof() {
                parser.error("unexpected token");
                parser.bump();
            }
        }
        CompiledItem::Optional(inner) => {
            if item_can_start(parser, spec, inner, source) {
                grammar_execute_item(parser, spec, inner, source);
            }
        }
        CompiledItem::Repeat(inner) => {
            while item_can_start(parser, spec, inner, source) && !parser.at_eof() {
                grammar_execute_item(parser, spec, inner, source);
            }
        }
    }
}

fn item_can_start(parser: &Parser, spec: &CompiledSpec, item: &CompiledItem, source: &str) -> bool {
    let cur_kind = parser.current();
    let cur_text = current_text(parser, source);
    match item {
        CompiledItem::Token(kind) => cur_kind == *kind,
        CompiledItem::Keyword { kind, text } => cur_kind == *kind && cur_text == text,
        CompiledItem::Node(idx) => {
            first_tokens_match(&spec.nodes[*idx].first_tokens, cur_kind, cur_text)
        }
        CompiledItem::Choice(alts) => alts
            .iter()
            .any(|&alt| first_tokens_match(&spec.nodes[alt].first_tokens, cur_kind, cur_text)),
        CompiledItem::Optional(_) => true,
        CompiledItem::Repeat(inner) => item_can_start(parser, spec, inner, source),
    }
}

fn first_tokens_match(first: &[FirstToken], kind: SyntaxKind, text: &str) -> bool {
    first.iter().any(|ft| match ft {
        FirstToken::Kind(k) => *k == kind,
        FirstToken::Keyword { kind: k, text: t } => *k == kind && t == text,
    })
}

fn current_text<'s>(parser: &Parser, source: &'s str) -> &'s str {
    let range = parser.current_range();
    let start = range.start().to_usize();
    let end = range.end().to_usize().min(source.len());
    &source[start..end]
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

// ── Symbol extraction ─────────────────────────────────────────────────────────

/// Walk the syntax tree and extract document symbols according to
/// `spec.symbol_rules`. For each node whose kind matches a rule's `node_kind`,
/// the first non-trivia child token with `field_kind` becomes the symbol name.
fn extract_symbols(tree: &SyntaxTree, spec: &CompiledSpec) -> Vec<SymbolInfo> {
    if spec.symbol_rules.is_empty() {
        return Vec::new();
    }
    let mut symbols: Vec<SymbolInfo> = Vec::new();
    for element in tree.root().preorder() {
        let SyntaxElement::Node(node) = element else {
            continue;
        };
        for rule in &spec.symbol_rules {
            if node.kind() != rule.node_kind {
                continue;
            }
            for child in node.children_with_tokens() {
                let SyntaxElement::Token(tok) = child else {
                    continue;
                };
                if tok.kind().is_trivia() {
                    continue;
                }
                if tok.kind() == rule.field_kind {
                    symbols.push(SymbolInfo {
                        name: tok.text().to_string(),
                        name_range: tok.text_range(),
                        decl_range: node.text_range(),
                        kind: rule.symbol_kind,
                    });
                    break;
                }
            }
        }
    }
    symbols
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

    // ── Grammar-driven tree tests ─────────────────────────────────────────────

    const GRAMMAR_SPEC: &str = r#"
language { id "g" extensions [".g"] }
tokens {
  keyword ["fn", "let"]
  ident    /[a-zA-Z_]\w*/
  "(" lparen
  ")" rparen
  "{" lbrace
  "}" rbrace
  "=" eq
  ";" semicolon
  whitespace /\s+/ trivia
}
grammar {
  node File    { TopDecl* }
  node TopDecl { FnDecl | LetStmt }
  node FnDecl  { "fn" name:ident "(" ")" Block }
  node LetStmt { "let" name:ident "=" ident ";" }
  node Block   { "{" LetStmt* "}" }
}
symbols {
  FnDecl.name -> function
  LetStmt.name -> constant
}
"#;

    fn make_grammar_plugin() -> RuntimePlugin {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        RuntimePlugin::new(compile(&spec))
    }

    #[test]
    fn grammar_parse_is_lossless() {
        let src = "fn foo() { let x = y; }";
        let result = make_grammar_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn grammar_root_is_file_node() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        let file_kind = cs.file_kind;
        let result = RuntimePlugin::new(cs).parse(&snap("fn foo() {}"));
        assert_eq!(result.tree.root().kind(), file_kind);
    }

    #[test]
    fn grammar_produces_structured_tree_not_flat() {
        let src = "fn foo() { let x = y; }";
        let result = make_grammar_plugin().parse(&snap(src));
        let root = result.tree.root();
        // Root (File) must have children that are nodes, not raw tokens.
        let child_count = root.children().count();
        assert!(child_count > 0, "File should have at least one child node");
        // The first child should be a TopDecl that wraps FnDecl — tree is nested,
        // not flat (flat would have many token-level children under root).
        let first_child = root.children().next().unwrap();
        // TopDecl has exactly one child (FnDecl)
        assert!(
            first_child.children().count() > 0,
            "TopDecl should have child nodes (FnDecl)"
        );
    }

    #[test]
    fn grammar_fn_decl_contains_block_child() {
        let src = "fn foo() { let x = y; }";
        let result = make_grammar_plugin().parse(&snap(src));
        let root = result.tree.root();
        // Descend: File → TopDecl → FnDecl → should contain a Block child
        let top_decl = root.children().next().expect("File has TopDecl");
        let fn_decl = top_decl.children().next().expect("TopDecl has FnDecl");
        let has_block = fn_decl.children().any(|n| {
            let spec = parse_spec(GRAMMAR_SPEC).unwrap();
            let cs = compile(&spec);
            let block_node = cs.nodes.iter().find(|n| n.name == "Block").unwrap();
            n.kind() == block_node.kind
        });
        assert!(has_block, "FnDecl should contain a Block child");
    }

    #[test]
    fn grammar_empty_body_lossless() {
        let src = "fn foo() {}";
        let result = make_grammar_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
    }

    #[test]
    fn grammar_multiple_top_decls() {
        let src = "fn a() {} fn b() {}";
        let result = make_grammar_plugin().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        let root = result.tree.root();
        assert_eq!(root.children().count(), 2, "two TopDecl nodes");
    }

    // ── Symbol extraction tests ───────────────────────────────────────────────

    #[test]
    fn symbols_extracted_for_fn_decl() {
        use sylven::SymbolKind;
        let src = "fn foo() {}";
        let result = make_grammar_plugin().parse(&snap(src));
        assert_eq!(result.features.symbols.len(), 1);
        let sym = &result.features.symbols[0];
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    #[test]
    fn symbols_multiple_fn_decls() {
        use sylven::SymbolKind;
        let src = "fn a() {} fn b() {}";
        let result = make_grammar_plugin().parse(&snap(src));
        let fns: Vec<_> = result
            .features
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "a");
        assert_eq!(fns[1].name, "b");
    }

    #[test]
    fn symbols_let_stmt_is_constant() {
        use sylven::SymbolKind;
        let src = "fn foo() { let x = y; }";
        let result = make_grammar_plugin().parse(&snap(src));
        let constants: Vec<_> = result
            .features
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(constants.len(), 1);
        assert_eq!(constants[0].name, "x");
    }

    #[test]
    fn no_symbols_without_symbol_rules() {
        let result = make_plugin().parse(&snap("fn foo() {}"));
        assert!(result.features.symbols.is_empty());
    }
}
