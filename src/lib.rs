//! Sylven: a recovery-first, lossless syntax engine for native editors.
//!
//! This crate is the public façade over [`sylven_text`], [`sylven_lex`],
//! [`sylven_tree`], and [`sylven_parse`]:
//!
//! - [`LanguagePlugin`] is the contract a language implements: parse a
//!   [`sylven_text::TextSnapshot`] into a [`ParseResult`].
//! - [`LanguageRegistry`] looks plugins up by [`LanguageId`].
//! - [`SyntaxEngine`] owns a registry (with built-ins, see [`lang`],
//!   pre-registered) and runs a plugin's parse.
//! - [`SyntaxSession`] is the per-document handle: it remembers the last
//!   snapshot and [`ParseResult`] for one open file.
//!
//! Stage 1 (plan.md §14) proves this pipeline end to end with one bundled
//! plugin, [`lang::mini_oxygen`], a hand-written parser for a small
//! Oxygen-like language. Later stages add typed rules/queries (replacing
//! Tree-sitter queries), a `.sylven` DSL compiled to runtime tables,
//! incremental reparsing, and an LSP bridge — all behind these same types.

mod engine;
mod language;
mod registry;
mod result;
mod session;

pub mod lang;
pub mod prelude;

pub use engine::SyntaxEngine;
pub use language::{LanguageId, LanguagePlugin};
pub use registry::LanguageRegistry;
pub use result::{Highlight, HighlightKind, ParseResult, SymbolInfo, SymbolKind, SyntaxFeatures};
pub use session::SyntaxSession;
pub use sylven_text::{DocumentId, RevisionId, TextSnapshot};
