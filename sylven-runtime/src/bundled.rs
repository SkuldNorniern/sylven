use std::sync::Arc;

use sylven::LanguageRegistry;

use crate::{RuntimePlugin, compile};

const RUST_SPEC: &str = include_str!("../specs/rust.sylven");
const C_SPEC: &str = include_str!("../specs/c.sylven");
const PYTHON_SPEC: &str = include_str!("../specs/python.sylven");

/// Register the bundled DSL-based language plugins (Rust, C, Python) into
/// `registry`. Existing plugins with the same `LanguageId` are replaced.
pub fn register_bundled(registry: &mut LanguageRegistry) {
    for spec_src in [RUST_SPEC, C_SPEC, PYTHON_SPEC] {
        match sylven_dsl::parse_spec(spec_src) {
            Ok(spec) => {
                let compiled = compile(&spec);
                registry.register(Arc::new(RuntimePlugin::new(compiled)));
            }
            Err(errs) => panic!("bundled spec parse error: {errs:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sylven::{HighlightKind, LanguageId};
    use sylven_text::{DocumentId, RevisionId, TextSnapshot};

    fn snap(text: &str) -> TextSnapshot {
        TextSnapshot::new(DocumentId(0), RevisionId(0), text)
    }

    fn reg() -> LanguageRegistry {
        let mut r = LanguageRegistry::new();
        register_bundled(&mut r);
        r
    }

    #[test]
    fn rust_spec_compiles() {
        let spec = sylven_dsl::parse_spec(RUST_SPEC).expect("rust.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "rust");
    }

    #[test]
    fn c_spec_compiles() {
        let spec = sylven_dsl::parse_spec(C_SPEC).expect("c.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "c");
    }

    #[test]
    fn python_spec_compiles() {
        let spec = sylven_dsl::parse_spec(PYTHON_SPEC).expect("python.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "python");
    }

    #[test]
    fn all_three_langs_registered() {
        let r = reg();
        assert!(r.contains(LanguageId("rust")));
        assert!(r.contains(LanguageId("c")));
        assert!(r.contains(LanguageId("python")));
    }

    #[test]
    fn rust_lossless_tree() {
        let r = reg();
        let plugin = r.get(LanguageId("rust")).unwrap();
        let src = "fn main() {}";
        assert_eq!(plugin.parse(&snap(src)).tree.text(), src);
    }

    #[test]
    fn rust_highlights_keyword() {
        let r = reg();
        let result = r
            .get(LanguageId("rust"))
            .unwrap()
            .parse(&snap("fn main() {}"));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
    }

    #[test]
    fn rust_highlights_type() {
        let r = reg();
        let result = r
            .get(LanguageId("rust"))
            .unwrap()
            .parse(&snap("let x: i32 = 0;"));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Type)
        );
    }

    #[test]
    fn rust_highlights_string() {
        let r = reg();
        let result = r
            .get(LanguageId("rust"))
            .unwrap()
            .parse(&snap(r#"let s = "hello";"#));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::String)
        );
    }

    #[test]
    fn rust_folds_multiline_block() {
        let r = reg();
        let result = r
            .get(LanguageId("rust"))
            .unwrap()
            .parse(&snap("fn f() {\n    let x = 1;\n}"));
        assert!(!result.features.folds.is_empty());
    }

    #[test]
    fn c_lossless_tree() {
        let r = reg();
        let src = "int main() { return 0; }";
        assert_eq!(
            r.get(LanguageId("c"))
                .unwrap()
                .parse(&snap(src))
                .tree
                .text(),
            src
        );
    }

    #[test]
    fn c_highlights_keyword() {
        let r = reg();
        let result = r.get(LanguageId("c")).unwrap().parse(&snap("int x = 0;"));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
    }

    #[test]
    fn c_highlights_string() {
        let r = reg();
        let result = r
            .get(LanguageId("c"))
            .unwrap()
            .parse(&snap(r#"char *s = "hello";"#));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::String)
        );
    }

    #[test]
    fn python_lossless_tree() {
        let r = reg();
        let src = "def foo(): pass";
        assert_eq!(
            r.get(LanguageId("python"))
                .unwrap()
                .parse(&snap(src))
                .tree
                .text(),
            src
        );
    }

    #[test]
    fn python_highlights_keyword() {
        let r = reg();
        let result = r
            .get(LanguageId("python"))
            .unwrap()
            .parse(&snap("def foo(): pass"));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
    }

    #[test]
    fn python_highlights_string() {
        let r = reg();
        let result = r
            .get(LanguageId("python"))
            .unwrap()
            .parse(&snap(r#"x = "hello""#));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::String)
        );
    }

    #[test]
    fn python_highlights_comment() {
        let r = reg();
        let result = r
            .get(LanguageId("python"))
            .unwrap()
            .parse(&snap("# a comment"));
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Comment)
        );
    }

    #[test]
    fn python_floor_div_is_not_comment() {
        let r = reg();
        let src = "x = 10 // 3";
        let result = r.get(LanguageId("python")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            !result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Comment)
        );
    }
}
