use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// A zero-based byte offset or length into a UTF-8 source text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextSize(u32);

impl TextSize {
    pub const ZERO: TextSize = TextSize(0);

    /// Length in bytes of `text`.
    ///
    /// Panics if `text.len()` does not fit in a `u32`; sources that large are
    /// out of scope for an editor-facing syntax tree.
    pub fn of(text: &str) -> TextSize {
        TextSize(u32::try_from(text.len()).expect("source text larger than 4 GiB"))
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }

    pub fn to_u32(self) -> u32 {
        self.0
    }
}

impl From<u32> for TextSize {
    fn from(value: u32) -> Self {
        TextSize(value)
    }
}

impl From<TextSize> for u32 {
    fn from(value: TextSize) -> Self {
        value.0
    }
}

impl Add for TextSize {
    type Output = TextSize;

    fn add(self, rhs: TextSize) -> TextSize {
        TextSize(self.0 + rhs.0)
    }
}

impl AddAssign for TextSize {
    fn add_assign(&mut self, rhs: TextSize) {
        self.0 += rhs.0;
    }
}

impl Sub for TextSize {
    type Output = TextSize;

    fn sub(self, rhs: TextSize) -> TextSize {
        TextSize(self.0 - rhs.0)
    }
}

impl SubAssign for TextSize {
    fn sub_assign(&mut self, rhs: TextSize) {
        self.0 -= rhs.0;
    }
}

impl fmt::Debug for TextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for TextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
