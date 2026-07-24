// ── Abstract Syntax Tree (AST) ───────────────────────────────────
//
// PARSER THEORY — GRAMMAR HIERARCHY:
//
//   The grammar is designed as a strict precedence hierarchy resolved
//   by recursive-descent parsing.  Each level corresponds to one
//   nonterminal in the grammar and one function in the parser.
//
//   PRECEDENCE TIERS (loosest → tightest):
//
//      expr  ::=  seq (">>=" seq)*                      [Seq]
//       seq  ::=  pratt                                 [Infix via Pratt]
//     pratt  ::=  bind (infix_op bind)*                 [Infix]
//      bind  ::=  "var" ident "<-" expr | apply         [Bind]
//     apply  ::=  atom+                                 [Apply, left-assoc]
//      atom  ::=  int | str | ident
//               | "(" expr ")"
//               | "\" ident "->" expr                   [Lambda]
//
//   This layering guarantees that the grammar is UNAMBIGUOUS:
//   every input string has exactly one valid derivation tree.
//
//   RECURSIVE DESCENT:
//     Each nonterminal has one parsing function that inspects the
//     next token (single-token LOOKAHEAD = LL(1)) and dispatches
//     based on FIRST sets.
//
//   PRATT PARSING (Top-Down Operator Precedence):
//     The `pratt` level uses Vaughan Pratt's algorithm: it reads a
//     left operand (from `bind`), then loops: if the next token is
//     an operator whose declared binding-power >= min_bp, consume it,
//     parse the right operand with min_bp = bp + 1 (left-associative),
//     and build an Infix node.
//
//   DESIGN RATIONALE:
//     - `>>=` (BindChain) is NOT an operator in the Pratt sense;
//       it is handled by `expr` before Pratt kicks in.  This keeps
//       the operator table free of built-in entries.
//     - `Infix` is kept as a distinct AST node so the parser does
//       not need to know operator→function mappings — that is
//       deferred to semantic analysis.
//     - `Bind` is a separate level so that `var x <- e` has the
//       tightness of an atom but binds through infix until >>=,
//       matching Haskell-style do-notation.

use std::fmt;

#[derive(Debug, Clone)]
pub enum Expr {
    // ── Literals ─────────────────────────────────────────────────
    Int(i64),
    Float(f64),
    Str(String),

    // ── Reference (variable use) ─────────────────────────────────
    Var(String),

    // ── Lambda abstraction ───────────────────────────────────────
    //  \param -> body
    //  Parser: atom → Backslash → expect_ident → expect(Arrow) → parse_expr
    Lambda { param: String, body: Box<Expr> },

    // ── Application (function call by juxtaposition) ─────────────
    //  f x y  ⇒  Apply(Apply(Var("f"), Var("x")), Var("y"))
    //  Left-associative, parsed in parse_apply().
    Apply(Box<Expr>, Box<Expr>),

    // ── Infix operation (desugared later by semant) ──────────────
    //  left op right
    //  Parsed by the Pratt loop in parse_pratt().
    Infix(Box<Expr>, String, Box<Expr>),

    // ── Local variable binding ───────────────────────────────────
    //  var name <- value
    //  Parsed in parse_bind().  Greedy: value consumes infix
    //  operators but stops at >>= because that's Token::BindChain.
    Bind { name: String, value: Box<Expr> },

    // ── Sequencing for bind-chains ───────────────────────────────
    //  a >>= b  ⇒  Seq(a, b)
    //  Left-associative: a >>= b >>= c  ⇒  Seq(Seq(a, b), c)
    //  Parsed in parse_expr() after Pratt handles all infix ops.
    Seq(Box<Expr>, Box<Expr>),
}

// ── Import specification ────────────────────────────────────────
//
// MODULE SYSTEM THEORY:
//
//   Each Parlance source file is a MODULE.  A module exports the
//   names bound by its `define` and `infix` statements.  The import
//   statement brings a subset of another module's exported names
//   into the current scope.
//
//   Import resolution is a form of NAME SPACE MANAGEMENT:
//
//     import "prelude"
//       → brings ALL exported names from prelude.plc into scope
//
//     import "prelude" (map, filter)
//       → brings ONLY the listed names
//
//     import "prelude" hiding (internal_helper)
//       → brings ALL names EXCEPT the listed ones
//
//   The underlying theory is SIMPLE SYMBOL IMPORT (not qualified
//   modules): imported names are merged directly into the current
//   scope, and a later definition shadows an earlier import.
//   Cycle detection is performed by tracking which files are
//   currently being resolved.

/// Controls which names are imported from a module.
#[derive(Debug, Clone)]
pub enum ImportSpec {
    /// `import "path"`  —  import all exported names
    All,
    /// `import "path" (a, b, c)`  —  import only the listed names
    Only(Vec<String>),
}

/// A resolved module: the set of names a module exports.
/// Built by the import resolver and consumed by semantic analysis.
#[derive(Debug, Clone)]
pub struct Module {
    pub path: String,
    pub exports: Vec<Export>,
}

/// A single exported binding from a module.
#[derive(Debug, Clone)]
pub enum Export {
    Define {
        name: String,
        expr: Expr,
    },
    Infix {
        op: String,
        strength: u32,
        func: Expr,
    },
}

// ── Statements ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `import "path"` or `import "path" (names...)` or `import "path" hiding (names...)`
    Import { path: String, spec: ImportSpec },

    /// `define name = expr`
    Define { name: String, expr: Expr },

    /// `infix op strength = func`
    Infix {
        op: String,
        strength: u32,
        func: Expr,
    },
}

// ── Display ──────────────────────────────────────────────────────
//  Each variant formats as its concrete syntax, useful for debugging.

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Float(n) => write!(f, "{n}"),
            Expr::Str(s) => write!(f, "\"{s}\""),
            Expr::Var(v) => write!(f, "{v}"),
            Expr::Lambda { param, body } => write!(f, "\\{param} => {body}"),
            Expr::Apply(func, arg) => write!(f, "({func} {arg})"),
            Expr::Infix(l, op, r) => write!(f, "({l} {op} {r})"),
            Expr::Bind { name, value } => write!(f, "(var {name} <- {value})"),
            Expr::Seq(a, b) => write!(f, "({a} >>= {b})"),
        }
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Stmt::Import { path, spec } => {
                write!(f, "import \"{path}\"")?;
                match spec {
                    ImportSpec::All => {}
                    ImportSpec::Only(names) => {
                        write!(f, " ({})", names.join(", "))?;
                    }
                }
                Ok(())
            }
            Stmt::Define { name, expr } => write!(f, "define {name} = {expr}"),
            Stmt::Infix { op, strength, func } => {
                write!(f, "infix {op} {strength} = {func}")
            }
        }
    }
}
