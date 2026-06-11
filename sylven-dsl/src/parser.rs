use crate::ast::{
    Assoc, FoldCondition, FoldRule, HighlightRule, HighlightSource, LanguageMeta, NodeDecl,
    NodeField, PrattInfix, PrattPrefix, PrattSpec, RecoveryRule, RecoveryStrategy, SylvenSpec,
    SymbolRule, TokenDecl, TokenKind,
};
use crate::lexer::{TK, Token, lex};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// A parse error from the `.sylven` DSL parser.
#[derive(Debug, Clone)]
pub struct DslError {
    pub offset: usize,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub errors: Vec<DslError>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    // --- cursor helpers ---

    fn cur(&self) -> TK {
        self.tokens[self.pos].kind
    }

    fn cur_text(&self) -> &str {
        &self.tokens[self.pos].text
    }

    fn cur_offset(&self) -> usize {
        self.tokens[self.pos].offset
    }

    fn at(&self, kind: TK) -> bool {
        self.cur() == kind
    }

    fn at_eof(&self) -> bool {
        self.cur() == TK::Eof
    }

    fn at_word(&self, word: &str) -> bool {
        self.cur() == TK::Word && self.cur_text() == word
    }

    // --- consume helpers ---

    /// Advance one position.
    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    /// Advance and return a clone of the current token's text.
    fn take(&mut self) -> String {
        let t = self.tokens[self.pos].text.clone();
        self.advance();
        t
    }

    /// Record an error at the current position.
    fn error(&mut self, msg: impl Into<String>) {
        let off = self.cur_offset();
        self.errors.push(DslError {
            offset: off,
            message: msg.into(),
        });
    }

    /// Advance if current kind matches; record an error otherwise.
    fn expect(&mut self, kind: TK) -> bool {
        if self.cur() == kind {
            self.advance();
            true
        } else {
            self.error(format!(
                "expected {:?}, found {:?} {:?}",
                kind,
                self.cur(),
                self.cur_text()
            ));
            false
        }
    }

    /// Advance and return text if current is a Word; otherwise error + None.
    fn expect_word(&mut self) -> Option<String> {
        if self.cur() == TK::Word {
            Some(self.take())
        } else {
            self.error(format!(
                "expected word, found {:?} {:?}",
                self.cur(),
                self.cur_text()
            ));
            None
        }
    }

    /// Advance and return text if current is a Str; otherwise error + None.
    fn expect_str(&mut self) -> Option<String> {
        if self.cur() == TK::Str {
            Some(self.take())
        } else {
            self.error(format!(
                "expected string, found {:?} {:?}",
                self.cur(),
                self.cur_text()
            ));
            None
        }
    }

    /// Skip tokens until `}` or EOF, consuming the `}`.
    fn skip_to_rbrace(&mut self) {
        while !self.at_eof() && !self.at(TK::RBrace) {
            self.advance();
        }
        if self.at(TK::RBrace) {
            self.advance();
        }
    }

    // --- top-level ---

    fn parse_top(&mut self) -> SylvenSpec {
        let mut spec = SylvenSpec::default();
        while !self.at_eof() {
            if !self.at(TK::Word) {
                self.error(format!("expected block keyword, found {:?}", self.cur()));
                self.advance();
                continue;
            }
            let kw = self.take();
            match kw.as_str() {
                "language" => spec.language = self.parse_language_block(),
                "tokens" => spec.tokens = self.parse_tokens_block(),
                "grammar" => spec.grammar = self.parse_grammar_block(),
                "pratt" => spec.pratt.push(self.parse_pratt_block()),
                "recovery" => spec.recovery = self.parse_recovery_block(),
                "highlight" => spec.highlight = self.parse_highlight_block(),
                "fold" => spec.fold = self.parse_fold_block(),
                "symbols" => spec.symbols = self.parse_symbols_block(),
                other => {
                    self.error(format!("unknown block `{other}`"));
                    self.skip_to_rbrace();
                }
            }
        }
        spec
    }

    // --- language {} ---

