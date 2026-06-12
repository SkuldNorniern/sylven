//! "Mini-Oxygen" language plugin: Sylven's Stage 1 proof-of-concept.
//!
//! A small recursive-descent parser, built on [`sylven_parse::Parser`], for
//! the C/JS-like subset of Oxygen used by `oxygen/sample/*.oxy` (function
//! declarations, `let`/`if`/`return`, calls, and arithmetic/comparison
//! expressions). It demonstrates the full lex → parse → tree pipeline,
//! including recovery from malformed input (plan.md §2.1, §14).

use sylven_lex::SyntaxKind;
use sylven_lex::mini_oxygen::{MiniOxygenKind as K, lex};
use sylven_parse::{Parser, build_tree};
use sylven_text::TextSnapshot;

use crate::{LanguageId, LanguagePlugin, ParseResult, SyntaxFeatures};

const FILE: SyntaxKind = K::File.to_syntax();
const FN_DECL: SyntaxKind = K::FnDecl.to_syntax();
const PARAM_LIST: SyntaxKind = K::ParamList.to_syntax();
const PARAM: SyntaxKind = K::Param.to_syntax();
const BLOCK: SyntaxKind = K::Block.to_syntax();
const LET_STMT: SyntaxKind = K::LetStmt.to_syntax();
const RETURN_STMT: SyntaxKind = K::ReturnStmt.to_syntax();
const IF_STMT: SyntaxKind = K::IfStmt.to_syntax();
const EXPR_STMT: SyntaxKind = K::ExprStmt.to_syntax();
const CALL_EXPR: SyntaxKind = K::CallExpr.to_syntax();
const ARG_LIST: SyntaxKind = K::ArgList.to_syntax();
const BINARY_EXPR: SyntaxKind = K::BinaryExpr.to_syntax();
const PREFIX_EXPR: SyntaxKind = K::PrefixExpr.to_syntax();
const PAREN_EXPR: SyntaxKind = K::ParenExpr.to_syntax();
const NAME: SyntaxKind = K::Name.to_syntax();
const NAME_REF: SyntaxKind = K::NameRef.to_syntax();
const LITERAL: SyntaxKind = K::Literal.to_syntax();
const ERROR_NODE: SyntaxKind = K::ErrorNode.to_syntax();

const IDENT: SyntaxKind = K::Ident.to_syntax();
const INT_NUMBER: SyntaxKind = K::IntNumber.to_syntax();
const STRING: SyntaxKind = K::String.to_syntax();
const FN_KW: SyntaxKind = K::FnKw.to_syntax();
const LET_KW: SyntaxKind = K::LetKw.to_syntax();
const IF_KW: SyntaxKind = K::IfKw.to_syntax();
const ELSE_KW: SyntaxKind = K::ElseKw.to_syntax();
const RETURN_KW: SyntaxKind = K::ReturnKw.to_syntax();
const L_PAREN: SyntaxKind = K::LParen.to_syntax();
const R_PAREN: SyntaxKind = K::RParen.to_syntax();
const L_BRACE: SyntaxKind = K::LBrace.to_syntax();
const R_BRACE: SyntaxKind = K::RBrace.to_syntax();
const COMMA: SyntaxKind = K::Comma.to_syntax();
const SEMICOLON: SyntaxKind = K::Semicolon.to_syntax();
const EQ: SyntaxKind = K::Eq.to_syntax();
const EQEQ: SyntaxKind = K::EqEq.to_syntax();
const NEQ: SyntaxKind = K::Neq.to_syntax();
const LT: SyntaxKind = K::Lt.to_syntax();
const LE: SyntaxKind = K::Le.to_syntax();
const GT: SyntaxKind = K::Gt.to_syntax();
const GE: SyntaxKind = K::Ge.to_syntax();
const PLUS: SyntaxKind = K::Plus.to_syntax();
const MINUS: SyntaxKind = K::Minus.to_syntax();
const STAR: SyntaxKind = K::Star.to_syntax();
const SLASH: SyntaxKind = K::Slash.to_syntax();
const BANG: SyntaxKind = K::Bang.to_syntax();

/// The Mini-Oxygen [`LanguagePlugin`].
pub struct MiniOxygen;

impl LanguagePlugin for MiniOxygen {
    fn id(&self) -> LanguageId {
        LanguageId("mini-oxygen")
    }

    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult {
        let source = snapshot.text();
        let stream = lex(source);
        let tokens = stream.as_slice();

        let mut parser = Parser::new(tokens);
        parse_file(&mut parser);
        let events = parser.finish();

        let (tree, errors) = build_tree(tokens, source, events);
        ParseResult {
            tree,
            errors,
            features: SyntaxFeatures::default(),
        }
    }
}

/// Tokens that can begin an expression. Used to decide, at statement
/// boundaries, whether to recurse into [`parse_expr`] or instead emit a
/// recovery [`ERROR_NODE`].
fn starts_expr(kind: SyntaxKind) -> bool {
    matches!(kind, INT_NUMBER | STRING | IDENT | L_PAREN | MINUS | BANG)
}

