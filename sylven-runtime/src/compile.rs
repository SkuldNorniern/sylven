use std::collections::{HashMap, HashSet};

use sylven::{HighlightKind, SymbolKind};
use sylven_lex::SyntaxKind;

use sylven_dsl::{
    FoldCondition, GrammarItem, HighlightSource, NodeDecl, SylvenSpec, SymbolRule, TokenDecl,
    TokenKind,
};

use crate::pattern::Pattern;

/// The compiled form of a `SylvenSpec` — ready for the runtime executor to
/// use without re-parsing the DSL source on every parse call.
#[derive(Debug, Clone)]
pub struct CompiledSpec {
    /// `LanguageId` string (owned; used via `Box::leak` in `RuntimePlugin`).
    pub lang_id: String,
    /// The compiled lexer: ordered list of `(Pattern, SyntaxKind, is_trivia)`.
    pub matchers: Vec<(Pattern, SyntaxKind, bool)>,
    /// `SyntaxKind` assigned to the root FILE node.
    pub file_kind: SyntaxKind,
    /// Token-level highlight rules: `SyntaxKind → HighlightKind`.
    pub highlights: HashMap<SyntaxKind, HighlightKind>,
    /// Bracket pairs `(open_kind, close_kind)` — used for fold and bracket features.
    pub bracket_pairs: Vec<(SyntaxKind, SyntaxKind)>,
    /// Whether folds should be emitted for bracket pairs that span multiple lines.
    pub fold_brackets: bool,
    /// Compiled grammar nodes; non-empty only when the spec has a `grammar {}` block.
    /// The first entry is always the root/file node.
    pub nodes: Vec<CompiledNode>,
    /// Compiled symbol extraction rules derived from the `symbols {}` block.
    pub symbol_rules: Vec<CompiledSymbolRule>,
}

// ── Compiled grammar types ─────────────────────────────────────────────────────

/// One compiled grammar node declaration.
#[derive(Debug, Clone)]
pub struct CompiledNode {
    /// Syntax kind assigned to this node.
    pub kind: SyntaxKind,
    /// Name as written in the spec (for diagnostics).
    pub name: String,
    /// Compiled grammar rule for this node.
    pub rule: CompiledRule,
    /// First-token set: the token kinds (plus optional keyword text) that can
    /// start this node. Used by the runtime executor for lookahead dispatch.
    pub first_tokens: Vec<FirstToken>,
}

/// Compiled grammar rule for a single node.
#[derive(Debug, Clone)]
pub enum CompiledRule {
    /// Consume items in order.
    Sequence(Vec<CompiledItem>),
}

/// One item in a compiled grammar sequence.
#[derive(Debug, Clone)]
pub enum CompiledItem {
    /// Consume a token of a specific kind (non-keyword).
    Token(SyntaxKind),
    /// Consume a keyword token whose source text equals `text`.
    Keyword { kind: SyntaxKind, text: String },
    /// Parse a child node (index into `CompiledSpec::nodes`).
    Node(usize),
    /// Dispatch to one of the listed nodes based on first-token lookahead.
    Choice(Vec<usize>),
    /// Execute inner item only when it can start.
    Optional(Box<CompiledItem>),
    /// Execute inner item zero-or-more times while it can start.
    Repeat(Box<CompiledItem>),
}

/// A token that can start a grammar node.
#[derive(Debug, Clone, PartialEq)]
pub enum FirstToken {
    /// Any token of this kind.
    Kind(SyntaxKind),
    /// A keyword token with this specific source text.
    Keyword { kind: SyntaxKind, text: String },
}

/// One compiled rule from the `symbols {}` block.
#[derive(Debug, Clone)]
pub struct CompiledSymbolRule {
    /// The kind of the grammar node that declares this symbol.
    pub node_kind: SyntaxKind,
    /// The kind of the child token that is the symbol's name.
    pub field_kind: SyntaxKind,
    /// The symbol category shown in the editor outline.
    pub symbol_kind: SymbolKind,
}

