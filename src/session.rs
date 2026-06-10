use std::sync::Arc;

use sylven_text::TextSnapshot;

use crate::{LanguageId, ParseResult, SyntaxEngine};

/// A document's connection to the syntax engine: pairs a [`LanguageId`] with
/// the most recently parsed [`TextSnapshot`] and [`ParseResult`].
///
/// Stage 1 always reparses from scratch in [`SyntaxSession::parse`].
/// Incremental reuse between revisions (plan.md "Incremental parsing levels
/// 0-4") is a later stage and will live behind this same method, so callers
/// don't need to change.
pub struct SyntaxSession {
    engine: Arc<SyntaxEngine>,
    language: LanguageId,
    snapshot: Option<TextSnapshot>,
    result: Option<ParseResult>,
}

impl SyntaxSession {
    pub fn new(engine: Arc<SyntaxEngine>, language: LanguageId) -> SyntaxSession {
        SyntaxSession {
            engine,
            language,
            snapshot: None,
            result: None,
        }
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    /// Parse `snapshot`, replacing any previous result, and return the new
    /// one.
    ///
    /// Returns `None` if no plugin is registered for this session's
    /// language; the previous result (if any) is left in place.
    pub fn parse(&mut self, snapshot: TextSnapshot) -> Option<&ParseResult> {
        let result = self.engine.parse(self.language, &snapshot)?;
        self.snapshot = Some(snapshot);
        self.result = Some(result);
        self.result.as_ref()
    }

    /// The most recent parse result, if [`Self::parse`] has succeeded at
    /// least once.
    pub fn current(&self) -> Option<&ParseResult> {
        self.result.as_ref()
    }

    /// The snapshot the current result was parsed from.
    pub fn snapshot(&self) -> Option<&TextSnapshot> {
        self.snapshot.as_ref()
    }
}