/// Left/right binding power for a binary operator, or `None` if `kind` isn't
/// one. `rbp == lbp + 1` for every operator, which makes every level
/// left-associative (see [`parse_expr_bp`]).
fn binary_binding_power(kind: SyntaxKind) -> Option<(u8, u8)> {
    match kind {
        EQEQ | NEQ => Some((1, 2)),
        LT | LE | GT | GE => Some((3, 4)),
        PLUS | MINUS => Some((5, 6)),
        STAR | SLASH => Some((7, 8)),
        _ => None,
    }
}

fn parse_file(p: &mut Parser) {
    p.start_node(FILE);
    while !p.at_eof() {
        parse_stmt(p);
    }
    // Trailing trivia (e.g. a final newline) belongs in the tree too, or the
    // tree wouldn't losslessly cover the whole source.
    p.eat_trailing_trivia();
    p.finish_node();
}

fn parse_stmt(p: &mut Parser) {
    match p.current() {
        LET_KW => parse_let_stmt(p),
        RETURN_KW => parse_return_stmt(p),
        IF_KW => parse_if_stmt(p),
        FN_KW => parse_fn_decl(p),
        L_BRACE => parse_block(p),
        _ => parse_expr_stmt(p),
    }
}

fn parse_let_stmt(p: &mut Parser) {
    p.start_node(LET_STMT);
    p.bump(); // 'let'
    if p.at(IDENT) {
        p.start_node(NAME);
        p.bump();
        p.finish_node();
    } else {
        p.error("expected a name after 'let'");
    }
    p.expect(EQ);
    parse_expr(p);
    p.expect(SEMICOLON);
    p.finish_node();
}

fn parse_return_stmt(p: &mut Parser) {
    p.start_node(RETURN_STMT);
    p.bump(); // 'return'
    if starts_expr(p.current()) {
        parse_expr(p);
    }
    p.expect(SEMICOLON);
    p.finish_node();
}

fn parse_if_stmt(p: &mut Parser) {
    p.start_node(IF_STMT);
    p.bump(); // 'if'
    p.expect(L_PAREN);
    parse_expr(p);
    p.expect(R_PAREN);
    parse_block(p);
    if p.at(ELSE_KW) {
        p.bump();
        if p.at(IF_KW) {
            parse_if_stmt(p);
        } else {
            parse_block(p);
        }
    }
    p.finish_node();
}

fn parse_fn_decl(p: &mut Parser) {
    p.start_node(FN_DECL);
    p.bump(); // 'fn'
    if p.at(IDENT) {
        p.start_node(NAME);
        p.bump();
        p.finish_node();
    } else {
        p.error("expected a function name after 'fn'");
    }
    parse_param_list(p);
    parse_block(p);
    p.finish_node();
}

fn parse_param_list(p: &mut Parser) {
    p.start_node(PARAM_LIST);
    p.expect(L_PAREN);
    while p.at(IDENT) {
        p.start_node(PARAM);
        p.bump();
        p.finish_node();
        if p.at(COMMA) {
            p.bump();
        } else {
            break;
        }
    }
    p.expect(R_PAREN);
    p.finish_node();
}

fn parse_block(p: &mut Parser) {
    p.start_node(BLOCK);
    p.expect(L_BRACE);
    while !p.at(R_BRACE) && !p.at_eof() {
        parse_stmt(p);
    }
    p.expect(R_BRACE);
    p.finish_node();
}

fn parse_expr_stmt(p: &mut Parser) {
    if !starts_expr(p.current()) {
        // Recovery: the current token can't start anything we recognize as
        // a statement. Wrap it alone in an ERROR node — guaranteeing
        // progress — and let the caller's loop try again from the next
        // token.
        p.start_node(ERROR_NODE);
        p.error(format!("unexpected token {:?}", p.current()));
        p.bump();
        p.finish_node();
        return;
    }

    let checkpoint = p.checkpoint();
    parse_expr(p);
    p.start_node_at(checkpoint, EXPR_STMT);
    p.expect(SEMICOLON);
    p.finish_node();
}

fn parse_expr(p: &mut Parser) {
    parse_expr_bp(p, 0);
}

/// Pratt/precedence-climbing expression parser. `min_bp` is the minimum left
/// binding power an operator must have to be consumed at this level.
fn parse_expr_bp(p: &mut Parser, min_bp: u8) {
    let checkpoint = p.checkpoint();
    parse_unary(p);

    while let Some((lbp, rbp)) = binary_binding_power(p.current()) {
        if lbp < min_bp {
            break;
        }
        p.bump(); // operator
        p.start_node_at(checkpoint, BINARY_EXPR);
        parse_expr_bp(p, rbp);
        p.finish_node();
    }
}

