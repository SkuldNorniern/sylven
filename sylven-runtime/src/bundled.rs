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
const JAVASCRIPT_SPEC: &str = include_str!("../specs/javascript.sylven");
const TYPESCRIPT_SPEC: &str = include_str!("../specs/typescript.sylven");
const GO_SPEC: &str = include_str!("../specs/go.sylven");
const JAVA_SPEC: &str = include_str!("../specs/java.sylven");
const KOTLIN_SPEC: &str = include_str!("../specs/kotlin.sylven");
const CSHARP_SPEC: &str = include_str!("../specs/csharp.sylven");
const CPP_SPEC: &str = include_str!("../specs/cpp.sylven");
const SWIFT_SPEC: &str = include_str!("../specs/swift.sylven");
const LUA_SPEC: &str = include_str!("../specs/lua.sylven");
const RUBY_SPEC: &str = include_str!("../specs/ruby.sylven");
const PHP_SPEC: &str = include_str!("../specs/php.sylven");
const SHELL_SPEC: &str = include_str!("../specs/shell.sylven");
const POWERSHELL_SPEC: &str = include_str!("../specs/powershell.sylven");
const CSS_SPEC: &str = include_str!("../specs/css.sylven");
const HTML_SPEC: &str = include_str!("../specs/html.sylven");
const XML_SPEC: &str = include_str!("../specs/xml.sylven");
const SQL_SPEC: &str = include_str!("../specs/sql.sylven");
const INI_SPEC: &str = include_str!("../specs/ini.sylven");
const ZIG_SPEC: &str = include_str!("../specs/zig.sylven");

