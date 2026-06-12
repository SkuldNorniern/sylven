use std::sync::Arc;

use sylven_text::{TextRange, TextSnapshot};

use crate::lang::json::JsonLanguage;
use crate::lang::markdown::MarkdownLanguage;
use crate::lang::mini_oxygen::MiniOxygen;
use crate::lang::rust::RustLanguage;
use crate::lang::toml::TomlLanguage;
use crate::lang::yaml::YamlLanguage;
use crate::{Highlight, LanguageId, LanguageRegistry, ParseResult};

/// Entry point to the syntax engine: a [`LanguageRegistry`] plus the ability
/// to run a plugin's parse for a given language.
///
/// Construct one [`SyntaxEngine`] per editor process — it's cheap to share,
/// so wrap it in `Arc` — and create a [`SyntaxSession`](crate::SyntaxSession)
/// per open document.
pub struct SyntaxEngine {
    registry: LanguageRegistry,
}

impl SyntaxEngine {
    /// A new engine with the built-in hand-written language plugins registered
    /// (Rust, TOML, JSON, Markdown, YAML, mini-oxygen).
    ///
    /// To also load the DSL-compiled Rust, C, and Python plugins (which replace
    /// the hand-written Rust plugin), call
    /// `sylven_runtime::register_bundled(engine.registry_mut())` after construction.
    pub fn new() -> SyntaxEngine {
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MiniOxygen));
        registry.register(Arc::new(RustLanguage));
        registry.register(Arc::new(TomlLanguage));
        registry.register(Arc::new(MarkdownLanguage));
        registry.register(Arc::new(JsonLanguage));
        registry.register(Arc::new(YamlLanguage));
        SyntaxEngine { registry }
    }

    /// Create an engine from a pre-built registry. Useful in tests that need
    /// only a subset of plugins.
    pub fn with_registry(registry: LanguageRegistry) -> SyntaxEngine {
        SyntaxEngine { registry }
    }

    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut LanguageRegistry {
        &mut self.registry
    }

    /// Parse `snapshot` with the plugin registered for `language`.
    ///
    /// Returns `None` if no plugin is registered for `language`. Any
    /// embedded-language injections reported by the plugin (e.g. Markdown
    /// fenced code blocks) are recursively parsed with their matching child
    /// plugin, and the child's highlights are merged into the result with
    /// ranges translated into the parent document's coordinates.
    pub fn parse(&self, language: LanguageId, snapshot: &TextSnapshot) -> Option<ParseResult> {
        let plugin = self.registry.get(language)?;
        let mut result = plugin.parse(snapshot);
        self.merge_injections(snapshot, &mut result);
        Some(result)
    }

    /// Parses each of `result`'s injections with its matching registered
    /// child plugin and appends offset-translated highlights to
    /// `result.features.highlights`. Injections with no language tag, or
    /// whose language tag has no registered plugin, are left unparsed.
    fn merge_injections(&self, snapshot: &TextSnapshot, result: &mut ParseResult) {
        let source = snapshot.text();
        let injections = result.features.injections.clone();
        for injection in &injections {
            let Some(lang) = injection.language.as_deref() else {
                continue;
            };
            let Some(child_id) = self.registry.languages().find(|id| id.0 == lang) else {
                continue;
            };
            let Some(plugin) = self.registry.get(child_id) else {
                continue;
            };

            let start = injection.range.start().to_usize();
            let end = injection.range.end().to_usize();
            if end > source.len() || start > end {
                continue;
            }

            let child_snapshot = TextSnapshot::new(
                snapshot.document_id(),
                snapshot.revision(),
                &source[start..end],
            );
            let child_result = plugin.parse(&child_snapshot);

            let offset = injection.range.start();
            for hl in &child_result.features.highlights {
                result.features.highlights.push(Highlight {
                    range: TextRange::new(hl.range.start() + offset, hl.range.end() + offset),
                    kind: hl.kind,
                });
            }
        }
    }
}

impl Default for SyntaxEngine {
    fn default() -> SyntaxEngine {
        SyntaxEngine::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HighlightKind;
    use sylven_text::{DocumentId, RevisionId};

    #[test]
    fn markdown_fence_merges_rust_highlights() {
        let engine = SyntaxEngine::new();
        let source = "# Title\n\n```rust\nfn main() {}\n```\n";
        let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
        let result = engine.parse(LanguageId("markdown"), &snap).unwrap();

        let fn_start = source.find("fn main").unwrap();
        assert!(
            result.features.highlights.iter().any(|h| {
                h.kind == HighlightKind::Keyword
                    && h.range.start().to_usize() == fn_start
                    && &source[h.range.start().to_usize()..h.range.end().to_usize()] == "fn"
            }),
            "expected an injected `fn` keyword highlight at the parent offset, got {:?}",
            result.features.highlights
        );
    }

    #[test]
    fn markdown_without_fence_has_no_extra_highlights() {
        let engine = SyntaxEngine::new();
        let source = "# Title\n\nplain text\n";
        let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
        let result = engine.parse(LanguageId("markdown"), &snap).unwrap();
        assert!(result.features.injections.is_empty());
        assert!(
            !result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Function)
        );
    }
}
