use std::sync::Arc;

use sylven::LanguageRegistry;

use crate::{RuntimePlugin, compile};

const RUST_SPEC: &str = include_str!("../specs/rust.sylven");
const C_SPEC: &str = include_str!("../specs/c.sylven");
const PYTHON_SPEC: &str = include_str!("../specs/python.sylven");
const OXYGEN_SPEC: &str = include_str!("../specs/oxygen.sylven");
const VERILOG_SPEC: &str = include_str!("../specs/verilog.sylven");
const SYSTEMVERILOG_SPEC: &str = include_str!("../specs/systemverilog.sylven");
const VHDL_SPEC: &str = include_str!("../specs/vhdl.sylven");
const BSV_SPEC: &str = include_str!("../specs/bsv.sylven");
const BH_SPEC: &str = include_str!("../specs/bh.sylven");
const SDC_SPEC: &str = include_str!("../specs/sdc.sylven");

/// Register the bundled DSL-based language plugins (Rust, C, Python, Oxygen,
/// Verilog, SystemVerilog, VHDL, BSV, Bluespec Haskell, SDC/XDC) into
/// `registry`. Existing plugins with the same `LanguageId` are replaced.
pub fn register_bundled(registry: &mut LanguageRegistry) {
    for spec_src in [
        RUST_SPEC,
        C_SPEC,
        PYTHON_SPEC,
        OXYGEN_SPEC,
        VERILOG_SPEC,
        SYSTEMVERILOG_SPEC,
        VHDL_SPEC,
        BSV_SPEC,
        BH_SPEC,
        SDC_SPEC,
    ] {
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
    fn oxygen_spec_compiles() {
        let spec = sylven_dsl::parse_spec(OXYGEN_SPEC).expect("oxygen.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "oxygen");
    }

    #[test]
    fn verilog_spec_compiles() {
        let spec = sylven_dsl::parse_spec(VERILOG_SPEC).expect("verilog.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "verilog");
    }

    #[test]
    fn systemverilog_spec_compiles() {
        let spec =
            sylven_dsl::parse_spec(SYSTEMVERILOG_SPEC).expect("systemverilog.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "systemverilog");
    }

    #[test]
    fn vhdl_spec_compiles() {
        let spec = sylven_dsl::parse_spec(VHDL_SPEC).expect("vhdl.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "vhdl");
    }

    #[test]
    fn bsv_spec_compiles() {
        let spec = sylven_dsl::parse_spec(BSV_SPEC).expect("bsv.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "bsv");
    }

    #[test]
    fn bh_spec_compiles() {
        let spec = sylven_dsl::parse_spec(BH_SPEC).expect("bh.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "bh");
    }

    #[test]
    fn sdc_spec_compiles() {
        let spec = sylven_dsl::parse_spec(SDC_SPEC).expect("sdc.sylven parse error");
        let cs = compile(&spec);
        assert_eq!(cs.lang_id, "sdc");
    }

    #[test]
    fn all_langs_registered() {
        let r = reg();
        assert!(r.contains(LanguageId("rust")));
        assert!(r.contains(LanguageId("c")));
        assert!(r.contains(LanguageId("python")));
        assert!(r.contains(LanguageId("oxygen")));
        assert!(r.contains(LanguageId("verilog")));
        assert!(r.contains(LanguageId("systemverilog")));
        assert!(r.contains(LanguageId("vhdl")));
        assert!(r.contains(LanguageId("bsv")));
        assert!(r.contains(LanguageId("bh")));
        assert!(r.contains(LanguageId("sdc")));
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

    #[test]
    fn verilog_highlights_keyword_and_type() {
        let r = reg();
        let src = "module top;\n  wire a;\nendmodule";
        let result = r.get(LanguageId("verilog")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Type)
        );
    }

    #[test]
    fn systemverilog_highlights_logic_type() {
        let r = reg();
        let src = "module top;\n  logic [7:0] a;\nendmodule";
        let result = r
            .get(LanguageId("systemverilog"))
            .unwrap()
            .parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Type)
        );
    }

    #[test]
    fn vhdl_highlights_keyword_and_comment() {
        let r = reg();
        let src = "entity foo is\nend entity; -- trailing\n";
        let result = r.get(LanguageId("vhdl")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Comment)
        );
    }

    #[test]
    fn bsv_highlights_rule_keyword() {
        let r = reg();
        let src = "module mkFoo(Empty);\n  rule tick;\n  endrule\nendmodule";
        let result = r.get(LanguageId("bsv")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
    }

    #[test]
    fn bh_highlights_block_comment() {
        let r = reg();
        let src = "{- doc -}\nmodule mkFoo where\n";
        let result = r.get(LanguageId("bh")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Comment)
        );
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
    }

    #[test]
    fn sdc_highlights_command_keyword() {
        let r = reg();
        let src = "create_clock -period 10 [get_ports clk] # main clock\n";
        let result = r.get(LanguageId("sdc")).unwrap().parse(&snap(src));
        assert_eq!(result.tree.text(), src);
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Keyword)
        );
        assert!(
            result
                .features
                .highlights
                .iter()
                .any(|h| h.kind == HighlightKind::Comment)
        );
    }
}