/// Compile a parsed `SylvenSpec` into a `CompiledSpec`. Assigns `SyntaxKind`
/// values starting at `SyntaxKind::LANG_KIND_BASE`:
///   - `LANG_KIND_BASE + 0` → FILE (root node)
///   - `LANG_KIND_BASE + 1..` → declared tokens in order
///
/// Trivia tokens (whitespace, line/block comments) are mapped to the shared
/// `WHITESPACE` / `COMMENT` kinds so that `Token::is_trivia()` works.
pub fn compile(spec: &SylvenSpec) -> CompiledSpec {
    let file_kind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

    // Build name→kind and name→pattern mappings.
    let mut next_id = SyntaxKind::LANG_KIND_BASE + 1;
    let mut name_to_kind: HashMap<&str, SyntaxKind> = HashMap::new();
    let mut matchers: Vec<(Pattern, SyntaxKind, bool)> = Vec::new();

    // We need keywords checked before ident, so we do two passes:
    // 1) collect all keyword sets into a single pattern per group
    // 2) then add the rest in declaration order

    // Split into keyword-set entries and others, preserving relative order.
    let mut kw_entries: Vec<&TokenDecl> = Vec::new();
    let mut other_entries: Vec<&TokenDecl> = Vec::new();
    for decl in &spec.tokens {
        if matches!(&decl.kind, TokenKind::KeywordSet(_)) {
            kw_entries.push(decl);
        } else {
            other_entries.push(decl);
        }
    }

    for decl in kw_entries.iter().chain(other_entries.iter()) {
        let (pattern, kind, is_trivia) = compile_token_decl(decl, &mut next_id);
        name_to_kind.insert(&decl.name, kind);
        matchers.push((pattern, kind, is_trivia));
    }

    // Compile token-level highlight rules.
    let mut highlights: HashMap<SyntaxKind, HighlightKind> = HashMap::new();
    for rule in &spec.highlight {
        let HighlightSource::Token(ref token_name) = rule.source else {
            continue; // NodeField rules require grammar — skip for Stage 5
        };
        let Some(&kind) = name_to_kind.get(token_name.as_str()) else {
            continue;
        };
        if let Some(hk) = parse_highlight_kind(&rule.scope) {
            highlights.insert(kind, hk);
        }
    }

    // Detect bracket pairs from Literal tokens whose text is a bracket char.
    let bracket_chars: &[(&str, &str)] = &[("(", ")"), ("{", "}"), ("[", "]")];
    let mut bracket_pairs: Vec<(SyntaxKind, SyntaxKind)> = Vec::new();
    for (open_lit, close_lit) in bracket_chars {
        let open_kind = spec.tokens.iter().find_map(|d| {
            if matches!(&d.kind, TokenKind::Literal(s) if s == open_lit) {
                name_to_kind.get(d.name.as_str()).copied()
            } else {
                None
            }
        });
        let close_kind = spec.tokens.iter().find_map(|d| {
            if matches!(&d.kind, TokenKind::Literal(s) if s == close_lit) {
                name_to_kind.get(d.name.as_str()).copied()
            } else {
                None
            }
        });
        if let (Some(ok), Some(ck)) = (open_kind, close_kind) {
            bracket_pairs.push((ok, ck));
        }
    }

    // fold_brackets: true if any fold rule references a multiline condition.
    let fold_brackets = spec
        .fold
        .iter()
        .any(|f| f.condition == FoldCondition::Multiline);

    // ── Grammar node compilation ─────────────────────────────────────────────

    // Build reverse maps needed to resolve grammar item references.
    // lit_to_kind: literal text (e.g. "(") → SyntaxKind
    let mut lit_to_kind: HashMap<&str, SyntaxKind> = HashMap::new();
    // kw_text_to_kind: keyword text (e.g. "fn") → shared keyword SyntaxKind
    let mut kw_text_to_kind: HashMap<&str, SyntaxKind> = HashMap::new();
    for (pat, kind, _) in &matchers {
        match pat {
            Pattern::Literal(s) => {
                lit_to_kind.insert(s.as_str(), *kind);
            }
            Pattern::Keywords(kws) => {
                for kw in kws {
                    kw_text_to_kind.insert(kw.as_str(), *kind);
                }
            }
            _ => {}
        }
    }

    let (nodes, symbol_rules) = if spec.grammar.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        compile_grammar(
            &spec.grammar,
            &spec.symbols,
            &name_to_kind,
            &lit_to_kind,
            &kw_text_to_kind,
            &mut next_id,
        )
    };

    // If grammar nodes exist, the first node is the root → override file_kind.
    let file_kind = nodes.first().map(|n| n.kind).unwrap_or(file_kind);

    CompiledSpec {
        lang_id: spec.language.id.clone(),
        matchers,
        file_kind,
        highlights,
        bracket_pairs,
        fold_brackets,
        nodes,
        symbol_rules,
    }
}