    fn parse_language_block(&mut self) -> LanguageMeta {
        let mut meta = LanguageMeta::default();
        if !self.expect(TK::LBrace) {
            return meta;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(key) = self.expect_word() else {
                self.skip_to_rbrace();
                return meta;
            };
            match key.as_str() {
                "id" => {
                    if let Some(s) = self.expect_str() {
                        meta.id = s;
                    }
                }
                "extensions" => {
                    meta.extensions = self.parse_str_list();
                }
                "comment.line" => {
                    meta.comment_line = self.expect_str();
                }
                "comment.block" => {
                    if let (Some(open), Some(close)) = (self.expect_str(), self.expect_str()) {
                        meta.comment_block = Some((open, close));
                    }
                }
                other => {
                    self.error(format!("unknown language key `{other}`"));
                }
            }
        }
        self.expect(TK::RBrace);
        meta
    }

    fn parse_str_list(&mut self) -> Vec<String> {
        let mut list = Vec::new();
        if !self.expect(TK::LBracket) {
            return list;
        }
        while self.at(TK::Str) {
            list.push(self.take());
            if self.at(TK::Comma) {
                self.advance();
            }
        }
        self.expect(TK::RBracket);
        list
    }

    // --- tokens {} ---

    fn parse_tokens_block(&mut self) -> Vec<TokenDecl> {
        let mut decls = Vec::new();
        if !self.expect(TK::LBrace) {
            return decls;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            if let Some(d) = self.parse_token_decl() {
                decls.push(d);
            }
        }
        self.expect(TK::RBrace);
        decls
    }

    fn parse_token_decl(&mut self) -> Option<TokenDecl> {
        // `"lit" name`  →  Literal
        if self.at(TK::Str) {
            let lit = self.take();
            let name = self.expect_word()?;
            return Some(TokenDecl {
                name,
                kind: TokenKind::Literal(lit),
                is_trivia: false,
            });
        }

        // `name ...`
        let name = self.expect_word()?;

        match self.cur() {
            // `name [...]`  →  KeywordSet
            TK::LBracket => {
                let kws = self.parse_str_list();
                Some(TokenDecl {
                    name,
                    kind: TokenKind::KeywordSet(kws),
                    is_trivia: false,
                })
            }
            // `name /pat/` (optionally `trivia`)  →  Regex
            TK::Regex => {
                let pat = self.take();
                let is_trivia = self.at_word("trivia") && {
                    self.advance();
                    true
                };
                Some(TokenDecl {
                    name,
                    kind: TokenKind::Regex(pat),
                    is_trivia,
                })
            }
            // `name "open" to newline|"close"`  →  LineComment or BlockComment
            TK::Str => {
                let open = self.take();
                if !self.at_word("to") {
                    self.error("expected `to` after comment open delimiter");
                    return None;
                }
                self.advance(); // consume `to`
                if self.at_word("newline") {
                    self.advance();
                    Some(TokenDecl {
                        name,
                        kind: TokenKind::LineComment(open),
                        is_trivia: false,
                    })
                } else if self.at(TK::Str) {
                    let close = self.take();
                    Some(TokenDecl {
                        name,
                        kind: TokenKind::BlockComment(open, close),
                        is_trivia: false,
                    })
                } else {
                    self.error("expected `newline` or closing string after `to`");
                    None
                }
            }
            _ => {
                self.error(format!("unexpected {:?} in token declaration", self.cur()));
                self.advance();
                None
            }
        }
    }

    // --- grammar {} ---

    fn parse_grammar_block(&mut self) -> Vec<NodeDecl> {
        let mut decls = Vec::new();
        if !self.expect(TK::LBrace) {
            return decls;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            if self.at_word("node") {
                self.advance();
                if let Some(d) = self.parse_node_decl() {
                    decls.push(d);
                }
            } else {
                self.error(format!("expected `node`, found {:?}", self.cur()));
                self.advance();
            }
        }
        self.expect(TK::RBrace);
        decls
    }

    fn parse_node_decl(&mut self) -> Option<NodeDecl> {
        let name = self.expect_word()?;
        let mut fields = Vec::new();
        if self.at(TK::LBrace) {
            self.advance();
            while !self.at(TK::RBrace) && !self.at_eof() {
                let Some(label) = self.expect_word() else {
                    break;
                };
                if !self.expect(TK::Colon) {
                    break;
                }
                let Some(ty) = self.expect_word() else {
                    break;
                };
                fields.push(NodeField { label, ty });
            }
            self.expect(TK::RBrace);
        }
        Some(NodeDecl { name, fields })
    }

    // --- pratt Name {} ---

