use std::sync::Arc;

use sylven_text::TextSnapshot;

use crate::lang::markdown::MarkdownLanguage;
use crate::lang::mini_oxygen::MiniOxygen;
use crate::lang::rust::RustLanguage;
use crate::lang::toml::TomlLanguage;
use crate::{LanguageId, LanguageRegistry, ParseResult};

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
    /// A new engine with the built-in language plugins registered.
    pub fn new() -> SyntaxEngine {
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MiniOxygen));
        registry.register(Arc::new(RustLanguage));
        registry.register(Arc::new(TomlLanguage));
        registry.register(Arc::new(MarkdownLanguage));
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
    /// Returns `None` if no plugin is registered for `language`.
    pub fn parse(&self, language: LanguageId, snapshot: &TextSnapshot) -> Option<ParseResult> {
        let plugin = self.registry.get(language)?;
        Some(plugin.parse(snapshot))
    }
}

impl Default for SyntaxEngine {
    fn default() -> SyntaxEngine {
        SyntaxEngine::new()
    }
}