// ── Grammar compilation helpers ────────────────────────────────────────────────

fn compile_grammar(
    decls: &[NodeDecl],
    symbol_rules: &[SymbolRule],
    name_to_kind: &HashMap<&str, SyntaxKind>,
    lit_to_kind: &HashMap<&str, SyntaxKind>,
    kw_text_to_kind: &HashMap<&str, SyntaxKind>,
    next_id: &mut u16,
) -> (Vec<CompiledNode>, Vec<CompiledSymbolRule>) {
    // Pass 1: assign SyntaxKinds and build name→index map.
    let mut node_name_to_idx: HashMap<&str, usize> = HashMap::new();
    let mut node_kinds: Vec<SyntaxKind> = Vec::new();
    for (i, decl) in decls.iter().enumerate() {
        let kind = SyntaxKind(*next_id);
        *next_id += 1;
        node_name_to_idx.insert(decl.name.as_str(), i);
        node_kinds.push(kind);
    }

    // Pass 2: compile items for each node.
    let mut nodes: Vec<CompiledNode> = decls
        .iter()
        .enumerate()
        .map(|(i, decl)| {
            let rule = compile_node_items(
                &decl.items,
                name_to_kind,
                lit_to_kind,
                kw_text_to_kind,
                &node_name_to_idx,
            );
            CompiledNode {
                kind: node_kinds[i],
                name: decl.name.clone(),
                rule,
                first_tokens: Vec::new(), // filled in pass 3
            }
        })
        .collect();

    // Pass 3: compute first-token sets (needs all nodes to already exist).
    let first_tokens: Vec<Vec<FirstToken>> = (0..nodes.len())
        .map(|i| compute_first_tokens(&nodes, i, &mut HashSet::new()))
        .collect();
    for (node, ft) in nodes.iter_mut().zip(first_tokens) {
        node.first_tokens = ft;
    }

    // Pass 4: compile symbol rules.
    let compiled_symbols = compile_symbol_rules(
        symbol_rules,
        decls,
        &node_name_to_idx,
        &node_kinds,
        name_to_kind,
    );

    (nodes, compiled_symbols)
}

fn compile_node_items(
    items: &[GrammarItem],
    name_to_kind: &HashMap<&str, SyntaxKind>,
    lit_to_kind: &HashMap<&str, SyntaxKind>,
    kw_text_to_kind: &HashMap<&str, SyntaxKind>,
    node_name_to_idx: &HashMap<&str, usize>,
) -> CompiledRule {
    let compiled = items
        .iter()
        .map(|item| {
            compile_grammar_item(
                item,
                name_to_kind,
                lit_to_kind,
                kw_text_to_kind,
                node_name_to_idx,
            )
        })
        .collect();
    CompiledRule::Sequence(compiled)
}

