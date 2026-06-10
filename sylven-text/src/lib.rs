//! Text positions, ranges, and snapshots shared across the Sylven syntax
//! engine.
//!
//! This crate has no dependencies and no knowledge of any particular
//! language: it is the common vocabulary that [`sylven-lex`](https://docs.rs/sylven-lex),
//! [`sylven-tree`](https://docs.rs/sylven-tree), and
//! [`sylven-parse`](https://docs.rs/sylven-parse) build on.

mod edit;
mod line_index;
mod range;
mod size;
mod snapshot;

pub use edit::TextEdit;
pub use line_index::{LineCol, LineIndex};
pub use range::TextRange;
pub use size::TextSize;
pub use snapshot::{DocumentId, RevisionId, TextSnapshot};
