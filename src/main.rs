//! `sylven` CLI: debugging tools for `.sylven` language specs and for source
//! files parsed by the built-in language plugins.
//!
//! Commands:
//! - `sylven check <file.sylven>` — parse a `.sylven` spec and validate that
//!   its highlight, fold, symbol, and recovery rules, plus grammar field
//!   types, reference declared tokens and grammar nodes.
//! - `sylven tree <file>` — parse a source file with the built-in plugin
//!   matching its extension and print its syntax tree.
//! - `sylven highlight <file>` — parse a source file and print its highlight
//!   captures.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::process::ExitCode;

use sylven::{DocumentId, LanguageId, RevisionId, SyntaxEngine, TextSnapshot};
use sylven_dsl::{GrammarItem, HighlightSource, SylvenSpec};
use sylven_text::{LineIndex, TextSize};
use sylven_tree::{SyntaxElement, SyntaxNode};

/// Symbol kinds understood by `symbols { Node.field -> kind }` rules
/// (mirrors `sylven_runtime::compile::parse_symbol_kind`).
const SYMBOL_KINDS: &[&str] = &[
    "function", "struct", "enum", "trait", "impl", "module", "constant", "type", "macro",
    "section", "heading",
];

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return ExitCode::FAILURE;
    };
    let Some(path) = args.next() else {
        eprintln!("error: `{command}` requires a <file> argument");
        print_usage();
        return ExitCode::FAILURE;
    };

    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };

    match command.as_str() {
        "check" => cmd_check(&path, &source),
        "tree" => cmd_tree(&path, &source),
        "highlight" => cmd_highlight(&path, &source),
        other => {
            eprintln!("error: unknown command `{other}`");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!("usage: sylven check <file.sylven>");
    eprintln!("       sylven tree <file>");
    eprintln!("       sylven highlight <file>");
}

// ---------------------------------------------------------------------------
// check — validate a .sylven language spec
// ---------------------------------------------------------------------------

fn cmd_check(path: &str, source: &str) -> ExitCode {
    let spec = match sylven_dsl::parse_spec(source) {
        Ok(spec) => spec,
        Err(errors) => {
            let line_index = LineIndex::new(source);
            for err in &errors {
                let pos = line_index.line_col(TextSize::from(err.offset as u32));
                eprintln!(
                    "{path}:{}:{}: error: {}",
                    pos.line + 1,
                    pos.col + 1,
                    err.message
                );
            }
            return ExitCode::FAILURE;
        }
    };

    let diagnostics = check_spec(&spec);
    if diagnostics.is_empty() {
        println!(
            "{path}: ok ({} tokens, {} grammar nodes, {} highlight rules, {} fold rules, {} symbol rules)",
            spec.tokens.len(),
            spec.grammar.len(),
            spec.highlight.len(),
            spec.fold.len(),
            spec.symbols.len(),
        );
        ExitCode::SUCCESS
    } else {
        for d in &diagnostics {
            eprintln!("{path}: error: {d}");
        }
        ExitCode::FAILURE
    }
}

/// Structural checks that don't require running the DSL parser's own
/// validation: every reference from `highlight`/`fold`/`symbols`/`recovery`
/// rules, and every grammar field/ref/choice type, must name a declared
/// token or grammar node.
fn check_spec(spec: &SylvenSpec) -> Vec<String> {
    let mut errors = Vec::new();

    let token_names: HashSet<&str> = spec.tokens.iter().map(|t| t.name.as_str()).collect();
    let node_names: HashSet<&str> = spec.grammar.iter().map(|n| n.name.as_str()).collect();

    let node_fields: HashMap<&str, HashSet<&str>> = spec
        .grammar
        .iter()
        .map(|n| {
            let mut fields = HashSet::new();
            collect_fields(&n.items, &mut fields);
            (n.name.as_str(), fields)
        })
        .collect();

    for node in &spec.grammar {
        check_grammar_items(
            &node.name,
            &node.items,
            &token_names,
            &node_names,
            &mut errors,
        );
    }

    for rule in &spec.highlight {
        match &rule.source {
            HighlightSource::Token(name) => {
                if !token_names.contains(name.as_str()) {
                    errors.push(format!("highlight rule references unknown token `{name}`"));
                }
            }
            HighlightSource::NodeField { node, field } => match node_fields.get(node.as_str()) {
                None => errors.push(format!("highlight rule references unknown node `{node}`")),
                Some(fields) if !fields.contains(field.as_str()) => errors.push(format!(
                    "highlight rule references unknown field `{node}.{field}`"
                )),
                Some(_) => {}
            },
        }
    }

    // Specs without a `grammar {}` block use `fold { Block when multiline }`
    // purely as a flag to enable bracket-pair folding; the node name is not
    // resolved against anything, so only check it once a grammar exists.
    if !spec.grammar.is_empty() {
        for rule in &spec.fold {
            if !node_names.contains(rule.node.as_str()) {
                errors.push(format!("fold rule references unknown node `{}`", rule.node));
            }
        }
    }

    for rule in &spec.symbols {
        match node_fields.get(rule.node.as_str()) {
            None => errors.push(format!(
                "symbol rule references unknown node `{}`",
                rule.node
            )),
            Some(fields) if !fields.contains(rule.field.as_str()) => errors.push(format!(
                "symbol rule references unknown field `{}.{}`",
                rule.node, rule.field
            )),
            Some(_) => {}
        }
        if !SYMBOL_KINDS.contains(&rule.kind.as_str()) {
            errors.push(format!("symbol rule has unknown kind `{}`", rule.kind));
        }
    }

    for rule in &spec.recovery {
        if !node_names.contains(rule.node.as_str()) {
            errors.push(format!(
                "recovery rule references unknown node `{}`",
                rule.node
            ));
        }
    }

    errors
}

/// Collects the named field labels (`name:Type`) declared directly in
/// `items`, recursing through `Optional`/`Repeat` wrappers.
fn collect_fields<'a>(items: &'a [GrammarItem], out: &mut HashSet<&'a str>) {
    for item in items {
        match item {
            GrammarItem::Field { label, .. } => {
                out.insert(label.as_str());
            }
            GrammarItem::Optional(inner) | GrammarItem::Repeat(inner) => {
                collect_fields(std::slice::from_ref(inner.as_ref()), out);
            }
            GrammarItem::Literal(_) | GrammarItem::Ref(_) | GrammarItem::Choice(_) => {}
        }
    }
}