fn compile_grammar_item(
    item: &GrammarItem,
    name_to_kind: &HashMap<&str, SyntaxKind>,
    lit_to_kind: &HashMap<&str, SyntaxKind>,
    kw_text_to_kind: &HashMap<&str, SyntaxKind>,
    node_name_to_idx: &HashMap<&str, usize>,
) -> CompiledItem {
    match item {
        GrammarItem::Literal(text) => {
            if let Some(&kind) = lit_to_kind.get(text.as_str()) {
                CompiledItem::Token(kind)
            } else if let Some(&kind) = kw_text_to_kind.get(text.as_str()) {
                CompiledItem::Keyword {
                    kind,
                    text: text.clone(),
                }
            } else {
                CompiledItem::Token(SyntaxKind::ERROR)
            }
        }
        GrammarItem::Field { ty, .. } | GrammarItem::Ref(ty) => {
            resolve_type_ref(ty, name_to_kind, node_name_to_idx)
        }
        GrammarItem::Choice(names) => {
            let alts = names
                .iter()
                .filter_map(|n| node_name_to_idx.get(n.as_str()).copied())
                .collect();
            CompiledItem::Choice(alts)
        }
        GrammarItem::Optional(inner) => CompiledItem::Optional(Box::new(compile_grammar_item(
            inner,
            name_to_kind,
            lit_to_kind,
            kw_text_to_kind,
            node_name_to_idx,
        ))),
        GrammarItem::Repeat(inner) => CompiledItem::Repeat(Box::new(compile_grammar_item(
            inner,
            name_to_kind,
            lit_to_kind,
            kw_text_to_kind,
            node_name_to_idx,
        ))),
    }
}

fn resolve_type_ref(
    ty: &str,
    name_to_kind: &HashMap<&str, SyntaxKind>,
    node_name_to_idx: &HashMap<&str, usize>,
) -> CompiledItem {
    if let Some(&idx) = node_name_to_idx.get(ty) {
        CompiledItem::Node(idx)
    } else if let Some(&kind) = name_to_kind.get(ty) {
        CompiledItem::Token(kind)
    } else {
        CompiledItem::Token(SyntaxKind::ERROR)
    }
}

fn compute_first_tokens(
    nodes: &[CompiledNode],
    idx: usize,
    visiting: &mut HashSet<usize>,
) -> Vec<FirstToken> {
    if !visiting.insert(idx) {
        return Vec::new(); // cycle guard
    }
    let result = first_of_rule(&nodes[idx].rule, nodes, visiting);
    visiting.remove(&idx);
    result
}

fn first_of_rule(
    rule: &CompiledRule,
    nodes: &[CompiledNode],
    visiting: &mut HashSet<usize>,
) -> Vec<FirstToken> {
    match rule {
        CompiledRule::Sequence(items) => items
            .first()
            .map(|item| first_of_item(item, nodes, visiting))
            .unwrap_or_default(),
    }
}

fn first_of_item(
    item: &CompiledItem,
    nodes: &[CompiledNode],
    visiting: &mut HashSet<usize>,
) -> Vec<FirstToken> {
    match item {
        CompiledItem::Token(k) => vec![FirstToken::Kind(*k)],
        CompiledItem::Keyword { kind, text } => {
            vec![FirstToken::Keyword {
                kind: *kind,
                text: text.clone(),
            }]
        }
        CompiledItem::Node(idx) => compute_first_tokens(nodes, *idx, visiting),
        CompiledItem::Choice(alts) => alts
            .iter()
            .flat_map(|&alt| compute_first_tokens(nodes, alt, visiting))
            .collect(),
        CompiledItem::Optional(inner) | CompiledItem::Repeat(inner) => {
            first_of_item(inner, nodes, visiting)
        }
    }
}

