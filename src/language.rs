use sylven_text::TextSnapshot;

use crate::ParseResult;

/// A language's identity within a [`LanguageRegistry`](crate::LanguageRegistry).
///
/// Backed by a `&'static str` (e.g. `"mini-oxygen"`) rather than an enum so
/// new languages — including ones defined entirely by `sylven-dsl` — never
/// require a change to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanguageId(pub &'static str);

/// A language plugin: given a [`TextSnapshot`], produce a [`ParseResult`].
///
/// Stage 1 plugins (like `mini_oxygen`) are hand-written recursive-descent
/// parsers built on [`sylven_parse::Parser`]. Later stages add plugins
/// compiled from `.sylven` grammar files by `sylven-dsl`; both kinds
/// implement this same trait.
pub trait LanguagePlugin: Send + Sync {
    fn id(&self) -> LanguageId;

    /// Parse `snapshot.text()` from scratch.
    ///
    /// Incremental reparse (given a previous tree and an edit) is a later
    /// stage (plan.md "Incremental parsing levels 0-4"); every plugin must
    /// support full reparse regardless.
    fn parse(&self, snapshot: &TextSnapshot) -> ParseResult;
}