/// Recursively checks that every type name referenced by `items` (field
/// types, bare refs, and choice alternatives) names a declared token or
/// grammar node.
fn check_grammar_items(
    node_name: &str,
    items: &[GrammarItem],
    token_names: &HashSet<&str>,
    node_names: &HashSet<&str>,
    errors: &mut Vec<String>,
) {
    let check_ty = |ty: &str, errors: &mut Vec<String>| {
        if !token_names.contains(ty) && !node_names.contains(ty) {
            errors.push(format!("node `{node_name}` references unknown type `{ty}`"));
        }
    };
    for item in items {
        match item {
            GrammarItem::Literal(_) => {}
            GrammarItem::Field { ty, .. } | GrammarItem::Ref(ty) => check_ty(ty, errors),
            GrammarItem::Choice(alts) => {
                for ty in alts {
                    check_ty(ty, errors);
                }
            }
            GrammarItem::Optional(inner) | GrammarItem::Repeat(inner) => {
                check_grammar_items(
                    node_name,
                    std::slice::from_ref(inner.as_ref()),
                    token_names,
                    node_names,
                    errors,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// tree / highlight — parse a source file with a built-in plugin
// ---------------------------------------------------------------------------

/// Maps a file extension to the built-in [`LanguageId`] that handles it.
fn detect_language(path: &str) -> Option<LanguageId> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    Some(match ext {
        "rs" => LanguageId("rust"),
        "md" | "markdown" => LanguageId("markdown"),
        "toml" => LanguageId("toml"),
        "json" => LanguageId("json"),
        "yaml" | "yml" => LanguageId("yaml"),
        "oxy" => LanguageId("mini-oxygen"),
        _ => return None,
    })
}

fn cmd_tree(path: &str, source: &str) -> ExitCode {
    let Some(lang) = detect_language(path) else {
        eprintln!("error: cannot detect language for `{path}` (unrecognized extension)");
        return ExitCode::FAILURE;
    };
    let engine = SyntaxEngine::new();
    let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
    let result = engine
        .parse(lang, &snap)
        .expect("built-in language plugins are always registered");

    print_tree(&result.tree.root(), 0);
    if !result.errors.is_empty() {
        eprintln!("{} parse error(s)", result.errors.len());
    }
    ExitCode::SUCCESS
}

fn print_tree(node: &SyntaxNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let range = node.text_range();
    println!(
        "{indent}{:?}@{}..{}",
        node.kind(),
        range.start().to_usize(),
        range.end().to_usize()
    );
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Node(n) => print_tree(&n, depth + 1),
            SyntaxElement::Token(t) => {
                let r = t.text_range();
                println!(
                    "{}{:?}@{}..{} {:?}",
                    "  ".repeat(depth + 1),
                    t.kind(),
                    r.start().to_usize(),
                    r.end().to_usize(),
                    t.text()
                );
            }
        }
    }
}

fn cmd_highlight(path: &str, source: &str) -> ExitCode {
    let Some(lang) = detect_language(path) else {
        eprintln!("error: cannot detect language for `{path}` (unrecognized extension)");
        return ExitCode::FAILURE;
    };
    let engine = SyntaxEngine::new();
    let snap = TextSnapshot::new(DocumentId(0), RevisionId(0), source);
    let result = engine
        .parse(lang, &snap)
        .expect("built-in language plugins are always registered");

    let mut highlights = result.features.highlights.clone();
    highlights.sort_by_key(|h| h.range.start());
    for h in &highlights {
        let start = h.range.start().to_usize();
        let end = h.range.end().to_usize();
        println!("{:?}@{start}..{end} {:?}", h.kind, &source[start..end]);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_by_extension() {
        assert_eq!(detect_language("foo.rs"), Some(LanguageId("rust")));
        assert_eq!(detect_language("foo.md"), Some(LanguageId("markdown")));
        assert_eq!(detect_language("foo.toml"), Some(LanguageId("toml")));
        assert_eq!(detect_language("foo.json"), Some(LanguageId("json")));
        assert_eq!(detect_language("foo.yaml"), Some(LanguageId("yaml")));
        assert_eq!(detect_language("foo.yml"), Some(LanguageId("yaml")));
        assert_eq!(detect_language("foo.oxy"), Some(LanguageId("mini-oxygen")));
        assert_eq!(detect_language("foo.txt"), None);
        assert_eq!(detect_language("foo"), None);
    }

    #[test]
    fn check_spec_accepts_flat_token_spec_with_block_fold() {
        // Specs with no `grammar {}` block use `fold { Block when multiline }`
        // purely as a flag; "Block" names nothing and must not be flagged.
        let src = r#"
language { id "toy" extensions [".toy"] }
tokens { kw ["let"] ident /[a-z]+/ }
highlight { token kw -> keyword }
fold { Block when multiline }
"#;
        let spec = sylven_dsl::parse_spec(src).unwrap();
        assert!(check_spec(&spec).is_empty());
    }

    #[test]
    fn check_spec_rejects_unknown_highlight_node() {
        let src = r#"
language { id "toy" extensions [".toy"] }
tokens { kw ["let"] ident /[a-z]+/ }
grammar { node LetStmt { "let" name:ident ";" } }
highlight { Bogus.name -> variable }
"#;
        let spec = sylven_dsl::parse_spec(src).unwrap();
        let errors = check_spec(&spec);
        assert!(
            errors.iter().any(|e| e.contains("Bogus")),
            "expected an unknown-node error, got {errors:?}"
        );
    }

    #[test]
    fn check_spec_rejects_unknown_symbol_field_and_kind() {
        let src = r#"
language { id "toy" extensions [".toy"] }
tokens { kw ["let"] ident /[a-z]+/ }
grammar { node LetStmt { "let" name:ident ";" } }
symbols {
  LetStmt.missing -> constant
  LetStmt.name -> not.a.kind
}
"#;
        let spec = sylven_dsl::parse_spec(src).unwrap();
        let errors = check_spec(&spec);
        assert!(
            errors.iter().any(|e| e.contains("LetStmt.missing")),
            "expected an unknown-field error, got {errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("not.a.kind")),
            "expected an unknown-kind error, got {errors:?}"
        );
    }

    #[test]
    fn check_spec_rejects_unknown_grammar_type_ref() {
        let src = r#"
language { id "toy" extensions [".toy"] }
tokens { kw ["let"] ident /[a-z]+/ }
grammar { node LetStmt { "let" name:ident value:Expr ";" } }
"#;
        let spec = sylven_dsl::parse_spec(src).unwrap();
        let errors = check_spec(&spec);
        assert!(
            errors.iter().any(|e| e.contains("Expr")),
            "expected an unknown-type error, got {errors:?}"
        );
    }

    #[test]
    fn check_spec_accepts_valid_grammar_spec() {
        let src = r#"
language { id "toy" extensions [".toy"] }
tokens { kw ["let"] ident /[a-z]+/ }
grammar {
  node LetStmt { "let" name:ident ";" }
}
highlight { LetStmt.name -> variable }
fold { LetStmt when multiline }
symbols { LetStmt.name -> constant }
"#;
        let spec = sylven_dsl::parse_spec(src).unwrap();
        assert!(check_spec(&spec).is_empty());
    }
}