fn compile_symbol_rules(
    rules: &[SymbolRule],
    decls: &[NodeDecl],
    node_name_to_idx: &HashMap<&str, usize>,
    node_kinds: &[SyntaxKind],
    name_to_kind: &HashMap<&str, SyntaxKind>,
) -> Vec<CompiledSymbolRule> {
    rules
        .iter()
        .filter_map(|rule| {
            let &node_idx = node_name_to_idx.get(rule.node.as_str())?;
            let node_kind = node_kinds[node_idx];
            let decl = &decls[node_idx];
            let field_kind = find_field_kind(
                &rule.field,
                &decl.items,
                node_name_to_idx,
                node_kinds,
                name_to_kind,
            )?;
            let symbol_kind = parse_symbol_kind(&rule.kind)?;
            Some(CompiledSymbolRule {
                node_kind,
                field_kind,
                symbol_kind,
            })
        })
        .collect()
}

fn find_field_kind(
    label: &str,
    items: &[GrammarItem],
    node_name_to_idx: &HashMap<&str, usize>,
    node_kinds: &[SyntaxKind],
    name_to_kind: &HashMap<&str, SyntaxKind>,
) -> Option<SyntaxKind> {
    items.iter().find_map(|item| {
        find_field_kind_in_item(label, item, node_name_to_idx, node_kinds, name_to_kind)
    })
}

fn find_field_kind_in_item(
    label: &str,
    item: &GrammarItem,
    node_name_to_idx: &HashMap<&str, usize>,
    node_kinds: &[SyntaxKind],
    name_to_kind: &HashMap<&str, SyntaxKind>,
) -> Option<SyntaxKind> {
    match item {
        GrammarItem::Field { label: l, ty } if l == label => {
            if let Some(&idx) = node_name_to_idx.get(ty.as_str()) {
                Some(node_kinds[idx])
            } else {
                name_to_kind.get(ty.as_str()).copied()
            }
        }
        GrammarItem::Optional(inner) | GrammarItem::Repeat(inner) => {
            find_field_kind_in_item(label, inner, node_name_to_idx, node_kinds, name_to_kind)
        }
        _ => None,
    }
}

fn parse_symbol_kind(s: &str) -> Option<SymbolKind> {
    Some(match s {
        "function" => SymbolKind::Function,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "impl" => SymbolKind::Impl,
        "module" => SymbolKind::Module,
        "constant" => SymbolKind::Constant,
        "type" => SymbolKind::TypeAlias,
        "macro" => SymbolKind::Macro,
        "section" => SymbolKind::Section,
        "heading" => SymbolKind::Heading,
        _ => return None,
    })
}

fn compile_token_decl(decl: &TokenDecl, next_id: &mut u16) -> (Pattern, SyntaxKind, bool) {
    match &decl.kind {
        TokenKind::KeywordSet(kws) => {
            let kind = SyntaxKind(*next_id);
            *next_id += 1;
            (Pattern::Keywords(kws.clone()), kind, false)
        }
        TokenKind::Regex(pat) => {
            // Trivia tokens use the shared built-in kinds so is_trivia() works.
            let (kind, is_trivia) = if decl.is_trivia {
                if pat.contains("\\s") || pat.contains("\\r") || pat.contains("\\n") {
                    (SyntaxKind::WHITESPACE, true)
                } else {
                    (SyntaxKind::COMMENT, true)
                }
            } else {
                let k = SyntaxKind(*next_id);
                *next_id += 1;
                (k, false)
            };
            (Pattern::from_regex(pat), kind, is_trivia)
        }
        TokenKind::Literal(s) => {
            let kind = SyntaxKind(*next_id);
            *next_id += 1;
            (Pattern::Literal(s.clone()), kind, false)
        }
        TokenKind::LineComment(prefix) => {
            // Line comments are trivia → COMMENT kind
            (
                Pattern::LineComment(prefix.clone()),
                SyntaxKind::COMMENT,
                true,
            )
        }
        TokenKind::BlockComment(open, close) => (
            Pattern::BlockComment(open.clone(), close.clone()),
            SyntaxKind::COMMENT,
            true,
        ),
    }
}

