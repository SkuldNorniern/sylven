//! Lossless green/red syntax trees for the Sylven syntax engine.
//!
//! - The **green tree** ([`GreenNode`], [`GreenToken`], [`GreenElement`]) is
//!   immutable, reference-counted, and stores only relative (child-local)
//!   text lengths, so unchanged subtrees are cheap to share.
//! - The **red tree** ([`SyntaxNode`], [`SyntaxToken`], [`SyntaxElement`]) is
//!   a position-annotated view over a green tree, computed on demand.
//! - [`GreenNodeBuilder`] is how a parser assembles a green tree bottom-up.
//! - [`SyntaxTree`] bundles a green root with the red view over it.

mod builder;
mod green;
mod red;
mod tree;

pub use builder::GreenNodeBuilder;
pub use green::{GreenElement, GreenNode, GreenToken};
pub use red::{Preorder, SyntaxElement, SyntaxNode, SyntaxToken};
pub use tree::SyntaxTree;
