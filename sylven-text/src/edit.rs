use crate::TextRange;

/// A single replace edit: delete `delete`, then insert `insert` at
/// `delete.start()`.
///
/// Insertion and pure deletion are both expressible: an empty `delete` range
/// is a pure insert, and an empty `insert` string is a pure delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub delete: TextRange,
    pub insert: String,
}

impl TextEdit {
    pub fn insert(at: crate::TextSize, text: impl Into<String>) -> TextEdit {
        TextEdit {
            delete: TextRange::at(at),
            insert: text.into(),
        }
    }

    pub fn delete(range: TextRange) -> TextEdit {
        TextEdit {
            delete: range,
            insert: String::new(),
        }
    }

    pub fn replace(range: TextRange, text: impl Into<String>) -> TextEdit {
        TextEdit {
            delete: range,
            insert: text.into(),
        }
    }

    /// Apply this edit to `text`, returning the resulting string.
    ///
    /// Panics if `delete` is out of bounds or splits a UTF-8 code point.
    pub fn apply(&self, text: &str) -> String {
        let start = self.delete.start().to_usize();
        let end = self.delete.end().to_usize();
        let mut out = String::with_capacity(text.len() - (end - start) + self.insert.len());
        out.push_str(&text[..start]);
        out.push_str(&self.insert);
        out.push_str(&text[end..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextSize;

    #[test]
    fn insert_into_middle() {
        let edit = TextEdit::insert(TextSize::from(5), ", world");
        assert_eq!(edit.apply("hello!"), "hello, world!");
    }

    #[test]
    fn replace_range() {
        let edit = TextEdit::replace(
            TextRange::new(TextSize::from(0), TextSize::from(5)),
            "goodbye",
        );
        assert_eq!(edit.apply("hello world"), "goodbye world");
    }

    #[test]
    fn delete_range() {
        let edit = TextEdit::delete(TextRange::new(TextSize::from(5), TextSize::from(11)));
        assert_eq!(edit.apply("hello world"), "hello");
    }
}
