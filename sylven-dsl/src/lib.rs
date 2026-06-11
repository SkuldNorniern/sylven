//! `sylven-dsl` — parser for `.sylven` language specification files.
//!
//! Stage 4 of sylven's roadmap (plan.md §14): a prototype DSL that can
//! describe a language's tokens, grammar nodes, Pratt expression table,
//! recovery rules, highlight rules, fold rules, and document-symbol rules in
//! one `.sylven` file. The goal for this stage is to parse a spec that
//! describes the same behaviour as the hand-written mini-Oxygen plugin.
//!
//! Stage 5 will add a code-generator that compiles a [`SylvenSpec`] into a
//! [`sylven::LanguagePlugin`] implementation, replacing the hand-written plugins.
//!
//! # Quick start
//!
//! ```
//! let src = r#"
//! language { id "toy" extensions [".toy"] }
//! tokens  { kw ["let"] ident /[a-z]+/ }
//! grammar { node LetStmt { name:Ident } }
//! "#;
//! let spec = sylven_dsl::parse_spec(src).unwrap();
//! assert_eq!(spec.language.id, "toy");
//! ```

mod ast;
mod lexer;
mod parser;

pub use ast::{
    Assoc, FoldCondition, FoldRule, HighlightRule, HighlightSource, LanguageMeta, NodeDecl,
    NodeField, PrattInfix, PrattPrefix, PrattSpec, RecoveryRule, RecoveryStrategy, SylvenSpec,
    SymbolRule, TokenDecl, TokenKind,
};
pub use parser::{DslError, parse_spec};
