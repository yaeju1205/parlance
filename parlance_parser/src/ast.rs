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

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Type variable (generic): a, b, c ...
    TVar(String),
    /// Type constructor: Int, Float, Bool, Str, IO ...
    TCon(String),
    /// Fun(param, result) = param -> result
    Fun(Box<Type>, Box<Type>),
}

impl Type {
    /// Check if this type has a specific constructor name.
    pub fn is(&self, name: &str) -> bool {
        matches!(self, Type::TCon(s) if s == name)
    }

    /// Count the number of arguments in a curried function type.
    /// e.g. Int -> Int -> Int  ⇒  arity = 2
    pub fn arity(&self) -> u32 {
        match self {
            Type::Fun(_, ret) => 1 + ret.arity(),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    // ── Literals ─────────────────────────────────────────────────
    Int(i64),
    Float(f64),
    Str(String),

    // ── Reference (variable use) ─────────────────────────────────
    Var(String),

    // ── Lambda abstraction ───────────────────────────────────────
    Lambda { param: String, body: Box<Expr> },

    // ── Application (function call by juxtaposition) ─────────────
    Apply(Box<Expr>, Box<Expr>),

    // ── Infix operation (desugared later by semant) ──────────────
    Infix(Box<Expr>, String, Box<Expr>),

    // ── Local variable binding ───────────────────────────────────
    Bind { name: String, value: Box<Expr> },

    // ── Sequencing for bind-chains ───────────────────────────────
    Seq(Box<Expr>, Box<Expr>),
}

// ── Import specification ────────────────────────────────────────

/// Controls which names are imported from a module.
#[derive(Debug, Clone)]
pub enum ImportSpec {
    All,
    Only(Vec<String>),
}

/// A resolved module: the set of names a module exports.
#[derive(Debug, Clone)]
pub struct Module {
    pub path: String,
    pub exports: Vec<Export>,
}

/// A single exported binding from a module.
#[derive(Debug, Clone)]
pub enum Export {
    Define { name: String, expr: Expr },
    Infix { op: String, strength: u32, func: Expr },
}

// ── Statements ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `import "path"` or `import "path" (names...)`
    Import { path: String, spec: ImportSpec },

    /// `define name [: type] = expr`
    Define { name: String, type_sig: Option<Type>, expr: Expr },

    /// `infix op strength = func`
    Infix { op: String, strength: u32, func: Expr },

    /// `native name : type`
    Native { name: String, type_sig: Type },
}

// ── Display ──────────────────────────────────────────────────────

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::TVar(v) => write!(f, "{v}"),
            Type::TCon(c) => write!(f, "{c}"),
            Type::Fun(param, ret) => {
                // Parenthesize param if it's also a Fun (right-assoc)
                match param.as_ref() {
                    Type::Fun(_, _) => write!(f, "({param} -> {ret})"),
                    _ => write!(f, "{param} -> {ret}"),
                }
            }
        }
    }
}

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
            Stmt::Define { name, type_sig, expr } => {
                write!(f, "define {name}")?;
                if let Some(ty) = type_sig {
                    write!(f, " : {ty}")?;
                }
                write!(f, " = {expr}")
            }
            Stmt::Infix { op, strength, func } => {
                write!(f, "infix {op} {strength} = {func}")
            }
            Stmt::Native { name, type_sig } => {
                write!(f, "native {name} : {type_sig}")
            }
        }
    }
}
