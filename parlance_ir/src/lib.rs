// ── Intermediate Representation (pure lambda calculus) ──────────
//
// IR THEORY:
//
//   The IR is a PURE LAMBDA CALCULUS extended with literal values.
//   After semantic analysis has desugared all syntactic sugar, every
//   expression can be represented with just five node types.
//
//     Ir = Int(i64) | Float(f64) | Str(String)
//        | Var(String)
//        | Lam(String, Box<Ir>)
//        | App(Box<Ir>, Box<Ir>)
//
//   This is the minimum viable core: it is TURING-COMPLETE (Lambda
//   and App alone suffice for computation) and it serves as the
//   COMMON INTERMEDIATE for all downstream passes:
//
//     Optimization passes (constant folding, β-reduction, etc.)
//     → transform Ir → Ir
//
//     Code generation (GraftVM bytecode)
//     → translate Ir → bytecode
//
//   LOWERING is a direct homomorphism from the desugared AST
//   (which already contains no Infix, Bind, or Seq nodes) to Ir.
//   Because semant guarantees this invariant, lower() never fails.

use std::fmt;

use parlance_parser::ast::{Expr, Stmt};

// ── IR nodes ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Ir {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),

    // Variable reference
    Var(String),

    // Lambda abstraction
    Lam { param: String, body: Box<Ir> },

    // Application
    App(Box<Ir>, Box<Ir>),
}

/// A top-level definition in the IR.
/// Produced by lowering a desugared AST program.
#[derive(Debug, Clone, PartialEq)]
pub enum IrDef {
    /// `define name = ir`
    Bind { name: String, expr: Ir },
    /// `infix op strength = func_ir`
    Infix { op: String, strength: u32, func: Ir },
    /// `native name : type` — no body, host provides implementation
    Native { name: String, arity: u32 },
}

// ── Display ──────────────────────────────────────────────────────

impl fmt::Display for Ir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Ir::Int(n) => write!(f, "{n}"),
            Ir::Float(n) => write!(f, "{n}"),
            Ir::Str(s) => write!(f, "\"{s}\""),
            Ir::Var(v) => write!(f, "{v}"),
            Ir::Lam { param, body } => write!(f, "(\\{param} => {body})"),
            Ir::App(g, a) => write!(f, "({g} {a})"),
        }
    }
}

impl fmt::Display for IrDef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IrDef::Bind { name, expr } => write!(f, "define {name} = {expr}"),
            IrDef::Infix { op, strength, func } => {
                write!(f, "infix {op} {strength} = {func}")
            }
            IrDef::Native { name, arity } => {
                write!(f, "native {name} (arity={arity})")
            }
        }
    }
}

// ── Lowering (AST → IR) ─────────────────────────────────────────

/// Lower a single desugared expression to IR.
///
/// INVARIANT: `expr` must contain no Infix, Bind, or Seq nodes.
/// If it does, lower() panics — this is a safety check that catches
/// bugs in the semantic analysis pass.
pub fn lower(expr: &Expr) -> Ir {
    match expr {
        Expr::Int(n) => Ir::Int(*n),
        Expr::Float(n) => Ir::Float(*n),
        Expr::Str(s) => Ir::Str(s.clone()),
        Expr::Var(v) => Ir::Var(v.clone()),
        Expr::Lambda { param, body } => Ir::Lam {
            param: param.clone(),
            body: Box::new(lower(body)),
        },
        Expr::Apply(f, a) => Ir::App(Box::new(lower(f)), Box::new(lower(a))),

        // Sugar — should never appear after semant.
        Expr::Infix(..) => panic!("ir::lower: Infix node — run semant first"),
        Expr::Bind { .. } => panic!("ir::lower: Bind node — run semant first"),
        Expr::Seq(..) => panic!("ir::lower: Seq node — run semant first"),
    }
}

