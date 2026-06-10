//! Recovery-first event parser driver for the Sylven syntax engine.
//!
//! [`Parser`] is generic: it knows how to track lookahead over a
//! [`TokenStream`](sylven_lex::TokenStream) (skipping trivia, but never
//! discarding it) and record a [`ParseEvent`] log. It has no grammar of its
//! own — language plugins drive it with recursive-descent functions, then
//! pass the resulting events to [`build_tree`] to get a
//! [`SyntaxTree`](sylven_tree::SyntaxTree).

mod error;
mod event;
mod parser;
mod tree_builder;

pub use error::ParseError;
pub use event::{ParseEvent, TokenId};
pub use parser::{Checkpoint, Parser};
pub use tree_builder::build_tree;
