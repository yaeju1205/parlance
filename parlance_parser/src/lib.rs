// ── Recursive-descent + Pratt parser ─────────────────────────────
//
// PARSER THEORY:
//
//   The parser combines two classical techniques: RECURSIVE DESCENT
//   for the statement and expression scaffolding, and PRATT PARSING
//   (Top-Down Operator Precedence) for infix operators.
//
//   1. RECURSIVE DESCENT
//      Each grammar nonterminal has one Rust function.  The function
//      inspects the current token (1-token LOOKAHEAD = LL(1)) and
//      decides which production to follow based on the token's FIRST
//      set.  This produces clear, self-documenting code that mirrors
//      the grammar directly.
//
//   2. PRATT PARSING (Vaughan Pratt, 1973)
//      Infix operators have a numeric BINDING POWER declared at
//      runtime via `infix` statements.  The Pratt algorithm parses
//      a left operand, then loops: if the next token is an operator
//      whose binding power >= a minimum threshold (min_bp), consume
//      it, parse the right operand with threshold = bp + 1 (making
//      the operator LEFT-ASSOCIATIVE — a right operand that contains
//      the same operator must have strictly higher bp), and combine.
//
//   GRAMMAR (EBNF with FIRST sets):
//
//     program   ::= stmt* EOF
//
//     stmt      ::= import_stmt            FIRST(stmt) = {"import", "define", "infix"}
//                 | define_stmt
//                 | infix_stmt
//
//     import_stmt ::= "import" STR
//                    | "import" STR "(" IDENT ("," IDENT)* ")"
//                    | "import" STR "hiding" "(" IDENT ("," IDENT)* ")"
//                                           FIRST = {"import"}
//     define_stmt ::= "define" IDENT "=" expr
//                                           FIRST = {"define"}
//     infix_stmt  ::= "infix" OP INT "=" expr
//                                           FIRST = {"infix"}
//
//     expr      ::= seq (">>=" seq)*        FIRST(expr) = FIRST(seq)
//     seq       ::= pratt                   FIRST(seq) = FIRST(pratt)
//     pratt     ::= bind (OP bind)*         FIRST(pratt) = FIRST(bind)
//     bind      ::= "var" IDENT "<-" expr   FIRST(bind) = {"var"} ∪ FIRST(apply)
//                 | apply
//     apply     ::= atom+                   FIRST(apply) = FIRST(atom)
//     atom      ::= INT | FLOAT              FIRST(atom) = {int, float, str, ident, "(", "\"}
//                 | STR
//                 | IDENT
//                 | "(" expr ")"
//                 | "\" IDENT "=>" expr
//
//   PRODUCTION-LEVEL SEMANTICS:
//
//     import_stmt   →  parser registers a module dependency.
//     define_stmt   →  global name bound to an expression.
//     infix_stmt    →  operator registered in the binding-power table.
//     expr >>= seq  →  Seq(left, right) — the bind-chain sequence.
//     OP            →  Infix(left, op, right) — resolved by Pratt loop.
//     var x <- e    →  Bind{name: x, value: e}.
//     juxtaposition →  Apply(func, arg) — left-associative.
//     \x -> e       →  Lambda{param: x, body: e}.
//     (e)           →  e (grouping only, no AST node).

use std::collections::HashMap;

pub mod ast;

use ast::*;
use parlance_lexer::{Spanned, Token};

// ── Error type ────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

// ── Parser ────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
    /// Binding-power table populated at runtime by `infix` declarations.
    /// Higher number = tighter binding.  Maps operator string → bp.
    binding_pow: HashMap<String, u32>,
}

