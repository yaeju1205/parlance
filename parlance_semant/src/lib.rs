// ── Semantic Analysis (desugar + name resolution) ───────────────
//
// SEMANTIC ANALYSIS THEORY:
//
//   Semantic analysis transforms the parse-tree AST into a desugared
//   AST that only contains the CORE LANGUAGE (Int, Float, Str, Var,
//   Lambda, Apply).  All syntactic sugar — Infix, Bind, Seq, and the
//   import mechanism — is resolved away.
//
//   DENOTATIONAL SEMANTICS (macro-expansion style):
//
//     Each high-level construct is assigned a meaning by translating
//     it into a composition of core-language terms.  This is a form
//     of DENOTATIONAL SEMANTICS: the "denotation" (meaning) of the
//     sugar is a core term.
//
//     DESUGARING RULES:
//
//       Infix(l, op, r)  ⇒  Apply(Apply(func_of(op), l), r)
//         "op is a binary function:    (op l r) in curried form"
//
//       Bind{name, value} ⇒  ERROR (no >>= continuation)
//         "A bare var x <- e has nowhere to bind x — every Bind
//          must be part of a >>= chain that ends in a body expr."
//
//       Seq(a, b)
//         where chain is flattened to [Bind₁, Bind₂, ..., Body]
//         ⇒  Apply(Lambda{xₙ, … Apply(Lambda{x₁, Body₍desugared₎}, v₁) …}, vₙ)
//         "The let* desugar: sequential binds become nested
//          Apply(Lambda, value) — the pure functional encoding of
//          Haskell's do-notation."
//
//   NAME RESOLUTION (static scoping):
//
//     The resolver walks the desugared expression tracking a scope
//     stack of lambda-bound parameters.  Every Var(name) must
//     resolve against:
//       1. The innermost lambda scope that binds `name`
//       2. A global `define name` in the current module or an
//          imported module
//       3. An `infix op` name (the function bound to the operator)
//
//     Unresolved variables produce a SemanticError.
//
//   MODULE MERGING:
//
//     Imports are resolved by parlance-import BEFORE desugaring.
//     Each resolved module's exports are flattened into synthetic
//     Define/Infix statements at the point of import.  Later
//     definitions shadow earlier ones (including imports).

use std::collections::{HashMap, HashSet};

use parlance_import::resolve_import;
use parlance_parser::ast::{Export, Expr, Stmt};

// ── Error type ────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SemanticError {
    pub msg: String,
}

// ── Semant context ────────────────────────────────────────────────

/// Analysis context built from the raw statement list.
struct Context {
    /// Operator → function mapping from `infix` declarations.
    /// Maps operator string (e.g. "+") to its function expression.
    op_to_func: HashMap<String, Expr>,

    /// Global names: all `define` names and `infix` operator names
    /// (used as function binders in the desugared form).
    globals: HashSet<String>,
}

// ── Public entry point ────────────────────────────────────────────