    fn parse_pratt_block(&mut self) -> PrattSpec {
        let name = self.expect_word().unwrap_or_default();
        let mut spec = PrattSpec {
            name,
            atoms: Vec::new(),
            prefix: Vec::new(),
            infix: Vec::new(),
        };
        if !self.expect(TK::LBrace) {
            return spec;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(kw) = self.expect_word() else {
                break;
            };
            match kw.as_str() {
                "atom" => {
                    if let Some(n) = self.expect_word() {
                        spec.atoms.push(n);
                    }
                }
                "prefix" => {
                    let op = self.expect_str().unwrap_or_default();
                    let node = self.expect_word().unwrap_or_default();
                    spec.prefix.push(PrattPrefix { op, node });
                }
                "infix" => {
                    let assoc_w = self.expect_word().unwrap_or_default();
                    let assoc = if assoc_w == "right" {
                        Assoc::Right
                    } else {
                        Assoc::Left
                    };
                    let op = self.expect_str().unwrap_or_default();
                    let prec = if self.at(TK::Number) {
                        self.take().parse::<u8>().unwrap_or(0)
                    } else {
                        self.error("expected precedence number after operator");
                        0
                    };
                    let node = self.expect_word().unwrap_or_default();
                    spec.infix.push(PrattInfix {
                        assoc,
                        op,
                        prec,
                        node,
                    });
                }
                other => {
                    self.error(format!("unexpected pratt entry `{other}`"));
                }
            }
        }
        self.expect(TK::RBrace);
        spec
    }

    // --- recovery {} ---

    fn parse_recovery_block(&mut self) -> Vec<RecoveryRule> {
        let mut rules = Vec::new();
        if !self.expect(TK::LBrace) {
            return rules;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(node) = self.expect_word() else {
                break;
            };
            let strategy_w = self.expect_word().unwrap_or_default();
            match strategy_w.as_str() {
                "skip_until" => {
                    let toks = self.parse_str_list();
                    rules.push(RecoveryRule {
                        node,
                        strategy: RecoveryStrategy::SkipUntil(toks),
                    });
                }
                other => {
                    self.error(format!("unknown recovery strategy `{other}`"));
                }
            }
        }
        self.expect(TK::RBrace);
        rules
    }

    // --- highlight {} ---

    fn parse_highlight_block(&mut self) -> Vec<HighlightRule> {
        let mut rules = Vec::new();
        if !self.expect(TK::LBrace) {
            return rules;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(first) = self.expect_word() else {
                break;
            };
            let source = if first == "token" {
                let name = self.expect_word().unwrap_or_default();
                HighlightSource::Token(name)
            } else {
                // `Node.field` — split on the first dot.
                match first.find('.') {
                    Some(dot) => HighlightSource::NodeField {
                        node: first[..dot].to_owned(),
                        field: first[dot + 1..].to_owned(),
                    },
                    None => {
                        self.error(format!("expected `Node.field` in highlight, got `{first}`"));
                        continue;
                    }
                }
            };
            if !self.expect(TK::Arrow) {
                break;
            }
            let scope = self.expect_word().unwrap_or_default();
            rules.push(HighlightRule { source, scope });
        }
        self.expect(TK::RBrace);
        rules
    }

    // --- fold {} ---

    fn parse_fold_block(&mut self) -> Vec<FoldRule> {
        let mut rules = Vec::new();
        if !self.expect(TK::LBrace) {
            return rules;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(node) = self.expect_word() else {
                break;
            };
            if !self.at_word("when") {
                self.error(format!(
                    "expected `when` after node `{node}`, found {:?}",
                    self.cur()
                ));
                continue;
            }
            self.advance(); // consume `when`
            let cond_w = self.expect_word().unwrap_or_default();
            let condition = match cond_w.as_str() {
                "multiline" => FoldCondition::Multiline,
                "always" => FoldCondition::Always,
                other => {
                    self.error(format!("unknown fold condition `{other}`"));
                    FoldCondition::Multiline
                }
            };
            rules.push(FoldRule { node, condition });
        }
        self.expect(TK::RBrace);
        rules
    }

    // --- symbols {} ---

