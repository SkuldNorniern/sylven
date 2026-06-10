//! Common imports for working with Sylven:
//!
//! ```
//! use sylven::prelude::*;
//! ```

pub use crate::{
    LanguageId, LanguagePlugin, LanguageRegistry, ParseResult, SyntaxEngine, SyntaxFeatures,
    SyntaxSession,
};
pub use sylven_lex::{SyntaxKind, Token, TokenStream};
pub use sylven_parse::{ParseError, ParseEvent, Parser};
pub use sylven_text::{DocumentId, RevisionId, TextRange, TextSize, TextSnapshot};
pub use sylven_tree::{SyntaxElement, SyntaxNode, SyntaxToken, SyntaxTree};