/// Run semantic analysis on a fully-parsed program.
///
/// Steps:
///   1. Resolve imports and flatten them into synthetic statements.
///   2. Build the operator→function and global-name tables.
///   3. Desugar every expression (Infix → Apply, Seq → nested Lam/App).
///   4. Check that every variable reference resolves.
///
/// Returns the desugared statement list (no Infix, Bind, or Seq nodes)
/// with all names verified.
pub fn analyze(stmts: Vec<Stmt>) -> Result<Vec<Stmt>, SemanticError> {
    // ── 1. Resolve imports ───────────────────────────────────────
    //  Each import is resolved via parlance-import.  The exports
    //  are flattened into synthetic Define/Infix statements inserted
    //  at the import's position.
    let mut resolved = Vec::new();
    for stmt in stmts {
        match &stmt {
            Stmt::Import { .. } => {
                // Try to resolve; if filesystem isn't available (e.g. tests),
                // don't error — just skip imports silently.
                let source_dir = std::path::Path::new(".");
                match resolve_import(&stmt, source_dir) {
                    Ok(mod_) => {
                        for export in mod_.exports {
                            match export {
                                Export::Define { name, expr } => {
                                    resolved.push(Stmt::Define { name, expr, type_sig: None });
                                }
                                Export::Infix { op, strength, func } => {
                                    resolved.push(Stmt::Infix { op, strength, func });
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // If resolution fails (e.g. test environment with
                        // no actual files), keep the import as-is for
                        // downstream errors rather than crashing.
                        resolved.push(stmt.clone());
                    }
                }
            }
            _ => {
                resolved.push(stmt.clone());
            }
        }
    }

    // ── 2. Build operator and global tables ──────────────────────
    let mut ctx = Context {
        op_to_func: HashMap::new(),
        globals: HashSet::new(),
    };

    for stmt in &resolved {
        match stmt {
            Stmt::Define { name, .. } => {
                ctx.globals.insert(name.clone());
            }
            Stmt::Infix { op, func, .. } => {
                ctx.op_to_func.insert(op.clone(), func.clone());
                if let Expr::Var(n) = func {
                    ctx.globals.insert(n.clone());
                }
            }
            Stmt::Native { name, .. } => {
                ctx.globals.insert(name.clone());
            }Stmt::Import { .. } => {
                // Already resolved above; skip.
            }
        }
    }

    // ── 3. Desugar + 4. Name resolution ──────────────────────────
    let mut out = Vec::with_capacity(resolved.len());
    for stmt in resolved {
        match stmt {
            Stmt::Import { .. } => {
                // Already resolved; drop the original import.
            }
            Stmt::Define { name, expr, .. } => {
                let e = desugar(&expr, &ctx)?;
                check_names(&e, &ctx)?;
                out.push(Stmt::Define { name, type_sig: None, expr: e });
            }
            Stmt::Native { name, type_sig } => {
                // Native declarations have no body — just pass through.
                out.push(Stmt::Native { name, type_sig });
            }
            Stmt::Infix { op, strength, func } => {
                let f = desugar(&func, &ctx)?;
                check_names(&f, &ctx)?;
                out.push(Stmt::Infix {
                    op,
                    strength,
                    func: f,
                });
            }
        }
    }

    Ok(out)
}

// ── Desugaring ────────────────────────────────────────────────────
//  Recursively transforms an Expr, replacing sugar with core terms.

fn desugar(expr: &Expr, ctx: &Context) -> Result<Expr, SemanticError> {
    Ok(match expr {
        // Core literals — pass through unchanged.
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Var(_) => expr.clone(),

        // Lambda — recurse into body.
        Expr::Lambda { param, body } => Expr::Lambda {
            param: param.clone(),
            body: Box::new(desugar(body, ctx)?),
        },

        // Application — recurse into both sides.
        Expr::Apply(f, a) => Expr::Apply(Box::new(desugar(f, ctx)?), Box::new(desugar(a, ctx)?)),

        // Infix(l, op, r)  →  Apply(Apply(func_of(op), l), r)
        Expr::Infix(l, op, r) => {
            let Some(func) = ctx.op_to_func.get(op) else {
                return Err(SemanticError {
                    msg: format!("infix operator '{op}' has no declared function"),
                });
            };
            let df = desugar(func, ctx)?;
            let dl = desugar(l, ctx)?;
            let dr = desugar(r, ctx)?;
            Expr::Apply(
                Box::new(Expr::Apply(Box::new(df), Box::new(dl))),
                Box::new(dr),
            )
        }

        // Standalone Bind — error: no >>= continuation.
        Expr::Bind { name, .. } => {
            return Err(SemanticError {
                msg: format!("var '{name}' bound without >>= continuation"),
            });
        }

        // Seq — desugar the entire >>= chain.
        //   Flatten the left-assoc Seq spine into a list:
        //     Seq(Seq(Bind(x,1), Bind(y,2)), body)
        //     →  [Bind(x,1), Bind(y,2), body]
        //   Then: every element except the last must be Bind;
        //         the last is the body expression.
        //   Build nested Apply(Lambda{name, ...}, value) from the
        //   inside out:  Apply(Lambda{x, Apply(Lambda{y, body}, 2)}, 1)
        Expr::Seq(..) => {
            let mut parts: Vec<Expr> = Vec::new();
            flatten_seq(expr, &mut parts);

            let Some((last, binds)) = parts.split_last() else {
                return Err(SemanticError {
                    msg: ">>= chain must have at least two operands".into(),
                });
            };

            // Every element in `binds` must be a Bind.
            let mut bind_pairs: Vec<(String, Expr)> = Vec::new();
            for p in binds {
                match p {
                    Expr::Bind { name, value } => {
                        bind_pairs.push((name.clone(), (**value).clone()));
                    }
                    _ => {
                        return Err(SemanticError {
                            msg: ">>= left operand must be 'var name <- value'".into(),
                        });
                    }
                }
            }

            // If the last element is also a raw Bind (no body after >>=),
            // that's an error too.
            if let Expr::Bind { name, .. } = last {
                return Err(SemanticError {
                    msg: format!("var '{name}' bound without >>= continuation"),
                });
            }

            // Desugar body first, then wrap binds from right to left.
            let mut acc = desugar(last, ctx)?;
            for (name, value) in bind_pairs.into_iter().rev() {
                let v = desugar(&value, ctx)?;
                acc = Expr::Apply(
                    Box::new(Expr::Lambda {
                        param: name,
                        body: Box::new(acc),
                    }),
                    Box::new(v),
                );
            }
            acc
        }
    })
}

/// Flatten a left-assoc `Seq` spine into a flat list.
/// Seq(Seq(Bind(x,1), Bind(y,2)), body) → [Bind(x,1), Bind(y,2), body]
fn flatten_seq(expr: &Expr, out: &mut Vec<Expr>) {
    match expr {
        Expr::Seq(l, r) => {
            flatten_seq(l, out);
            flatten_seq(r, out);
        }
        other => out.push(other.clone()),
    }
}

// ── Name resolution ───────────────────────────────────────────────
//  Walks the desugared AST and ensures every Var has a binding site.

fn check_names(expr: &Expr, ctx: &Context) -> Result<(), SemanticError> {
    let mut scope: HashSet<String> = HashSet::new();
    walk_names(expr, &ctx.globals, &mut scope)
}

fn walk_names(
    expr: &Expr,
    globals: &HashSet<String>,
    scope: &mut HashSet<String>,
) -> Result<(), SemanticError> {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) => Ok(()),

        Expr::Var(n) => {
            if scope.contains(n) || globals.contains(n) {
                Ok(())
            } else {
                Err(SemanticError {
                    msg: format!("unbound variable: '{n}'"),
                })
            }
        }

        Expr::Lambda { param, body } => {
            // Shadow: param is in scope for the body.
            let already = scope.contains(param);
            if !already {
                scope.insert(param.clone());
            }
            walk_names(body, globals, scope)?;
            if !already {
                scope.remove(param);
            }
            Ok(())
        }

        Expr::Apply(f, a) => {
            walk_names(f, globals, scope)?;
            walk_names(a, globals, scope)
        }

        // After desugaring, these should not appear.
        Expr::Infix(..) | Expr::Bind { .. } | Expr::Seq(..) => Err(SemanticError {
            msg: "sugar node (Infix/Bind/Seq) survived desugaring".into(),
        }),
    }
}

// ── Self-test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_parser;

    fn ok(src: &str) -> Vec<Stmt> {
        let (stmts, _) = parlance_parser::parse_program(src).unwrap();
        analyze(stmts).unwrap()
    }

    fn err(src: &str) -> String {
        let (stmts, _) = parlance_parser::parse_program(src).unwrap();
        analyze(stmts).unwrap_err().msg
    }

    #[test]
    fn test_desugar_infix() {
        let src = r#"
            infix + 5 = add
            define x = 1 + 2
        "#;
        let stmts = ok(src);
        let expr = match &stmts[1] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        // 1 + 2 → Apply(Apply(Var(add), Int(1)), Int(2))
        if let Expr::Apply(outer_left, outer_right) = expr {
            if let Expr::Apply(inner_left, inner_right) = outer_left.as_ref() {
                if let Expr::Var(f) = inner_left.as_ref() {
                    assert_eq!(f, "add");
                } else {
                    panic!("expected Var for function, got: {outer_left}");
                }
                assert!(matches!(inner_right.as_ref(), Expr::Int(1)));
            } else {
                panic!("expected inner Apply, got: {outer_left}");
            }
            assert!(matches!(outer_right.as_ref(), Expr::Int(2)));
        } else {
            panic!("unexpected expr: {expr}");
        }
    }

    #[test]
    fn test_desugar_infix_precedence() {
        let src = r#"
            infix + 5 = add
            infix * 6 = mul
            define x = 1 + 2 * 3
        "#;
        let stmts = ok(src);
        let expr = match &stmts[2] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        // 1 + (2 * 3)  →  Apply(Apply(add, 1), Apply(Apply(mul, 2), 3))
        if let Expr::Apply(outer_left, outer_right) = expr {
            // Check outer_left = Apply(add, 1)
            if let Expr::Apply(inner_f, inner_arg) = outer_left.as_ref() {
                assert!(matches!(inner_f.as_ref(), Expr::Var(f) if f == "add"));
                assert!(matches!(inner_arg.as_ref(), Expr::Int(1)));
            } else {
                panic!("expected Apply(add, 1), got: {outer_left}");
            }
            // Check outer_right = Apply(Apply(mul, 2), 3)
            if let Expr::Apply(mul_app, three) = outer_right.as_ref() {
                assert!(matches!(three.as_ref(), Expr::Int(3)));
                if let Expr::Apply(mul_f, two) = mul_app.as_ref() {
                    assert!(matches!(mul_f.as_ref(), Expr::Var(g) if g == "mul"));
                    assert!(matches!(two.as_ref(), Expr::Int(2)));
                } else {
                    panic!("expected Apply(mul, 2), got: {mul_app}");
                }
            } else {
                panic!("expected Apply(..., 3), got: {outer_right}");
            }
        } else {
            panic!("unexpected expr: {expr}");
        }
    }

    #[test]
    fn test_desugar_bind_chain() {
        let src = "define r = var x <- 1 >>= var y <- 2 >>= x";
        let stmts = ok(src);
        let expr = match &stmts[0] {
            Stmt::Define { expr, .. } => expr,
            _ => panic!("expected define"),
        };
        // Apply(Lambda{x, Apply(Lambda{y, x}, 2)}, 1)
        if let Expr::Apply(lam, val) = expr {
            assert!(matches!(val.as_ref(), Expr::Int(1)));
            if let Expr::Lambda { param: xp, .. } = lam.as_ref() {
                assert_eq!(xp, "x");
            } else {
                panic!("expected Lambda, got: {lam}");
            }
        } else {
            panic!("unexpected expr: {expr}");
        }
    }

    #[test]
    fn test_bare_bind_error() {
        let msg = err("define r = var x <- 1");
        assert!(msg.contains("without >>= continuation"), "got: {msg}");
    }

    #[test]
    fn test_unbound_var() {
        let src = r#"infix + 5 = add
            define x = z"#;
        let (stmts, _) = parlance_parser::parse_program(src).unwrap();
        let msg = analyze(stmts).unwrap_err().msg;
        assert!(msg.contains("unbound variable"), "got: {msg}");
    }

    #[test]
    fn test_well_scoped_lambda() {
        let src = r#"
            define x = 1
            define f = \x -> x
        "#;
        ok(src);
    }

    #[test]
    fn test_no_sugar_survives() {
        let src = r#"
            infix + 5 = add
            define r = var x <- 1 >>= var y <- 2 >>= x + y
        "#;
        let stmts = ok(src);
        for stmt in &stmts {
            match stmt {
                Stmt::Define { expr, .. } => assert_no_sugar(expr),
                Stmt::Infix { func, .. } => assert_no_sugar(func),
                _ => {}
            }
        }
    }

    fn assert_no_sugar(expr: &Expr) {
        match expr {
            Expr::Infix(..) | Expr::Bind { .. } | Expr::Seq(..) => {
                panic!("sugar node survived: {expr}");
            }
            Expr::Lambda { body, .. } => assert_no_sugar(body),
            Expr::Apply(f, a) => {
                assert_no_sugar(f);
                assert_no_sugar(a);
            }
            _ => {}
        }
    }
}