/// Register the bundled DSL-based language plugins (Rust, C, Python, Oxygen,
/// Verilog, SystemVerilog, VHDL, BSV, Bluespec Haskell, SDC/XDC, and the
/// common languages of the web, apps and systems) into
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
        JAVASCRIPT_SPEC,
        TYPESCRIPT_SPEC,
        GO_SPEC,
        JAVA_SPEC,
        KOTLIN_SPEC,
        CSHARP_SPEC,
        CPP_SPEC,
        SWIFT_SPEC,
        LUA_SPEC,
        RUBY_SPEC,
        PHP_SPEC,
        SHELL_SPEC,
        POWERSHELL_SPEC,
        CSS_SPEC,
        HTML_SPEC,
        XML_SPEC,
        SQL_SPEC,
        INI_SPEC,
        ZIG_SPEC,
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
    use sylven::{HighlightKind, LanguageId, LanguagePlugin, ParseResult};
    use sylven_text::{DocumentId, RevisionId, TextRange, TextSnapshot};

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
        for id in NEW_LANGS {
            assert!(r.contains(LanguageId(id)), "{id}");
        }
    }

    /// the languages added with the table generator, a sample each, and a
    /// word in it with the kind it should have.
    const NEW_LANGS: [&str; 19] = [
        "javascript",
        "typescript",
        "go",
        "java",
        "kotlin",
        "csharp",
        "cpp",
        "swift",
        "lua",
        "ruby",
        "php",
        "shell",
        "powershell",
        "css",
        "html",
        "xml",
        "sql",
        "ini",
        "zig",
    ];

    fn sample(id: &str) -> (&'static str, &'static [(&'static str, HighlightKind)]) {
        use HighlightKind::{
            Attribute, Comment, Function, Keyword, KeywordControl, Number, SectionHeader, String,
            Type, Variable,
        };
        match id {
            "javascript" => (
                "// c\nclass Box extends Base {}\nfunction run(a) { return `t${a}` + 0x1f; }\n",
                &[
                    ("// c", Comment),
                    ("Box", Type),
                    ("run", Function),
                    ("return", KeywordControl),
                    ("`t${a}`", String),
                    ("0x1f", Number),
                    ("class", Keyword),
                ],
            ),
            "typescript" => (
                "interface Shape { area(): number }\nconst s: Shape = make();\n",
                &[
                    ("Shape", Type),
                    ("number", Type),
                    ("make", Function),
                    ("const", Keyword),
                ],
            ),
            "go" => (
                "package main\nfunc add(a int) int { return a }\ntype Point struct{}\n",
                &[
                    ("add", Function),
                    ("int", Type),
                    ("return", KeywordControl),
                    ("Point", Type),
                ],
            ),
            "java" => (
                "public class App { void run() { if (x) return; } }\n",
                &[
                    ("App", Type),
                    ("run", Function),
                    ("if", KeywordControl),
                    ("void", Type),
                ],
            ),
            "kotlin" => (
                "fun main() { val x: Int = 1 }\n",
                &[("main", Function), ("Int", Type), ("val", Keyword)],
            ),
            "csharp" => (
                "namespace A { class B { string C() { return \"d\"; } } }\n",
                &[
                    ("B", Type),
                    ("C", Function),
                    ("string", Type),
                    ("\"d\"", String),
                ],
            ),
            "cpp" => (
                "template <typename T> struct Vec { int size() const; };\n",
                &[
                    ("T", Type),
                    ("Vec", Type),
                    ("size", Function),
                    ("template", Keyword),
                ],
            ),
            "swift" => (
                "struct P { func go() -> Int { guard x else { return 1 } } }\n",
                &[("P", Type), ("go", Function), ("guard", KeywordControl)],
            ),
            "lua" => (
                "-- c\nlocal function f(a) if a then return nil end end\n",
                &[
                    ("-- c", Comment),
                    ("f", Function),
                    ("then", KeywordControl),
                    ("nil", Number),
                ],
            ),
            "ruby" => (
                "class Dog\n  def bark\n    @name\n  end\nend\n",
                &[("Dog", Type), ("bark", Function), ("@name", Variable)],
            ),
            "php" => (
                "<?php function f($a) { return $a; } # c\n",
                &[("f", Function), ("$a", Variable), ("# c", Comment)],
            ),
            "shell" => (
                "# c\nfunction go { echo \"$HOME\"; }\nif true; then x=$y; fi\n",
                &[
                    ("# c", Comment),
                    ("go", Function),
                    ("if", KeywordControl),
                    ("$y", Variable),
                ],
            ),
            "powershell" => (
                "function Get-It { param($Name) Write-Host $Name }\n",
                &[
                    ("Get-It", Function),
                    ("$Name", Variable),
                    ("Write-Host", Function),
                ],
            ),
            "css" => (
                "/* c */\n.box { font-size: 12px; }\n@media screen {}\n",
                &[
                    ("/* c */", Comment),
                    ("font-size", Attribute),
                    ("@media", Variable),
                ],
            ),
            "html" => (
                "<!-- c -->\n<div class=\"a\">x</div>\n",
                &[
                    ("<!-- c -->", Comment),
                    ("div", Keyword),
                    ("class", Attribute),
                    ("\"a\"", String),
                ],
            ),
            "xml" => (
                "<root id=\"1\"><item/></root>\n",
                &[("root", Keyword), ("id", Attribute), ("item", Keyword)],
            ),
            "sql" => (
                "SELECT name FROM users WHERE id = 1; -- c\n",
                &[
                    ("SELECT", Keyword),
                    ("FROM", Keyword),
                    ("-- c", Comment),
                    ("1", Number),
                ],
            ),
            "ini" => (
                "; c\n[core]\nname = value\n",
                &[
                    ("; c", Comment),
                    ("core", SectionHeader),
                    ("name", Attribute),
                ],
            ),
            "zig" => (
                "pub fn main() void { const x: u8 = @intCast(1); }\n",
                &[
                    ("main", Function),
                    ("void", Type),
                    ("@intCast", Variable),
                    ("pub", Keyword),
                ],
            ),
            _ => ("", &[]),
        }
    }

    #[test]
    fn new_languages_are_lossless_and_lit() {
        let r = reg();
        for id in NEW_LANGS {
            let plugin = r.get(LanguageId(id)).unwrap();
            let (src, expect) = sample(id);
            let result = plugin.parse(&snap(src));
            assert_eq!(result.tree.text(), src, "{id} lossless");
            for (word, kind) in expect {
                // the word may show inside another; any whole match will do
                let lit = src.match_indices(word).any(|(at, _)| {
                    result.features.highlights.iter().any(|h| {
                        h.range.start().to_usize() == at
                            && h.range.end().to_usize() == at + word.len()
                            && h.kind == *kind
                    })
                });
                assert!(lit, "{id}: {word} as {kind:?}");
            }
        }
    }

    #[test]
    fn new_languages_survive_broken_text() {
        let r = reg();
        for id in NEW_LANGS {
            let plugin = r.get(LanguageId(id)).unwrap();
            let (src, _) = sample(id);
            // cut short at every char: never a panic, always the same text back
            for end in (0..=src.len()).filter(|&i| src.is_char_boundary(i)) {
                let part = &src[..end];
                assert_eq!(plugin.parse(&snap(part)).tree.text(), part, "{id} at {end}");
            }
        }
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

    // ── Corpus: handwritten RustLanguage vs compiled rust.sylven ─────────────

    fn highlights_of_kind(result: &ParseResult, kind: HighlightKind) -> Vec<TextRange> {
        let mut v: Vec<_> = result
            .features
            .highlights
            .iter()
            .filter(|h| h.kind == kind)
            .map(|h| h.range)
            .collect();
        v.sort_by_key(|r| r.start());
        v
    }

    #[test]
    fn rust_corpus_both_lossless() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let snippets = [
            "fn main() {}",
            "fn f() {\n    let x = 1;\n}",
            r#"let s = "hello";"#,
            "// comment",
            "struct Foo {}",
        ];
        for src in &snippets {
            let hw = RustLanguage.parse(&snap(src));
            let cm = compiled.parse(&snap(src));
            assert_eq!(hw.tree.text(), *src, "hw lossless: {src:?}");
            assert_eq!(cm.tree.text(), *src, "compiled lossless: {src:?}");
        }
    }

    #[test]
    fn rust_corpus_keyword_ranges_agree() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let src = "fn main() {}";
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        let hw_kw = highlights_of_kind(&hw, HighlightKind::Keyword);
        let cm_kw = highlights_of_kind(&cm, HighlightKind::Keyword);
        assert_eq!(hw_kw, cm_kw, "keyword ranges must agree for {src:?}");
    }

    #[test]
    fn rust_corpus_string_ranges_agree() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let src = r#"let s = "hello";"#;
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        let hw_s = highlights_of_kind(&hw, HighlightKind::String);
        let cm_s = highlights_of_kind(&cm, HighlightKind::String);
        assert_eq!(hw_s, cm_s, "string ranges must agree for {src:?}");
    }

    #[test]
    fn rust_corpus_type_ranges_agree() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let src = "let x: i32 = 0;";
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        let hw_t = highlights_of_kind(&hw, HighlightKind::Type);
        let cm_t = highlights_of_kind(&cm, HighlightKind::Type);
        assert_eq!(hw_t, cm_t, "type ranges must agree for {src:?}");
    }

    #[test]
    fn rust_corpus_comment_ranges_agree() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        // No trailing newline: lex_rust includes \n in comment token,
        // while Pattern::LineComment stops before \n.
        let src = "// line comment";
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        let hw_c = highlights_of_kind(&hw, HighlightKind::Comment);
        let cm_c = highlights_of_kind(&cm, HighlightKind::Comment);
        assert_eq!(hw_c, cm_c, "comment ranges must agree for {src:?}");
    }

    #[test]
    fn rust_corpus_folds_agree_multiline_block() {
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let src = "fn f() {\n    let x = 1;\n}";
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        assert!(
            !hw.features.folds.is_empty(),
            "hw: multiline block should fold"
        );
        assert!(
            !cm.features.folds.is_empty(),
            "compiled: multiline block should fold"
        );
    }

    #[test]
    fn rust_corpus_symbols_gap_documented() {
        // Known gap: rust.sylven has no grammar/symbols block.
        // Handwritten derives symbols from the token stream; compiled produces none.
        // Update this test when symbols are added to rust.sylven.
        use sylven::lang::rust::RustLanguage;
        let r = reg();
        let compiled = r.get(LanguageId("rust")).unwrap();
        let src = "fn foo() {}\nstruct Bar {}\n";
        let hw = RustLanguage.parse(&snap(src));
        let cm = compiled.parse(&snap(src));
        assert!(!hw.features.symbols.is_empty(), "hw extracts symbols");
        assert!(
            cm.features.symbols.is_empty(),
            "compiled has no symbols (gap — add symbols block to rust.sylven)"
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

    fn engine() -> Arc<sylven::SyntaxEngine> {
        Arc::new(sylven::SyntaxEngine::with_registry(reg()))
    }

    fn parse(lang: &'static str, src: &str) -> sylven::ParseResult {
        let snap = sylven::TextSnapshot::new(sylven::DocumentId(0), sylven::RevisionId(0), src);
        engine().parse(LanguageId(lang), &snap).expect("registered")
    }

    #[test]
    fn string_ending_in_a_backslash_parses() {
        for lang in ["c", "python", "vhdl", "sdc", "verilog", "rust"] {
            for src in ["x = \"abc\\", "'\\", "\"가\\"] {
                assert_eq!(parse(lang, src).tree.text(), src, "{lang} {src:?}");
            }
        }
    }

    #[test]
    fn closers_without_openers_stay_linear() {
        let src = "[)".repeat(50_000);
        let started = std::time::Instant::now();
        let result = parse("c", &src);
        assert_eq!(result.tree.text(), src);
        // the old scan of every open bracket took seconds here
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let mixed = parse("c", "( [ ) ]");
        assert_eq!(
            mixed.features.brackets.len(),
            2,
            "each closer finds its own kind"
        );
    }

    /// what the edit tests type at every cut: all of it for a few languages,
    /// a few for the rest, so the run stays short.
    const EVERY: &[&str] = &["(", ")", "{", "\"", "x", "\n", "\r\n"];
    const LIGHT: &[&str] = &["(", "\"", "\n"];

    /// every one char insert and delete at every cut of `src` parses on
    /// to the same tree, errors and features as a full parse.
    fn check_small_edits(lang: &'static str, src: &str, inserts: &[&str]) {
        use sylven::SyntaxSession;
        use sylven_text::{TextEdit, TextRange, TextSize};
        let snap =
            |t: &str| sylven::TextSnapshot::new(sylven::DocumentId(0), sylven::RevisionId(0), t);
        let cuts: Vec<usize> = (0..=src.len())
            .filter(|&i| src.is_char_boundary(i))
            .collect();
        let mut edits = Vec::new();
        for &insert in inserts {
            for &at in &cuts {
                edits.push(TextEdit::insert(TextSize::from(at as u32), insert));
            }
        }
        for pair in cuts.windows(2) {
            let range = TextRange::new(
                TextSize::from(pair[0] as u32),
                TextSize::from(pair[1] as u32),
            );
            edits.push(TextEdit::replace(range, ""));
        }
        for edit in edits {
            let mut s = SyntaxSession::new(engine(), LanguageId(lang));
            s.parse(snap(src)).unwrap();
            let new_src = edit.apply(src);
            let got = s.parse_edit(&edit, snap(&new_src)).unwrap().clone();
            let mut full = SyntaxSession::new(engine(), LanguageId(lang));
            let want = full.parse(snap(&new_src)).unwrap();
            let at = format!("{lang} {src:?} {edit:?}");
            assert_eq!(got.tree.green(), want.tree.green(), "tree at {at}");
            assert_eq!(got.errors, want.errors, "errors at {at}");
            assert_eq!(got.features, want.features, "features at {at}");
        }
    }

    #[test]
    fn small_edits_match_full_parse_in_runtime_languages() {
        check_small_edits(
            "c",
            "int f(int a) {\n  return a; /* x */\n}\nint g() { return \"s\"; }\n",
            EVERY,
        );
        check_small_edits(
            "python",
            "def f(a):\n    return [a, \"가\"]\n\nclass B:\n    pass\n",
            EVERY,
        );
        check_small_edits(
            "verilog",
            "module m(input a);\n  assign b = (a);\nendmodule\n",
            EVERY,
        );
    }

    #[test]
    fn small_edits_match_full_parse_in_every_runtime_language() {
        let cases: [(&'static str, &str); 26] = [
            (
                "rust",
                "fn f(a: &str) -> u8 { // c\n    let s = \"x\"; (a) }\n",
            ),
            (
                "oxygen",
                "// c\nfn f(a: int) -> int { let s = \"x\"; (a) }\n",
            ),
            (
                "systemverilog",
                "// c\nmodule m(input a);\n  string s = \"x\"; assign b = (a);\nendmodule\n",
            ),
            (
                "vhdl",
                "-- c\nentity e is port (a : in string := \"x\"); end;\n",
            ),
            (
                "bsv",
                "// c\nmodule m;\n  String s = \"x\"; Reg r = (1);\nendmodule\n",
            ),
            ("bh", "-- c\n{- b -}\nmodule m (a :: Integer) = (\"x\")\n"),
            ("sdc", "# c\nset_false_path -from [get_ports a] -to \"x\"\n"),
            (
                "javascript",
                "// c\nconst s = `x${(1)}`; function f(a) { return [a, \"s\"]; }\n",
            ),
            (
                "typescript",
                "interface S { a(): number }\nconst s: S = { b: \"x\" }; // c\n",
            ),
            (
                "go",
                "package main\nfunc f(a int) int { s := \"x\"; return a } // c\n",
            ),
            (
                "java",
                "class A { /* c */ void f() { int[] s = {(1)}; String t = \"x\"; } }\n",
            ),
            ("kotlin", "// c\nfun f(a: Int) { val s = \"x\" + (a) }\n"),
            ("csharp", "// c\nclass A { string F() { return \"x\"; } }\n"),
            (
                "cpp",
                "// c\nint f(int a) { const char* s = \"x\"; return (a); }\n",
            ),
            (
                "swift",
                "// c\nfunc f(a: Int) -> Int { let s = \"x\"; return (a) }\n",
            ),
            (
                "lua",
                "-- c\nlocal function f(a) local s = \"x\" return (a) end\n",
            ),
            ("ruby", "# c\ndef f(a)\n  s = \"x\"; [a, (1)]\nend\n"),
            (
                "php",
                "<?php // c\nfunction f($a) { $s = \"x\"; return [$a]; }\n",
            ),
            ("shell", "# c\nf() { s=\"x\"; [ -f \"$1\" ] && echo $s; }\n"),
            (
                "powershell",
                "function F([int]$a) { $s = \"x\"; @($a) } # c\n",
            ),
            ("css", "/* c */\n.a { color: red; } p > a[href=\"x\"] {}\n"),
            ("html", "<!-- c -->\n<p class=\"x\">(a)</p>\n"),
            ("xml", "<!-- c -->\n<a id=\"x\"><b/></a>\n"),
            ("sql", "-- c\nSELECT name, 'x' FROM users WHERE (id) = 1;\n"),
            ("ini", "; c\n[core]\nname = \"x\"\n"),
            (
                "zig",
                "// c\npub fn main() void { const s = \"x\"; _ = (1); }\n",
            ),
        ];
        // the list can't drift from what is registered
        for id in NEW_LANGS {
            assert!(cases.iter().any(|(l, _)| *l == id), "{id} has no sample");
        }
        for (lang, src) in cases {
            check_small_edits(lang, src, LIGHT);
        }
    }
}
