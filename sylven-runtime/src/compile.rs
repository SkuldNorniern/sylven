use std::collections::HashMap;

use sylven::HighlightKind;
use sylven_lex::SyntaxKind;

use sylven_dsl::{FoldCondition, HighlightSource, SylvenSpec, TokenDecl, TokenKind};

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

    CompiledSpec {
        lang_id: spec.language.id.clone(),
        matchers,
        file_kind,
        highlights,
        bracket_pairs,
        fold_brackets,
    }
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
}
