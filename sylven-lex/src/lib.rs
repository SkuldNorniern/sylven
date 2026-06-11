//! Token kinds, lexer infrastructure, and trivia handling for the Sylven
//! syntax engine.
//!
//! [`SyntaxKind`], [`Token`], and [`TokenStream`] are generic and make no
//! assumptions about any particular language. [`mini_oxygen`] is a
//! hand-written lexer for the Stage 1 proof-of-concept language; future
//! per-language lexers move to `sylven-langs`, generated from `sylven-dsl`.

mod kind;
mod token;

pub mod mini_oxygen;
pub mod rust;

pub use kind::SyntaxKind;
pub use token::{Token, TokenStream};