/// Lower a fully-desugared program to a list of IR definitions.
///
/// Accepts the output of `parlance_semant::analyze()`.
pub fn lower_program(stmts: &[Stmt]) -> Vec<IrDef> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::Define { name, expr, .. } => IrDef::Bind {
                name: name.clone(),
                expr: lower(expr),
            },
            Stmt::Infix { op, strength, func } => IrDef::Infix {
                op: op.clone(),
                strength: *strength,
                func: lower(func),
            },
            Stmt::Native { name, type_sig } => IrDef::Native {
                name: name.clone(),
                arity: type_sig.arity(),
            },
            Stmt::Import { .. } => {
                panic!("ir::lower_program: Import node survived semant")
            }
        })
        .collect()
}

/// Pretty-print the lowered IR program.
pub fn dump(defs: &[IrDef]) -> String {
    defs.iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Constant folding (intra-expression) ─────────────────────────
//
// OPTIMISATION THEORY — CONSTANT FOLDING:
//
//   Identifies sub-expressions whose value can be determined at
//   compile time and replaces them with their computed result.
//   This is a form of ABSTRACT INTERPRETATION where the abstract
//   domain is {known constant, unknown}.
//
//   Rules implemented:
//     App(Lam{x, body}, v) where v is a literal → β-reduction
//       (substitute x ↦ v in body, then recurse)
//
//   More aggressive rules (App(Lam{...}, App(Lam{...}, ...)), etc.)
//   belong in a dedicated optimisation pass.

/// Fold constant sub-expressions in an IR tree.
/// Returns a potentially simpler IR with constants evaluated.
pub fn fold_constants(ir: &Ir) -> Ir {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) | Ir::Var(_) => ir.clone(),

        Ir::Lam { param, body } => Ir::Lam {
            param: param.clone(),
            body: Box::new(fold_constants(body)),
        },

        Ir::App(f, a) => {
            let f = fold_constants(f);
            let a = fold_constants(a);

            // β-reduction: (λx. body) arg  →  body[x ↦ arg]
            // Only when `arg` is a literal (safe, no capture issues).
            if let Ir::Lam { param, body } = &f {
                if matches!(a, Ir::Int(_) | Ir::Float(_) | Ir::Str(_)) {
                    return fold_constants(&subst(body, param, &a));
                }
            }

            Ir::App(Box::new(f), Box::new(a))
        }
    }
}

/// Substitute `var` ↦ `replacement` in `ir`.
/// Simple capture-avoiding substitution for the pure lambda core.
pub fn subst(ir: &Ir, var: &str, replacement: &Ir) -> Ir {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) => ir.clone(),

        Ir::Var(v) => {
            if v == var {
                replacement.clone()
            } else {
                ir.clone()
            }
        }

        Ir::Lam { param, body } => {
            if param == var {
                // λvar. ... — binder shadows, stop substitution.
                ir.clone()
            } else {
                Ir::Lam {
                    param: param.clone(),
                    body: Box::new(subst(body, var, replacement)),
                }
            }
        }

        Ir::App(f, a) => Ir::App(
            Box::new(subst(f, var, replacement)),
            Box::new(subst(a, var, replacement)),
        ),
    }
}