    fn parse_symbols_block(&mut self) -> Vec<SymbolRule> {
        let mut rules = Vec::new();
        if !self.expect(TK::LBrace) {
            return rules;
        }
        while !self.at(TK::RBrace) && !self.at_eof() {
            let Some(source) = self.expect_word() else {
                break;
            };
            match source.find('.') {
                None => {
                    self.error(format!("expected `Node.field` in symbols, got `{source}`"));
                    continue;
                }
                Some(dot) => {
                    let node = source[..dot].to_owned();
                    let field = source[dot + 1..].to_owned();
                    if !self.expect(TK::Arrow) {
                        break;
                    }
                    let kind = self.expect_word().unwrap_or_default();
                    rules.push(SymbolRule { node, field, kind });
                }
            }
        }
        self.expect(TK::RBrace);
        rules
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a `.sylven` language spec. Returns the spec on success or a list of
/// parse errors (with byte offsets and messages) on failure.
pub fn parse_spec(source: &str) -> Result<SylvenSpec, Vec<DslError>> {
    let tokens = lex(source);
    let mut p = Parser::new(tokens);
    let spec = p.parse_top();
    if p.errors.is_empty() {
        Ok(spec)
    } else {
        Err(p.errors)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Assoc, FoldCondition, HighlightSource, RecoveryStrategy, TokenKind};

    /// Minimal spec used across several tests.
    const MINI_OXYGEN: &str = r#"
language {
  id "mini-oxygen"
  extensions [".oxy"]
  comment.line "//"
}

tokens {
  keyword    ["fn", "let", "if", "else", "return"]
  bool.lit   ["true", "false", "null"]
  ident      /[a-zA-Z_][a-zA-Z0-9_]*/
  int.number /[0-9]+/
  string     /"([^"\\]|\\.)*"/
  line.comment "//" to newline
  whitespace /\s+/ trivia
  "(" lparen
  ")" rparen
  "{" lbrace
  "}" rbrace
  "," comma
  ";" semicolon
  "=" eq
  "==" eqeq
  "!=" neq
  "<"  lt
  "<=" le
  ">"  gt
  ">=" ge
  "+"  plus
  "-"  minus
  "*"  star
  "/"  slash
  "!"  bang
}

grammar {
  node File
  node FnDecl      { name:Name params:ParamList body:Block }
  node ParamList
  node Param
  node Block
  node LetStmt     { name:Name value:Expr }
  node ReturnStmt  { value:Expr }
  node IfStmt      { cond:Expr then:Block }
  node ExprStmt    { expr:Expr }
  node CallExpr    { callee:NameRef args:ArgList }
  node ArgList
  node BinaryExpr  { lhs:Expr rhs:Expr }
  node PrefixExpr  { rhs:Expr }
  node ParenExpr   { inner:Expr }
  node Name
  node NameRef
  node Literal
}

pratt Expr {
  atom Literal
  atom NameRef
  atom CallExpr
  atom ParenExpr

  prefix "-" PrefixExpr
  prefix "!" PrefixExpr

  infix left  "==" 1 BinaryExpr
  infix left  "!=" 1 BinaryExpr
  infix left  "<"  3 BinaryExpr
  infix left  "<=" 3 BinaryExpr
  infix left  ">"  3 BinaryExpr
  infix left  ">=" 3 BinaryExpr
  infix left  "+"  5 BinaryExpr
  infix left  "-"  5 BinaryExpr
  infix left  "*"  7 BinaryExpr
  infix left  "/"  7 BinaryExpr
}

recovery {
  ExprStmt skip_until [";", "}"]
  Block    skip_until ["}"]
  FnDecl   skip_until ["}"]
}

highlight {
  token keyword      -> keyword
  token bool.lit     -> keyword
  token string       -> string
  token int.number   -> number
  token line.comment -> comment
  FnDecl.name        -> function.definition
  CallExpr.callee    -> function.call
}

fold {
  Block  when multiline
  FnDecl when multiline
}

symbols {
  FnDecl.name -> function
}
"#;

    fn ok(src: &str) -> SylvenSpec {
        match parse_spec(src) {
            Ok(s) => s,
            Err(errs) => panic!("parse errors: {:?}", errs),
        }
    }

    #[test]
    fn mini_oxygen_parses_without_errors() {
        let spec = ok(MINI_OXYGEN);
        assert_eq!(spec.language.id, "mini-oxygen");
        assert_eq!(spec.language.extensions, vec![".oxy"]);
    }

    #[test]
    fn language_block() {
        let spec = ok(MINI_OXYGEN);
        assert_eq!(spec.language.comment_line.as_deref(), Some("//"));
        assert!(spec.language.comment_block.is_none());
    }