fn parse_highlight_kind(scope: &str) -> Option<HighlightKind> {
    let base = scope.split('.').next().unwrap_or(scope);
    Some(match base {
        "keyword" => HighlightKind::Keyword,
        "string" => HighlightKind::String,
        "number" => HighlightKind::Number,
        "comment" => HighlightKind::Comment,
        "operator" => HighlightKind::Operator,
        "punctuation" => HighlightKind::Punctuation,
        "function" => HighlightKind::Function,
        "variable" => HighlightKind::Variable,
        "type" => HighlightKind::Type,
        "attribute" => HighlightKind::Attribute,
        "macro" => HighlightKind::Macro,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_dsl::parse_spec;

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

    #[test]
    fn compiles_without_panic() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "mini-oxygen");
    }

    #[test]
    fn file_kind_is_lang_base() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        assert_eq!(cs.file_kind.0, SyntaxKind::LANG_KIND_BASE);
    }

    #[test]
    fn keyword_matcher_comes_first() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        // Keywords must be checked before ident so `fn` isn't swallowed as ident.
        let first_pattern = &cs.matchers[0].0;
        assert!(matches!(first_pattern, Pattern::Keywords(_)));
    }

    #[test]
    fn highlight_map_populated() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        assert!(cs.highlights.values().any(|&h| h == HighlightKind::Keyword));
        assert!(cs.highlights.values().any(|&h| h == HighlightKind::String));
    }

    #[test]
    fn bracket_pairs_detected() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        assert!(!cs.bracket_pairs.is_empty());
    }

    #[test]
    fn fold_brackets_flag() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        assert!(cs.fold_brackets);
    }

    #[test]
    fn whitespace_uses_shared_kind() {
        let spec = parse_spec(MINI_OX).unwrap();
        let cs = compile(&spec);
        let ws = cs.matchers.iter().find(|(_, _, trivia)| *trivia).unwrap();
        assert!(ws.1.is_trivia());
    }

    const GRAMMAR_SPEC: &str = r#"
language { id "g" extensions [".g"] }
tokens {
  keyword ["fn", "let"]
  ident /[a-zA-Z_]\w*/
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

    #[test]
    fn grammar_nodes_compiled() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        assert_eq!(cs.nodes.len(), 5);
        assert_eq!(cs.nodes[0].name, "File");
    }

    #[test]
    fn grammar_file_kind_overridden_by_first_node() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        assert_eq!(cs.file_kind, cs.nodes[0].kind);
    }

    #[test]
    fn grammar_fn_decl_first_tokens() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        let fn_decl = cs.nodes.iter().find(|n| n.name == "FnDecl").unwrap();
        assert!(fn_decl.first_tokens.iter().any(|ft| matches!(
            ft, FirstToken::Keyword { text, .. } if text == "fn"
        )));
    }

    #[test]
    fn grammar_choice_first_tokens_are_union() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        let top_decl = cs.nodes.iter().find(|n| n.name == "TopDecl").unwrap();
        let has_fn = top_decl.first_tokens.iter().any(|ft| {
            matches!(
                ft, FirstToken::Keyword { text, .. } if text == "fn"
            )
        });
        let has_let = top_decl.first_tokens.iter().any(|ft| {
            matches!(
                ft, FirstToken::Keyword { text, .. } if text == "let"
            )
        });
        assert!(has_fn && has_let);
    }

    #[test]
    fn symbol_rules_compiled() {
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        assert_eq!(cs.symbol_rules.len(), 2);
    }

    #[test]
    fn symbol_rule_fn_decl_is_function_kind() {
        use sylven::SymbolKind;
        let spec = parse_spec(GRAMMAR_SPEC).unwrap();
        let cs = compile(&spec);
        let fn_rule = cs
            .symbol_rules
            .iter()
            .find(|r| r.node_kind == cs.nodes.iter().find(|n| n.name == "FnDecl").unwrap().kind)
            .unwrap();
        assert_eq!(fn_rule.symbol_kind, SymbolKind::Function);
    }
}
