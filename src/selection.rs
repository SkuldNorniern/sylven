use sylven_text::{TextRange, TextSize};

use crate::result::SyntaxFeatures;

/// Expand `(sel_start, sel_end)` byte offsets to the smallest structural range
/// that strictly contains the current selection.
///
/// Candidates are drawn from bracket-pair spans (open.start → close.end) and
/// fold ranges. The whole-document range is included as the ultimate backstop.
/// Returns `None` when the selection already covers the whole document or the
/// document has no structural markers.
pub fn expand_selection(
    features: &SyntaxFeatures,
    source_len: usize,
    sel_start: usize,
    sel_end: usize,
) -> Option<(usize, usize)> {
    let current = TextRange::new(
        TextSize::from(sel_start as u32),
        TextSize::from(sel_end as u32),
    );
    let whole_doc = TextRange::new(TextSize::from(0u32), TextSize::from(source_len as u32));

    let mut best: Option<TextRange> = None;

    let mut accept = |candidate: TextRange| {
        if candidate.contains_range(current) && candidate != current {
            best = Some(match best {
                None => candidate,
                Some(prev) => {
                    if candidate.len() < prev.len() {
                        candidate
                    } else {
                        prev
                    }
                }
            });
        }
    };

    for (open, close) in &features.brackets {
        accept(TextRange::new(open.start(), close.end()));
    }
    for &fold in &features.folds {
        accept(fold);
    }
    accept(whole_doc);

    best.map(|r| (r.start().to_usize(), r.end().to_usize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::SyntaxFeatures;

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::from(start), TextSize::from(end))
    }

    fn feats_brackets(pairs: &[(u32, u32, u32, u32)]) -> SyntaxFeatures {
        SyntaxFeatures {
            brackets: pairs
                .iter()
                .map(|&(os, oe, cs, ce)| (range(os, oe), range(cs, ce)))
                .collect(),
            ..Default::default()
        }
    }

    fn feats_folds(folds: &[(u32, u32)]) -> SyntaxFeatures {
        SyntaxFeatures {
            folds: folds.iter().map(|&(s, e)| range(s, e)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn no_features_expands_to_whole_doc() {
        let f = SyntaxFeatures::default();
        assert_eq!(expand_selection(&f, 10, 3, 3), Some((0, 10)));
    }

    #[test]
    fn cursor_inside_bracket_expands_to_it() {
        // `x(a+b)y` — paren open=1..2, close=6..7, span=1..7, doc_len=8
        let f = feats_brackets(&[(1, 2, 6, 7)]);
        assert_eq!(expand_selection(&f, 8, 3, 3), Some((1, 7)));
    }

    #[test]
    fn nested_brackets_expand_innermost_first() {
        // `((x))` len=5: outer open=0..1 close=4..5 span=0..5;
        //                inner open=1..2 close=3..4 span=1..4
        let f = feats_brackets(&[(0, 1, 4, 5), (1, 2, 3, 4)]);
        // cursor at 2 (on 'x') — innermost containing = 1..4
        assert_eq!(expand_selection(&f, 5, 2, 2), Some((1, 4)));
    }

    #[test]
    fn inner_bracket_span_expands_to_outer() {
        // Same `((x))` — selection already covering inner span 1..4
        let f = feats_brackets(&[(0, 1, 4, 5), (1, 2, 3, 4)]);
        assert_eq!(expand_selection(&f, 5, 1, 4), Some((0, 5)));
    }

    #[test]
    fn fold_used_as_candidate() {
        let f = feats_folds(&[(2, 8)]);
        assert_eq!(expand_selection(&f, 10, 4, 4), Some((2, 8)));
    }

    #[test]
    fn already_at_whole_doc_returns_none() {
        let f = SyntaxFeatures::default();
        assert_eq!(expand_selection(&f, 10, 0, 10), None);
    }

    #[test]
    fn selection_equal_to_bracket_skips_to_outer() {
        // `x(a)y` len=5: paren open=1..2, close=3..4, span=1..4
        // Selection already covers 1..4 → skip it, fall back to whole-doc 0..5.
        let f = feats_brackets(&[(1, 2, 3, 4)]);
        assert_eq!(expand_selection(&f, 5, 1, 4), Some((0, 5)));
    }
}