    #[test]
    fn token_keyword_set() {
        let spec = ok(MINI_OXYGEN);
        let kw = spec.tokens.iter().find(|t| t.name == "keyword").unwrap();
        assert!(matches!(&kw.kind, TokenKind::KeywordSet(v) if v.contains(&"fn".to_owned())));
        assert!(!kw.is_trivia);
    }

    #[test]
    fn token_regex_and_trivia_flag() {
        let spec = ok(MINI_OXYGEN);
        let ws = spec.tokens.iter().find(|t| t.name == "whitespace").unwrap();
        assert!(ws.is_trivia);
        let ident = spec.tokens.iter().find(|t| t.name == "ident").unwrap();
        assert!(!ident.is_trivia);
    }

    #[test]
    fn token_literal() {
        let spec = ok(MINI_OXYGEN);
        let lp = spec.tokens.iter().find(|t| t.name == "lparen").unwrap();
        assert!(matches!(&lp.kind, TokenKind::Literal(s) if s == "("));
    }

    #[test]
    fn token_line_comment() {
        let spec = ok(MINI_OXYGEN);
        let lc = spec
            .tokens
            .iter()
            .find(|t| t.name == "line.comment")
            .unwrap();
        assert!(matches!(&lc.kind, TokenKind::LineComment(s) if s == "//"));
    }

    #[test]
    fn grammar_nodes() {
        let spec = ok(MINI_OXYGEN);
        assert!(spec.grammar.iter().any(|n| n.name == "File"));
        let fn_decl = spec.grammar.iter().find(|n| n.name == "FnDecl").unwrap();
        assert_eq!(fn_decl.fields.len(), 3);
        assert_eq!(fn_decl.fields[0].label, "name");
        assert_eq!(fn_decl.fields[0].ty, "Name");
    }

    #[test]
    fn pratt_spec() {
        let spec = ok(MINI_OXYGEN);
        let p = spec.pratt.iter().find(|p| p.name == "Expr").unwrap();
        assert_eq!(p.atoms, vec!["Literal", "NameRef", "CallExpr", "ParenExpr"]);
        assert_eq!(p.prefix.len(), 2);
        let add = p.infix.iter().find(|i| i.op == "+").unwrap();
        assert_eq!(add.prec, 5);
        assert_eq!(add.assoc, Assoc::Left);
        assert_eq!(add.node, "BinaryExpr");
    }

    #[test]
    fn recovery_rules() {
        let spec = ok(MINI_OXYGEN);
        let r = spec.recovery.iter().find(|r| r.node == "ExprStmt").unwrap();
        let RecoveryStrategy::SkipUntil(toks) = &r.strategy;
        assert!(toks.contains(&";".to_owned()));
    }

    #[test]
    fn highlight_token_rule() {
        let spec = ok(MINI_OXYGEN);
        let kw_rule = spec
            .highlight
            .iter()
            .find(|h| matches!(&h.source, HighlightSource::Token(n) if n == "keyword"))
            .unwrap();
        assert_eq!(kw_rule.scope, "keyword");
    }

    #[test]
    fn highlight_node_field_rule() {
        let spec = ok(MINI_OXYGEN);
        let fn_rule = spec
            .highlight
            .iter()
            .find(|h| {
                matches!(&h.source, HighlightSource::NodeField { node, .. } if node == "FnDecl")
            })
            .unwrap();
        assert_eq!(fn_rule.scope, "function.definition");
        if let HighlightSource::NodeField { field, .. } = &fn_rule.source {
            assert_eq!(field, "name");
        }
    }

    #[test]
    fn fold_rules() {
        let spec = ok(MINI_OXYGEN);
        let block = spec.fold.iter().find(|f| f.node == "Block").unwrap();
        assert_eq!(block.condition, FoldCondition::Multiline);
    }

    #[test]
    fn symbol_rules() {
        let spec = ok(MINI_OXYGEN);
        let sym = spec.symbols.iter().find(|s| s.node == "FnDecl").unwrap();
        assert_eq!(sym.field, "name");
        assert_eq!(sym.kind, "function");
    }

    #[test]
    fn returns_errors_on_unknown_block() {
        let src = "unknown { }";
        assert!(parse_spec(src).is_err());
    }
}
