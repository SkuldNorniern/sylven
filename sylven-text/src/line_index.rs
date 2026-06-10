use crate::TextSize;

/// A 0-based (line, column) position. Both fields count UTF-8 bytes from the
/// start of the line, matching [`TextSize`]'s byte-offset semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Maps byte offsets to (line, column) positions and back.
///
/// Built once per [`TextSnapshot`](crate::TextSnapshot); recomputed whenever
/// the text changes.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first character of each line. Always starts with
    /// `TextSize::ZERO` and has at least one entry.
    line_starts: Vec<TextSize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![TextSize::ZERO];
        let mut offset = 0u32;
        for byte in text.bytes() {
            offset += 1;
            if byte == b'\n' {
                line_starts.push(TextSize::from(offset));
            }
        }
        LineIndex { line_starts }
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    pub fn line_start(&self, line: u32) -> Option<TextSize> {
        self.line_starts.get(line as usize).copied()
    }

    pub fn line_col(&self, offset: TextSize) -> LineCol {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next_line) => next_line - 1,
        };
        let col = offset - self.line_starts[line];
        LineCol {
            line: line as u32,
            col: col.to_u32(),
        }
    }

    pub fn offset(&self, pos: LineCol) -> Option<TextSize> {
        let line_start = self.line_start(pos.line)?;
        Some(line_start + TextSize::from(pos.col))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line() {
        let index = LineIndex::new("hello");
        assert_eq!(index.line_count(), 1);
        assert_eq!(
            index.line_col(TextSize::from(3)),
            LineCol { line: 0, col: 3 }
        );
    }

    #[test]
    fn multi_line_round_trip() {
        let text = "fn main() {\n    foo();\n}\n";
        let index = LineIndex::new(text);
        assert_eq!(index.line_count(), 4);

        // Offset of `foo` on line 1.
        let foo_offset = TextSize::from(text.find("foo").unwrap() as u32);
        let pos = index.line_col(foo_offset);
        assert_eq!(pos, LineCol { line: 1, col: 4 });
        assert_eq!(index.offset(pos), Some(foo_offset));
    }
}
