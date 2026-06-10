use std::fmt;

use crate::TextSize;

/// A half-open `[start, end)` byte range into a UTF-8 source text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRange {
    start: TextSize,
    end: TextSize,
}

impl TextRange {
    pub const EMPTY: TextRange = TextRange {
        start: TextSize::ZERO,
        end: TextSize::ZERO,
    };

    /// Panics if `end < start`.
    pub fn new(start: TextSize, end: TextSize) -> TextRange {
        assert!(start <= end, "TextRange::new: start {start} > end {end}");
        TextRange { start, end }
    }

    /// A zero-length range at `offset`.
    pub fn at(offset: TextSize) -> TextRange {
        TextRange {
            start: offset,
            end: offset,
        }
    }

    pub fn start(self) -> TextSize {
        self.start
    }

    pub fn end(self) -> TextSize {
        self.end
    }

    pub fn len(self) -> TextSize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Re-anchor this range so it starts at `self.start() + offset` instead
    /// of `self.start()`. Used to convert a child's range, recorded relative
    /// to its parent, into an absolute range.
    pub fn shift(self, offset: TextSize) -> TextRange {
        TextRange {
            start: self.start + offset,
            end: self.end + offset,
        }
    }

    pub fn contains(self, offset: TextSize) -> bool {
        self.start <= offset && offset < self.end
    }

    pub fn contains_inclusive(self, offset: TextSize) -> bool {
        self.start <= offset && offset <= self.end
    }

    pub fn contains_range(self, other: TextRange) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The smallest range that contains both `self` and `other`.
    pub fn cover(self, other: TextRange) -> TextRange {
        TextRange {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Debug for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl fmt::Display for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
