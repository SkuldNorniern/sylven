//! Rust language plugin for Sylven — Stage 1 (lexer-backed).
//!
//! Uses [`lex_rust`] to produce a flat lossless [`SyntaxTree`] and derives
//! [`SyntaxFeatures`] (highlights, folds, symbols, bracket pairs) directly
//! from the token stream, without a grammar-driven parse yet. A full
//! recursive-descent Rust parser replaces this in Stage 2.

use sylven_lex::SyntaxKind;
use sylven_lex::rust::{RustKind, lex_rust};
use sylven_parse::{ParseEvent, TokenId, build_tree};
use sylven_text::{LineIndex, TextRange, TextSize, TextSnapshot};

use crate::{
    LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures,
    result::{Highlight, HighlightKind, SymbolInfo, SymbolKind},
};

/// Node kind for the flat root FILE node.
const FILE: SyntaxKind = SyntaxKind(SyntaxKind::LANG_KIND_BASE);

/// The Rust [`LanguagePlugin`].
pub struct RustLanguage;

impl LanguagePlugin for RustLanguage {
    fn id(&self) -> LanguageId {
        LanguageId("rust")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let text = snapshot.text();
        let stream = lex_rust(text);
        let tokens = stream.as_slice();

        // Flat tree: FILE node wrapping every token as a direct leaf.
        // A grammar-driven parser will replace this in Stage 2.
        let mut events = Vec::with_capacity(tokens.len() + 2);
        events.push(ParseEvent::StartNode(FILE));
        for (i, _) in tokens.iter().enumerate() {
            events.push(ParseEvent::Token(TokenId(i as u32)));
        }
        events.push(ParseEvent::FinishNode);

        let (tree, errors) = build_tree(tokens, text, events);
        let features = derive_features(tokens, text);
        ParseResult { tree, errors, features }
    }
}

// ---------------------------------------------------------------------------
// Feature derivation from the flat token stream
// ---------------------------------------------------------------------------

fn token_text<'a>(tok: &sylven_lex::Token, source: &'a str) -> &'a str {
    let s = tok.range.start().to_usize();
    let e = tok.range.end().to_usize();
    &source[s..e]
}

fn derive_features(tokens: &[sylven_lex::Token], source: &str) -> SyntaxFeatures {
    SyntaxFeatures {
        highlights: derive_highlights(tokens, source),
        folds: derive_folds(tokens, source),
        symbols: derive_symbols(tokens, source),
        injections: Vec::new(),
        brackets: derive_brackets(tokens, source),
    }
}

// ---------------------------------------------------------------------------
// Highlights — map RustKind → HighlightKind
// ---------------------------------------------------------------------------

fn rust_kind_to_highlight(k: SyntaxKind) -> Option<HighlightKind> {
    if k == RustKind::Keyword.to_syntax() {
        return Some(HighlightKind::Keyword);
    }
    if k == RustKind::KeywordControl.to_syntax() {
        return Some(HighlightKind::KeywordControl);
    }
    if k == RustKind::PrimitiveType.to_syntax() || k == RustKind::StdType.to_syntax() {
        return Some(HighlightKind::Type);
    }
    if k == RustKind::BoolLit.to_syntax() || k == RustKind::NumberLit.to_syntax() {
        return Some(HighlightKind::Number);
    }
    if k == RustKind::Lifetime.to_syntax() {
        return Some(HighlightKind::Lifetime);
    }
    if k == RustKind::StringLit.to_syntax() || k == RustKind::CharLit.to_syntax() {
        return Some(HighlightKind::String);
    }
    if k == RustKind::LineComment.to_syntax() || k == RustKind::BlockComment.to_syntax() {
        return Some(HighlightKind::Comment);
    }
    if k == RustKind::MacroIdent.to_syntax() {
        return Some(HighlightKind::Macro);
    }
    if k == RustKind::Attribute.to_syntax() {
        return Some(HighlightKind::Attribute);
    }
    if k == RustKind::FunctionIdent.to_syntax() {
        return Some(HighlightKind::Function);
    }
    if k == RustKind::PascalIdent.to_syntax() {
        return Some(HighlightKind::Type);
    }
    if k == RustKind::Operator.to_syntax() {
        return Some(HighlightKind::Operator);
    }
    if k == RustKind::Punctuation.to_syntax() {
        return Some(HighlightKind::Punctuation);
    }
    if k == RustKind::Ident.to_syntax() {
        return Some(HighlightKind::Variable);
    }
    None // whitespace, EOF, ERROR
}

