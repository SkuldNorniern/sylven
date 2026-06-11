/// Parsed contents of a `.sylven` language specification file.
#[derive(Debug, Clone, Default)]
pub struct SylvenSpec {
    pub language: LanguageMeta,
    pub tokens: Vec<TokenDecl>,
    pub grammar: Vec<NodeDecl>,
    pub pratt: Vec<PrattSpec>,
    pub recovery: Vec<RecoveryRule>,
    pub highlight: Vec<HighlightRule>,
    pub fold: Vec<FoldRule>,
    pub symbols: Vec<SymbolRule>,
}

/// Language metadata: identity and comment syntax.
#[derive(Debug, Clone, Default)]
pub struct LanguageMeta {
    /// `LanguageId` string, e.g. `"mini-oxygen"`.
    pub id: String,
    /// File extensions with leading dot, e.g. `[".oxy"]`.
    pub extensions: Vec<String>,
    /// Line comment opener, e.g. `"//"`.
    pub comment_line: Option<String>,
    /// Block comment delimiters, e.g. `("/*", "*/")`.
    pub comment_block: Option<(String, String)>,
}

/// One token declared in the `tokens {}` block.
#[derive(Debug, Clone)]
pub struct TokenDecl {
    pub name: String,
    pub kind: TokenKind,
    /// True for whitespace/comment tokens that are attached as trivia.
    pub is_trivia: bool,
}

/// How a token is matched.
#[derive(Debug, Clone)]
pub enum TokenKind {
    /// A set of literal keywords: `keyword ["fn", "let"]`.
    KeywordSet(Vec<String>),
    /// A regex pattern: `ident /[a-zA-Z_]\w*/`.
    Regex(String),
    /// A single literal string for an operator or delimiter: `"(" lparen`.
    Literal(String),
    /// A line comment that runs to end-of-line: `line.comment "//" to newline`.
    LineComment(String),
    /// A block comment with open and close: `block.comment "/*" to "*/"`.
    BlockComment(String, String),
}

/// A node declared in the `grammar {}` block.
#[derive(Debug, Clone, Default)]
pub struct NodeDecl {
    pub name: String,
    /// Named child fields: `name:Name params:ParamList`.
    pub fields: Vec<NodeField>,
}

/// A named child reference inside a node declaration.
#[derive(Debug, Clone)]
pub struct NodeField {
    pub label: String,
    pub ty: String,
}

/// A Pratt expression table declared as `pratt Name { ... }`.
#[derive(Debug, Clone)]
pub struct PrattSpec {
    pub name: String,
    pub atoms: Vec<String>,
    pub prefix: Vec<PrattPrefix>,
    pub infix: Vec<PrattInfix>,
}

#[derive(Debug, Clone)]
pub struct PrattPrefix {
    pub op: String,
    pub node: String,
}

#[derive(Debug, Clone)]
pub struct PrattInfix {
    pub assoc: Assoc,
    pub op: String,
    pub prec: u8,
    pub node: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
}

/// One rule in the `recovery {}` block.
#[derive(Debug, Clone)]
pub struct RecoveryRule {
    pub node: String,
    pub strategy: RecoveryStrategy,
}

#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    SkipUntil(Vec<String>),
}

/// One rule in the `highlight {}` block.
#[derive(Debug, Clone)]
pub struct HighlightRule {
    pub source: HighlightSource,
    /// Theme scope name, e.g. `"keyword"`, `"function.definition"`.
    pub scope: String,
}

#[derive(Debug, Clone)]
pub enum HighlightSource {
    /// `token name` — highlights all tokens of a declared token type.
    Token(String),
    /// `Node.field` — highlights the field's span with a semantic scope.
    NodeField { node: String, field: String },
}

/// One rule in the `fold {}` block.
#[derive(Debug, Clone)]
pub struct FoldRule {
    pub node: String,
    pub condition: FoldCondition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldCondition {
    Multiline,
    Always,
}

/// One rule in the `symbols {}` block.
#[derive(Debug, Clone)]
pub struct SymbolRule {
    pub node: String,
    pub field: String,
    pub kind: String,
}