impl Parser {
    /// Create a new parser from a token stream produced by the
    /// DFA-based lexer (`parlance_lexer::tokenize`).
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Self {
            tokens,
            pos: 0,
            binding_pow: HashMap::new(),
        }
    }

    /// Parse the full program.
    /// Returns (statements, final binding-power table).
    pub fn parse(mut self) -> Result<(Vec<Stmt>, HashMap<String, u32>), ParseError> {
        let mut stmts = Vec::new();
        while self.peek().token != Token::Eof {
            stmts.push(self.parse_stmt()?);
        }
        Ok((stmts, self.binding_pow))
    }

    // ── Cursor operations ────────────────────────────────────────

    /// Current token (1-token LOOKAHEAD for LL(1) decisions).
    fn peek(&self) -> &Spanned {
        &self.tokens[self.pos]
    }

    /// Consume the current token and advance the cursor.
    fn advance(&mut self) -> Spanned {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    /// Location of the current token (for error messages).
    fn loc(&self) -> (usize, usize) {
        if self.pos < self.tokens.len() {
            (self.tokens[self.pos].line, self.tokens[self.pos].col)
        } else {
            let last = &self.tokens[self.tokens.len() - 1];
            (last.line, last.col)
        }
    }

    /// Produce a ParseError at the current location.
    fn err<T>(&self, msg: impl Into<String>) -> Result<T, ParseError> {
        let (line, col) = self.loc();
        Err(ParseError {
            line,
            col,
            msg: msg.into(),
        })
    }

    // ── Expect helpers ───────────────────────────────────────────
    //  These consume the expected token or produce a descriptive error.

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        if self.peek().token == *expected {
            self.advance();
            Ok(())
        } else {
            self.err(format!(
                "expected '{expected}', got '{}'",
                self.peek().token
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match &self.peek().token {
            Token::Ident(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            other => self.err(format!("expected identifier, got '{other}'")),
        }
    }

    fn expect_int(&mut self) -> Result<u32, ParseError> {
        match &self.peek().token {
            Token::Int(n) => {
                let n = *n as u32;
                self.advance();
                Ok(n)
            }
            other => self.err(format!("expected integer, got '{other}'")),
        }
    }

    fn expect_op(&mut self) -> Result<String, ParseError> {
        match &self.peek().token {
            Token::Op(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            other => self.err(format!("expected operator, got '{other}'")),
        }
    }

    fn expect_str(&mut self) -> Result<String, ParseError> {
        match &self.peek().token {
            Token::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            other => self.err(format!("expected string, got '{other}'")),
        }
    }

    // ── Statement parsing ────────────────────────────────────────
    //
    //  FIRST(stmt) = { "import", "define", "infix", "native" }
    //
    //  Each statement occupies one line and is parsed by a dedicated
    //  function that consumes exactly the tokens belonging to it.

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match &self.peek().token {
            Token::Import => self.parse_import(),
            Token::Define => self.parse_define(),
            Token::Infix => self.parse_infix_stmt(),
            Token::Native => self.parse_native(),
            other => self.err(format!(
                "expected statement (import/define/infix/native), got '{other}'"
            )),
        }
    }

    /// `import STR`
    fn parse_import(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'import'
        let path = self.expect_str()?;

        // Check for selective import
        if self.peek().token == Token::LParen {
            self.advance(); // '('
            let mut names = Vec::new();
            loop {
                names.push(self.expect_ident()?);
                if self.peek().token == Token::Comma {
                    self.advance(); // ','
                } else {
                    break;
                }
            }
            self.expect(&Token::RParen)?;
            return Ok(Stmt::Import {
                path,
                spec: ImportSpec::Only(names),
            });
        }

        Ok(Stmt::Import {
            path,
            spec: ImportSpec::All,
        })
    }

    /// `define IDENT [: type] = expr`
    fn parse_define(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'define'
        let name = self.expect_ident()?;
        let type_sig = if self.peek().token == Token::Colon {
            self.advance(); // ':'
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Equal)?;
        let expr = self.parse_expr()?;
        Ok(Stmt::Define { name, type_sig, expr })
    }

    /// `infix OP INT = expr`
    fn parse_infix_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'infix'
        let op = self.expect_op()?;
        let bp = self.expect_int()?;
        self.expect(&Token::Equal)?;
        let func = self.parse_expr()?;
        // Register the binding power for subsequent expressions.
        self.binding_pow.insert(op.clone(), bp);
        Ok(Stmt::Infix {
            op,
            strength: bp,
            func,
        })
    }

    /// `native IDENT : type`
    fn parse_native(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'native'
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let type_sig = self.parse_type()?;
        Ok(Stmt::Native { name, type_sig })
    }

    // ── Type parsing ──────────────────────────────────────────────
    //
    //   type_atom  ::= "Int" | "Float" | "Bool" | "Str" | "(" type ")"
    //   type       ::= type_atom { "->" type }
    //
    //   Right-associative: a -> b -> c = Fun(a, Fun(b, c))

    /// Parse a type expression (right-associative arrows).
    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let atom = self.parse_type_atom()?;
        if self.peek().token == Token::Arrow {
            self.advance(); // '->'
            let ret = self.parse_type()?;
            Ok(Type::Fun(Box::new(atom), Box::new(ret)))
        } else {
            Ok(atom)
        }
    }

    /// Parse a type atom: a built-in name or parenthesized type.
    fn parse_type_atom(&mut self) -> Result<Type, ParseError> {
        match &self.peek().token {
            Token::Ident(s) => {
                let ty = match s.as_str() {
                    "Int" => Type::Int,
                    "Float" => Type::Float,
                    "Bool" => Type::Bool,
                    "Str" => Type::Str,
                    other => return self.err(format!("unknown type '{other}'")),
                };
                self.advance();
                Ok(ty)
            }
            Token::LParen => {
                self.advance(); // '('
                let ty = self.parse_type()?;
                self.expect(&Token::RParen)?;
                Ok(ty)
            }
            other => self.err(format!("expected type, got '{other}'")),
        }
    }

    // ── Expression parsing ───────────────────────────────────────
    //
    //  Expression hierarchy (loosest → tightest):
    //
    //    parse_expr   →  >>= sequencing (left-assoc)
    //      └─ parse_pratt  →  Pratt-parsed infix operators
    //           └─ parse_bind  →  "var" IDENT "<-" expr
    //                └─ parse_apply  →  juxtaposition (left-assoc)
    //                     └─ parse_atom  →  literals / parens / lambda

    /// `>>=` sequencing — the loosest level.
    ///
    ///   expr ::= pratt (">>=" pratt)*
    ///
    /// LEFT-ASSOCIATIVE: a >>= b >>= c  →  Seq(Seq(a, b), c)
    /// The operands are parsed at the Pratt level so that they consume
    /// infix operators but stop at >>= (BindChain is not Token::Op).
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pratt(0)?;
        while self.peek().token == Token::BindChain {
            self.advance();
            let right = self.parse_pratt(0)?;
            left = Expr::Seq(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Pratt parser: parse an infix expression whose operators all
    /// have binding-power >= `min_bp`.
    ///
    ///   pratt ::= bind (OP bind)*
    ///
    /// Left-associative: the right operand demands `bp + 1`, so the
    /// Pratt loop only accepts an operator of *equal* binding power
    /// when it is on the left side of a new occurrence.
    fn parse_pratt(&mut self, min_bp: u32) -> Result<Expr, ParseError> {
        let mut left = self.parse_bind()?;

        loop {
            let op = match &self.peek().token {
                Token::Op(s) => s.clone(),
                _ => break,
            };
            let bp = match self.binding_pow.get(&op) {
                Some(&bp) => bp,
                None => break, // undeclared operator — not infix, stop.
            };
            if bp < min_bp {
                break;
            }

            self.advance();
            let right = self.parse_pratt(bp + 1)?;
            left = Expr::Infix(Box::new(left), op, Box::new(right));
        }

        Ok(left)
    }

    /// Local variable binding or application.
    ///
    ///   bind ::= "var" IDENT "<-" expr | apply
    ///
    /// FIRST(bind) = { "var" } ∪ FIRST(apply)
    fn parse_bind(&mut self) -> Result<Expr, ParseError> {
        if self.peek().token == Token::Var {
            self.advance(); // 'var'
            let name = self.expect_ident()?;
            self.expect(&Token::Bind)?; // '<-'
            let value = self.parse_pratt(0)?;
            // The value is parsed at the Pratt level so infix operators
            // are consumed, but >>= (BindChain) is NOT consumed because
            // it is Token::BindChain, not Token::Op.
            return Ok(Expr::Bind {
                name,
                value: Box::new(value),
            });
        }
        self.parse_apply()
    }

    /// Function application by juxtaposition.
    ///
    ///   apply ::= atom+
    ///
    /// LEFT-ASSOCIATIVE: f x y  →  Apply(Apply(Var("f"), Var("x")), Var("y"))
    fn parse_apply(&mut self) -> Result<Expr, ParseError> {
        let mut func = self.parse_atom()?;
        while self.starts_atom() {
            let arg = self.parse_atom()?;
            func = Expr::Apply(Box::new(func), Box::new(arg));
        }
        Ok(func)
    }

    /// Set of tokens that can begin an atom.
    /// Used by `parse_apply` to decide whether to loop.
    fn starts_atom(&self) -> bool {
        matches!(
            &self.peek().token,
            Token::Int(_)
                | Token::Float(_)
                | Token::Str(_)
                | Token::Ident(_)
                | Token::LParen
                | Token::Backslash
        )
    }

    /// Atomic expression.
    ///
    ///   atom ::= INT | FLOAT | STR | IDENT
    ///          | "(" expr ")"
    ///          | "\" IDENT "->" expr
    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let sp = self.advance();
        let (line, col) = (sp.line, sp.col);

        match sp.token {
            Token::Int(n) => Ok(Expr::Int(n)),
            Token::Float(n) => Ok(Expr::Float(n)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::Ident(s) => Ok(Expr::Var(s)),

            Token::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }

            Token::Backslash => {
                // \param -> body
                let param = self.expect_ident()?;
                self.expect(&Token::Arrow)?;
                let body = self.parse_expr()?;
                Ok(Expr::Lambda {
                    param,
                    body: Box::new(body),
                })
            }

            other => {
                return Err(ParseError {
                    line,
                    col,
                    msg: format!("expected expression (int/str/ident/(/\\\\), got '{other}'"),
                });
            }
        }
    }
}

// ── Convenience entry point ──────────────────────────────────────

/// Tokenize then parse in one call.
pub fn parse_program(src: &str) -> Result<(Vec<Stmt>, HashMap<String, u32>), String> {
    let tokens = parlance_lexer::tokenize(src)?;
    Parser::new(tokens)
        .parse()
        .map_err(|e| format!("{}:{}: {}", e.line, e.col, e.msg))
}

// ── Tests ─────────────────────────────────────────────────────────
//  Each test exercises one grammar production end-to-end.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_parse_import() {
        let (stmts, _) = parse_program(r#"import "foo""#).unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Stmt::Import { path, spec: ImportSpec::All } if path == "foo"));
    }

    #[test]
    fn smoke_parse_define_ident() {
        let (stmts, _) = parse_program("define x = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Stmt::Define { name, .. } if name == "x"));
    }

    #[test]
    fn smoke_parse_lambda() {
        let (stmts, _) = parse_program(r"define id = \x -> x").unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Lambda { param, .. } if param == "x"));
    }

    #[test]
    fn smoke_parse_apply() {
        let (stmts, _) = parse_program("define f = g h i").unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Apply(..)));
        if let Expr::Apply(inner, _) = expr {
            assert!(matches!(inner.as_ref(), Expr::Apply(..)));
        }
    }

    #[test]
    fn smoke_parse_infix() {
        let src = r#"
            infix + 5 = add
            define x = 1 + 2
        "#;
        let (stmts, prec) = parse_program(src).unwrap();
        assert_eq!(stmts.len(), 2);
        assert_eq!(prec.get("+"), Some(&5));
        let expr = match &stmts[1] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Infix(..)));
    }

    #[test]
    fn smoke_parse_infix_precedence() {
        let src = r#"
            infix + 5 = add
            infix * 6 = mul
            define x = 1 + 2 * 3
        "#;
        let (stmts, _) = parse_program(src).unwrap();
        let expr = match &stmts[2] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        // With * tighter (bp=6) than + (bp=5):  1 + (2 * 3)
        // → Infix(Int(1), +, Infix(Int(2), *, Int(3)))
        assert!(matches!(expr, Expr::Infix(..)));
        if let Expr::Infix(l, op, r) = expr {
            assert_eq!(op, "+");
            assert!(matches!(l.as_ref(), Expr::Int(1)));
            assert!(matches!(r.as_ref(), Expr::Infix(inner, op2, _)
                if matches!(inner.as_ref(), Expr::Int(2)) && op2 == "*"));
        }
    }

    #[test]
    fn smoke_parse_bind_chain() {
        let src = "define r = var x <- 1 >>= var y <- 2 >>= x";
        let (stmts, _) = parse_program(src).unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        // >>= is left-assoc:  Seq(Seq(Bind(x,1), Bind(y,2)), Var(x))
        assert!(matches!(expr, Expr::Seq(..)));
    }

    #[test]
    fn smoke_parse_parens() {
        let (stmts, _) = parse_program("define x = (1)").unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Int(1)));
    }

    #[test]
    fn smoke_parse_error_bad_token() {
        let err = parse_program("define x = @").unwrap_err();
        assert!(err.contains("expected expression"), "got: {err}");
    }

    #[test]
    fn smoke_parse_error_eof() {
        let err = parse_program("define x = ").unwrap_err();
        assert!(err.contains("expected expression"), "got: {err}");
    }

    #[test]
    fn smoke_parse_float() {
        let (stmts, _) = parse_program("define x = 3.14").unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Float(n) if (n - 3.14).abs() < 1e-10));
    }

    #[test]
    fn smoke_parse_int_not_float() {
        let (stmts, _) = parse_program("define x = 42").unwrap();
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        assert!(matches!(expr, Expr::Int(42)));
    }
}