fn parse_unary(p: &mut Parser) {
    if matches!(p.current(), MINUS | BANG) {
        p.start_node(PREFIX_EXPR);
        p.bump();
        parse_unary(p);
        p.finish_node();
    } else {
        parse_atom(p);
    }
}

fn parse_atom(p: &mut Parser) {
    match p.current() {
        INT_NUMBER | STRING => {
            p.start_node(LITERAL);
            p.bump();
            p.finish_node();
        }
        IDENT => {
            let checkpoint = p.checkpoint();
            p.start_node(NAME_REF);
            p.bump();
            p.finish_node();
            if p.at(L_PAREN) {
                p.start_node_at(checkpoint, CALL_EXPR);
                parse_arg_list(p);
                p.finish_node();
            }
        }
        L_PAREN => {
            p.start_node(PAREN_EXPR);
            p.bump();
            parse_expr(p);
            p.expect(R_PAREN);
            p.finish_node();
        }
        _ => {
            // Recovery: an expression was expected but the current token
            // can't start one. Wrap it in an ERROR node (consuming it, so
            // the caller always makes progress) rather than aborting.
            p.start_node(ERROR_NODE);
            p.error(format!("expected an expression, found {:?}", p.current()));
            if !p.at_eof() {
                p.bump();
            }
            p.finish_node();
        }
    }
}

fn parse_arg_list(p: &mut Parser) {
    p.start_node(ARG_LIST);
    p.expect(L_PAREN);
    while !p.at(R_PAREN) && !p.at_eof() {
        parse_expr(p);
        if p.at(COMMA) {
            p.bump();
        } else {
            break;
        }
    }
    p.expect(R_PAREN);
    p.finish_node();
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven_text::{DocumentId, RevisionId};

    fn parse(source: &str) -> ParseResult {
        let snapshot = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
        MiniOxygen.parse(&snapshot)
    }

    #[test]
    fn id_is_mini_oxygen() {
        assert_eq!(MiniOxygen.id(), LanguageId("mini-oxygen"));
    }

    #[test]
    fn parses_helloworld_sample_without_errors() {
        let source = include_str!("../../../oxygen/sample/helloworld.oxy");
        let result = parse(source);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert_eq!(result.tree.text(), source);
        assert_eq!(result.tree.root().kind(), FILE);
    }

    #[test]
    fn parses_fibonacci_sample_without_errors() {
        let source = include_str!("../../../oxygen/sample/fibonacci.oxy");
        let result = parse(source);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert_eq!(result.tree.text(), source);

        // Sanity-check the tree shape: one top-level FnDecl (`fib`) among
        // the top-level ExprStmts (the `print(...)` calls).
        let fn_decls = result
            .tree
            .root()
            .children()
            .filter(|node| node.kind() == FN_DECL)
            .count();
        assert_eq!(fn_decls, 1);
    }

    #[test]
    fn binary_expressions_are_left_associative() {
        // `1 + 2 + 3` should parse as `(1 + 2) + 3`: the outer BinaryExpr's
        // first child is itself a BinaryExpr, not a Literal.
        let result = parse("1 + 2 + 3;");
        assert!(result.errors.is_empty());

        let file = result.tree.root();
        let expr_stmt = file.children().next().expect("an ExprStmt");
        assert_eq!(expr_stmt.kind(), EXPR_STMT);

        let outer = expr_stmt.children().next().expect("outer BinaryExpr");
        assert_eq!(outer.kind(), BINARY_EXPR);

        let inner = outer.children().next().expect("inner BinaryExpr");
        assert_eq!(inner.kind(), BINARY_EXPR);
    }

    #[test]
    fn recovers_from_malformed_function() {
        // Missing `)` after the parameter list, and a missing `;` after
        // `let x = 1`. The parser must still produce a tree that losslessly
        // covers the whole source, plus diagnostics pointing at both
        // problems.
        let source = "fn main( {\n    let x = 1\n}\n";
        let result = parse(source);

        // Lossless no matter how broken the input is.
        assert_eq!(result.tree.text(), source);

        assert!(
            result.errors.len() >= 2,
            "expected at least 2 errors, got {:?}",
            result.errors
        );

        let file = result.tree.root();
        let fn_decl = file.children().next().expect("a FnDecl");
        assert_eq!(fn_decl.kind(), FN_DECL);

        // The FnDecl still has a Block child, recovered after the missing
        // `)`.
        let has_block = fn_decl.children().any(|node| node.kind() == BLOCK);
        assert!(has_block, "FnDecl should still contain a Block");
    }

    #[test]
    fn unexpected_top_level_token_becomes_error_node() {
        let source = "@ let x = 1;";
        let result = parse(source);

        assert_eq!(result.tree.text(), source);
        assert!(!result.errors.is_empty());

        let file = result.tree.root();
        let mut children = file.children();
        assert_eq!(children.next().unwrap().kind(), ERROR_NODE);
        assert_eq!(children.next().unwrap().kind(), LET_STMT);
    }
}
