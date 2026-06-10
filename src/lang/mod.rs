//! Bundled language plugins.
//!
//! Stage 1 has exactly one: [`mini_oxygen`], a hand-written recursive-descent
//! parser proving the lex → parse → tree pipeline end to end (plan.md §14).
//! Once `sylven-langs`/`sylven-dsl` exist, plugins like this move there and
//! this module becomes a thin re-export.

pub mod mini_oxygen;