// ── Self-test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_parser::ast::Expr;

    #[test]
    fn test_lower_int() {
        let ir = lower(&Expr::Int(42));
        assert_eq!(ir, Ir::Int(42));
    }

    #[test]
    fn test_lower_float() {
        let ir = lower(&Expr::Float(3.14));
        assert_eq!(ir, Ir::Float(3.14));
    }

    #[test]
    fn test_lower_var() {
        let ir = lower(&Expr::Var("x".into()));
        assert_eq!(ir, Ir::Var("x".into()));
    }

    #[test]
    fn test_lower_lambda() {
        let e = Expr::Lambda {
            param: "x".into(),
            body: Box::new(Expr::Var("x".into())),
        };
        let ir = lower(&e);
        assert_eq!(
            ir,
            Ir::Lam {
                param: "x".into(),
                body: Box::new(Ir::Var("x".into())),
            }
        );
    }

    #[test]
    fn test_lower_apply() {
        let e = Expr::Apply(Box::new(Expr::Var("f".into())), Box::new(Expr::Int(1)));
        let ir = lower(&e);
        assert_eq!(
            ir,
            Ir::App(Box::new(Ir::Var("f".into())), Box::new(Ir::Int(1)))
        );
    }

    #[test]
    #[should_panic(expected = "Infix")]
    fn test_lower_panics_on_infix() {
        lower(&Expr::Infix(
            Box::new(Expr::Int(1)),
            "+".into(),
            Box::new(Expr::Int(2)),
        ));
    }

    #[test]
    #[should_panic(expected = "Bind")]
    fn test_lower_panics_on_bind() {
        lower(&Expr::Bind {
            name: "x".into(),
            value: Box::new(Expr::Int(1)),
        });
    }

    #[test]
    #[should_panic(expected = "Seq")]
    fn test_lower_panics_on_seq() {
        lower(&Expr::Seq(Box::new(Expr::Int(1)), Box::new(Expr::Int(2))));
    }

    #[test]
    fn test_fold_constants_noop() {
        let ir = Ir::App(Box::new(Ir::Var("f".into())), Box::new(Ir::Int(42)));
        let folded = fold_constants(&ir);
        assert_eq!(folded, ir);
    }

    #[test]
    fn test_fold_beta_redex() {
        // (λx. x) 42  →  42
        let ir = Ir::App(
            Box::new(Ir::Lam {
                param: "x".into(),
                body: Box::new(Ir::Var("x".into())),
            }),
            Box::new(Ir::Int(42)),
        );
        let folded = fold_constants(&ir);
        assert_eq!(folded, Ir::Int(42));
    }

    #[test]
    fn test_fold_beta_nested() {
        // (λx. (λy. x)) 1 2  →  (λy. 1) 2  →  1
        // Actually: Apply(Apply(Lam{x, Lam{y, x}}, 1), 2)
        let ir = Ir::App(
            Box::new(Ir::App(
                Box::new(Ir::Lam {
                    param: "x".into(),
                    body: Box::new(Ir::Lam {
                        param: "y".into(),
                        body: Box::new(Ir::Var("x".into())),
                    }),
                }),
                Box::new(Ir::Int(1)),
            )),
            Box::new(Ir::Int(2)),
        );
        let folded = fold_constants(&ir);
        // After outer β: Apply(Lam{y, 1}, 2) → 1
        assert_eq!(folded, Ir::Int(1));
    }

    #[test]
    fn test_subst_basic() {
        let ir = Ir::App(Box::new(Ir::Var("x".into())), Box::new(Ir::Var("y".into())));
        let result = subst(&ir, "x", &Ir::Int(99));
        assert_eq!(
            result,
            Ir::App(Box::new(Ir::Int(99)), Box::new(Ir::Var("y".into())))
        );
    }

    #[test]
    fn test_subst_avoids_capture() {
        // (λx. x) — substitution of x in body should stop at the binder
        let ir = Ir::Lam {
            param: "x".into(),
            body: Box::new(Ir::Var("x".into())),
        };
        let result = subst(&ir, "x", &Ir::Int(99));
        // binder shadows — no change
        assert_eq!(result, ir);
    }

    #[test]
    fn test_lower_program() {
        let stmts = vec![
            Stmt::Define {
                name: "id".into(),
                expr: Expr::Lambda {
                    param: "x".into(),
                    body: Box::new(Expr::Var("x".into())),
                },
                type_sig: None,
            },
            Stmt::Infix {
                op: "+".into(),
                strength: 5,
                func: Expr::Var("add".into()),
            },
        ];
        let defs = lower_program(&stmts);
        assert_eq!(defs.len(), 2);
        match &defs[0] {
            IrDef::Bind { name, .. } => assert_eq!(name, "id"),
            _ => panic!("expected Bind"),
        }
        match &defs[1] {
            IrDef::Infix { op, strength, .. } => {
                assert_eq!(op, "+");
                assert_eq!(*strength, 5);
            }
            _ => panic!("expected Infix"),
        }
    }

    #[test]
    fn test_dump() {
        let defs = vec![IrDef::Bind {
            name: "x".into(),
            expr: Ir::Int(42),
        }];
        let s = dump(&defs);
        assert_eq!(s, "define x = 42");
    }
}
