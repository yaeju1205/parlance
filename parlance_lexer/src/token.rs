// ── Token types ──────────────────────────────────────────────────
//
// SCANNER THEORY (DFA):
//
//   The lexer implements a Deterministic Finite Automaton (DFA).
//   The alphabet is partitioned into character classes.  Each token
//   corresponds to an accepting state reached by consuming characters
//   from the input stream:
//
//     Class          →  Accepting state      →  Token produced
//     ──────────────────────────────────────────────────────────
//     digit          →  INT                  →  Int(i64)
//     digit + '.' + digit → FLOAT            →  Float(f64)
//     '.' + digit    →  FLOAT                →  Float(f64)
//     ident_start    →  IDENT                →  Ident(String)
//     quote (")      →  STRING               →  Str(String)
//     symbol         →  SYMBOL               →  Op(String)
//     \              →  (single-char)        →  Backslash
//     (              →  (single-char)        →  LParen
//     )              →  (single-char)        →  RParen
//
//   KEYWORD RESOLUTION:
//     After the DFA accepts a full IDENT token, the raw string is
//     looked up in a keyword table.  If it matches "import", "define",
//     "infix", or "var", the corresponding keyword token is emitted
//     instead of Ident.  This keeps the DFA itself small — the DFA
//     doesn't need a separate state per keyword.
//
//   SYMBOL MULTI-CHARACTER RESOLUTION:
//     Similarly, after the DFA accepts a full SYMBOL run (maximal
//     munch), the accumulated string is checked against the reserved
//     multi-character punctuation sequences ->, =, <-, >>=.  If none
//     match, the string becomes a user-declared operator Op(String).
//
//   MAXIMAL MUNCH:
//     Both IDENT and SYMBOL consume the longest possible run of
//     their respective character class.  This prevents, e.g., `>>=`
//     from being tokenized as `>` `>=`.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ── Keywords (reserved identifiers) ──────────────────────────
    Import,
    Define,
    Infix,
    Var,

    // ── Literals ─────────────────────────────────────────────────
    Int(i64),
    Float(f64),
    Str(String),

    // ── Names ────────────────────────────────────────────────────
    Ident(String),

    // ── User-declared operators ──────────────────────────────────
    Op(String),

    // ── Punctuation ──────────────────────────────────────────────
    Backslash, //  \
    Arrow,     //  ->
    Equal,     //  =
    Bind,      //  <-
    BindChain, //  >>=
    LParen,    //  (
    RParen,    //  )
    Comma,     //  ,

    // ── Sentinel ─────────────────────────────────────────────────
    Eof,
}

/// A token annotated with its source location (line, col).
/// Used by the parser for error reporting.
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Import => write!(f, "import"),
            Token::Define => write!(f, "define"),
            Token::Infix => write!(f, "infix"),
            Token::Var => write!(f, "var"),
            Token::Int(n) => write!(f, "{n}"),
            Token::Float(n) => write!(f, "{n}"),
            Token::Str(s) => write!(f, "\"{s}\""),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Op(s) => write!(f, "{s}"),
            Token::Backslash => write!(f, "\\"),
            Token::Arrow => write!(f, "=>"),
            Token::Equal => write!(f, "="),
            Token::Bind => write!(f, "<-"),
            Token::BindChain => write!(f, ">>="),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Eof => write!(f, "EOF"),
        }
    }
}
