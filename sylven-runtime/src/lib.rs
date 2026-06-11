//! Runtime executor for `.sylven` language specs.
//!
//! Converts a parsed [`SylvenSpec`](sylven_dsl::SylvenSpec) into a
//! [`RuntimePlugin`] that implements [`LanguagePlugin`] — without any
//! hand-written grammar. The plugin produces a flat token tree, then derives
//! highlights, folds, and bracket pairs from the token stream.
//!
//! Typical usage:
//! ```ignore
//! let spec = sylven_dsl::parse_spec(source).unwrap();
//! let plugin = sylven_runtime::RuntimePlugin::new(sylven_runtime::compile(&spec));
//! registry.register(Box::new(plugin));
//! ```

mod compile;
mod pattern;
mod plugin;

pub use compile::{CompiledSpec, compile};
pub use pattern::Pattern;
pub use plugin::RuntimePlugin;
