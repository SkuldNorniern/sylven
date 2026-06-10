use std::sync::Arc;

use crate::{LineIndex, TextSize};

/// Identifies a single open document for the lifetime of a [`SyntaxSession`]
/// (rendered here as an opaque handle so the engine can be the source of
/// truth for assigning ids).
///
/// [`SyntaxSession`]: https://docs.rs/sylven (root crate)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(pub u64);

/// Monotonically increasing revision counter for a document. Bumped on every
/// edit; used to detect stale parse results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RevisionId(pub u64);

impl RevisionId {
    pub fn next(self) -> RevisionId {
        RevisionId(self.0 + 1)
    }
}

/// An immutable, cheaply-cloneable view of a document's text at a given
/// [`RevisionId`], plus its [`LineIndex`].
///
/// Snapshots are the unit of input to the syntax engine: a parse always runs
/// against one snapshot and produces a tree tagged with that snapshot's
/// `(document, revision)`.
#[derive(Debug, Clone)]
pub struct TextSnapshot {
    document: DocumentId,
    revision: RevisionId,
    text: Arc<str>,
    line_index: Arc<LineIndex>,
}

impl TextSnapshot {
    pub fn new(document: DocumentId, revision: RevisionId, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let line_index = Arc::new(LineIndex::new(&text));
        TextSnapshot {
            document,
            revision,
            text,
            line_index,
        }
    }

    pub fn document_id(&self) -> DocumentId {
        self.document
    }

    pub fn revision(&self) -> RevisionId {
        self.revision
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> TextSize {
        TextSize::of(&self.text)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }
}