fn derive_highlights(tokens: &[sylven_lex::Token], _source: &str) -> Vec<Highlight> {
    tokens
        .iter()
        .filter_map(|tok| {
            let kind = rust_kind_to_highlight(tok.kind)?;
            Some(Highlight { range: tok.range, kind })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Folds — { / } pairs that span at least one line boundary
// ---------------------------------------------------------------------------

fn derive_folds(tokens: &[sylven_lex::Token], source: &str) -> Vec<TextRange> {
    let line_index = LineIndex::new(source);
    let mut stack: Vec<TextSize> = Vec::new();
    let mut folds = Vec::new();

    for tok in tokens {
        if tok.kind != RustKind::Punctuation.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        let e = tok.range.end().to_usize();
        if e > source.len() || s >= e {
            continue;
        }
        let ch = source.as_bytes()[s];
        if ch == b'{' {
            stack.push(tok.range.start());
        } else if ch == b'}' {
            if let Some(open_start) = stack.pop() {
                let open_line = line_index.line_col(open_start).line;
                let close_line = line_index.line_col(tok.range.start()).line;
                if close_line > open_line {
                    folds.push(TextRange::new(open_start, tok.range.end()));
                }
            }
        }
    }
    folds
}

// ---------------------------------------------------------------------------
// Symbols — keyword-sequence heuristic over the token stream
// ---------------------------------------------------------------------------

fn is_significant(k: SyntaxKind) -> bool {
    k != SyntaxKind::WHITESPACE && k != SyntaxKind::COMMENT && k != SyntaxKind::EOF
}

fn is_name_token(k: SyntaxKind) -> bool {
    k == RustKind::Ident.to_syntax()
        || k == RustKind::FunctionIdent.to_syntax()
        || k == RustKind::PascalIdent.to_syntax()
        || k == RustKind::StdType.to_syntax()
        || k == RustKind::PrimitiveType.to_syntax()
}

fn derive_symbols(tokens: &[sylven_lex::Token], source: &str) -> Vec<SymbolInfo> {
    let sig: Vec<&sylven_lex::Token> = tokens
        .iter()
        .filter(|t| is_significant(t.kind))
        .collect();

    let mut symbols = Vec::new();
    let mut i = 0;

    while i < sig.len() {
        let tok = sig[i];
        let text = token_text(tok, source);

        // Determine if this keyword opens a named declaration.
        let sym_kind: Option<SymbolKind> = if tok.kind == RustKind::Keyword.to_syntax() {
            match text {
                "fn" => Some(SymbolKind::Function),
                "struct" => Some(SymbolKind::Struct),
                "enum" => Some(SymbolKind::Enum),
                "trait" => Some(SymbolKind::Trait),
                "impl" => Some(SymbolKind::Impl),
                "mod" => Some(SymbolKind::Module),
                "const" | "static" => Some(SymbolKind::Constant),
                "type" => Some(SymbolKind::TypeAlias),
                _ => None,
            }
        } else if tok.kind == RustKind::MacroIdent.to_syntax()
            && text == "macro_rules!"
        {
            Some(SymbolKind::Macro)
        } else {
            None
        };

        if let Some(kind) = sym_kind {
            // For `impl`, the next token is the type name — could be PascalIdent,
            // StdType, or Ident. Accept any name-like token.
            // For `macro_rules!`, the next token is the macro name (Ident).
            if let Some(name_tok) = sig.get(i + 1).copied() {
                if is_name_token(name_tok.kind) {
                    let name = token_text(name_tok, source).to_string();
                    symbols.push(SymbolInfo {
                        name,
                        name_range: name_tok.range,
                        decl_range: tok.range,
                        kind,
                    });
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    symbols
}

// ---------------------------------------------------------------------------
// Brackets — matching { } [ ] ( ) pairs
// ---------------------------------------------------------------------------

fn derive_brackets(tokens: &[sylven_lex::Token], source: &str) -> Vec<(TextRange, TextRange)> {
    let mut curly: Vec<TextRange> = Vec::new();
    let mut square: Vec<TextRange> = Vec::new();
    let mut paren: Vec<TextRange> = Vec::new();
    let mut pairs = Vec::new();

    for tok in tokens {
        if tok.kind != RustKind::Punctuation.to_syntax() {
            continue;
        }
        let s = tok.range.start().to_usize();
        if s >= source.len() {
            continue;
        }
        match source.as_bytes()[s] {
            b'{' => curly.push(tok.range),
            b'[' => square.push(tok.range),
            b'(' => paren.push(tok.range),
            b'}' => {
                if let Some(open) = curly.pop() {
                    pairs.push((open, tok.range));
                }
            }
            b']' => {
                if let Some(open) = square.pop() {
                    pairs.push((open, tok.range));
                }
            }
            b')' => {
                if let Some(open) = paren.pop() {
                    pairs.push((open, tok.range));
                }
            }
            _ => {}
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::{DocumentId, RevisionId};

    fn parse(source: &str) -> ParseResult {
        let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
        RustLanguage.parse(&snap)
    }

    fn features(source: &str) -> SyntaxFeatures {
        parse(source).features
    }

    #[test]
    fn id_is_rust() {
        assert_eq!(RustLanguage.id(), LanguageId("rust"));
    }

    #[test]
    fn lossless_tree() {
        let src = "fn main() {}\n";
        let r = parse(src);
        assert_eq!(r.tree.text(), src);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn highlights_contain_keyword() {
        let f = features("fn main() {}");
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::Keyword));
    }

    #[test]
    fn highlights_contain_function() {
        let f = features("fn main() {}");
        assert!(f.highlights.iter().any(|h| h.kind == HighlightKind::Function));
    }

    #[test]
    fn fold_on_multiline_block() {
        let src = "fn f() {\n    let x = 1;\n}\n";
        let f = features(src);
        assert!(!f.folds.is_empty(), "expected at least one fold");
    }

    #[test]
    fn no_fold_on_single_line() {
        let f = features("fn f() {}");
        assert!(f.folds.is_empty(), "single-line block should not fold");
    }

    #[test]
    fn symbol_function() {
        let f = features("fn my_func() {}");
        assert!(
            f.symbols.iter().any(|s| s.name == "my_func" && s.kind == SymbolKind::Function),
            "expected function symbol 'my_func', got {:?}",
            f.symbols
        );
    }

    #[test]
    fn symbol_struct() {
        let f = features("struct Foo {}");
        assert!(f.symbols.iter().any(|s| s.name == "Foo" && s.kind == SymbolKind::Struct));
    }

    #[test]
    fn symbol_enum() {
        let f = features("enum Direction { North, South }");
        assert!(f.symbols.iter().any(|s| s.name == "Direction" && s.kind == SymbolKind::Enum));
    }

    #[test]
    fn symbol_trait() {
        let f = features("trait Display {}");
        assert!(f.symbols.iter().any(|s| s.name == "Display" && s.kind == SymbolKind::Trait));
    }

    #[test]
    fn symbol_impl() {
        let f = features("impl Foo {}");
        assert!(f.symbols.iter().any(|s| s.name == "Foo" && s.kind == SymbolKind::Impl));
    }

    #[test]
    fn symbol_mod() {
        let f = features("mod utils {}");
        assert!(f.symbols.iter().any(|s| s.name == "utils" && s.kind == SymbolKind::Module));
    }

    #[test]
    fn brackets_matched() {
        let f = features("fn f() { let x = [1, 2]; }");
        assert!(!f.brackets.is_empty());
    }

    #[test]
    fn multiple_symbols() {
        let src = "fn foo() {}\nstruct Bar {}\nenum Baz {}\n";
        let f = features(src);
        assert!(f.symbols.iter().any(|s| s.kind == SymbolKind::Function));
        assert!(f.symbols.iter().any(|s| s.kind == SymbolKind::Struct));
        assert!(f.symbols.iter().any(|s| s.kind == SymbolKind::Enum));
    }
}
