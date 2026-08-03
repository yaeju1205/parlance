// ── Optimisation passes (Ir → Ir) ───────────────────────────────
//
// OPTIMISATION THEORY:
//
//   Optimisation passes transform the IR into semantically equivalent
//   but simpler/faster IR.  Each pass preserves the denotational
//   meaning (observable behaviour) of the program.
//
//   PASSES IMPLEMENTED:
//
//     η-reduction  (η-conversion)
//       λx. (f x)  →  f       (when x ∉ FV(f))
//       Eliminates eta-expanded wrappers.  Based on the EXTENSIONALITY
//       principle: two functions are equal if they produce equal
//       results for all arguments.  Restricted to PURE function
//       expressions `f` (a variable, lambda, or literal): η-reduction
//       moves `f`'s evaluation from application time to value-creation
//       time, which is only observationally invisible for pure `f`.
//
//     Full β-reduction (STRICTNESS-AWARE)
//       (λx. body) arg  →  body[x ↦ arg]
//       Always applied when the argument is a literal, or when `x`
//       occurs exactly once in body.  The language is evaluated
//       STRICTLY (arguments are evaluated before the call), so a
//       redex whose argument is effectful (a native call such as
//       `print`) and whose parameter is used zero or ≥2 times is
//       KEPT: erasing it would lose the side effect, and duplicating
//       it would repeat the side effect.
//
//     Inlining
//       Replace Var("name") with the body of `define name = ...`
//       when the body is "small" (a single literal or variable).
//       This is a form of PROCEDURE INTEGRATION.
//
//     Dead definition elimination (DCE)
//       Remove definitions (`define name = ...`) whose name is never
//       referenced anywhere in the program.  Based on REACHABILITY
//       analysis: a definition is live iff its name appears in at
//       least one other definition's expression.
//
//   PIPELINE:
//     The top-level `optimize()` function runs all passes until
//     fixpoint (the IR stops changing).  Pass order matters:
//
//       1. Inline small definitions       (creates new β-redexes)
//       2. β-reduce applied functions     (simplifies the inlined code)
//       3. β-reduce remaining redexes     (eliminates η-redexes)
//       4. η-reduce lambda wrappers       (removes unused wrappers)
//       5. Remove dead definitions        (cleans up inlined-away defs)

use std::collections::{HashMap, HashSet};

use parlance_ir::{Ir, IrDef};

// ── Pass 1: η-reduction ──────────────────────────────────────────

fn eta_reduce(ir: &Ir) -> Ir {
    match ir {
        Ir::Lam { param, body } => {
            let body = eta_reduce(body);
            if let Ir::App(f, a) = &body {
                if let Ir::Var(v) = a.as_ref() {
                    if v == param && !free_in(f, param) && is_pure_ref(f) {
                        return eta_reduce(f);
                    }
                }
            }
            Ir::Lam {
                param: param.clone(),
                body: Box::new(body),
            }
        }
        Ir::App(f, a) => Ir::App(Box::new(eta_reduce(f)), Box::new(eta_reduce(a))),
        other => other.clone(),
    }
}

/// True when evaluating `ir` has no observable effects and no cost
/// worth preserving: a variable reference, a lambda, or a literal.
/// η-reduction is restricted to these so that `λx. f x → f` never
/// moves an effectful computation from application time to
/// value-creation time.
fn is_pure_ref(ir: &Ir) -> bool {
    matches!(
        ir,
        Ir::Var(_) | Ir::Lam { .. } | Ir::Int(_) | Ir::Float(_) | Ir::Str(_)
    )
}

fn free_in(ir: &Ir, name: &str) -> bool {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) => false,
        Ir::Var(v) => v == name,
        Ir::Lam { param, body } => {
            if param == name {
                false
            } else {
                free_in(body, name)
            }
        }
        Ir::App(f, a) => free_in(f, name) || free_in(a, name),
    }
}

// ── Pass 2: Strictness-aware β-reduction ─────────────────────────

/// Count how many times `name` occurs FREE in `ir`, respecting
/// binder shadowing (`λname. …` stops the count).
fn count_uses(ir: &Ir, name: &str) -> usize {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) => 0,
        Ir::Var(v) => usize::from(v == name),
        Ir::Lam { param, body } => {
            if param == name {
                0
            } else {
                count_uses(body, name)
            }
        }
        Ir::App(f, a) => count_uses(f, name) + count_uses(a, name),
    }
}

fn beta_reduce(ir: &Ir) -> Ir {
    match ir {
        Ir::Lam { param, body } => Ir::Lam {
            param: param.clone(),
            body: Box::new(beta_reduce(body)),
        },
        Ir::App(f, a) => {
            let f = beta_reduce(f);
            let a = beta_reduce(a);
            if let Ir::Lam { param, body } = &f {
                // Strictness-aware β-reduction:
                //   (λx. body) arg → body[x ↦ arg]
                // only when it cannot change the evaluation of `arg`:
                //   - arg is a pure literal, OR
                //   - x occurs exactly once in body (the argument is
                //     neither erased nor duplicated).
                // Erasing an effectful argument (e.g. `print 1` bound
                // to an unused variable) or duplicating it would
                // change observable behaviour under strict evaluation.
                let uses = count_uses(body, param);
                let arg_pure = matches!(a, Ir::Int(_) | Ir::Float(_) | Ir::Str(_));
                if arg_pure || uses == 1 {
                    let reduced = parlance_ir::subst(body, param, &a);
                    return beta_reduce(&reduced);
                }
            }
            Ir::App(Box::new(f), Box::new(a))
        }
        other => other.clone(),
    }
}

// ── Pass 3: Inlining ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum InlineHeuristic {
    Small,
    Keep,
}

fn inline_class(ir: &Ir) -> InlineHeuristic {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) | Ir::Var(_) => InlineHeuristic::Small,
        Ir::Lam { body, .. } => inline_class(body),
        Ir::App(f, a) => {
            if matches!(f.as_ref(), Ir::Var(_))
                && matches!(a.as_ref(), Ir::Int(_) | Ir::Float(_) | Ir::Str(_))
            {
                InlineHeuristic::Small
            } else {
                InlineHeuristic::Keep
            }
        }
    }
}

fn inline_defs(defs: &[IrDef]) -> Vec<IrDef> {
    if defs.is_empty() {
        return vec![];
    }

    let last_idx = defs.len() - 1;

    let inline_map: Vec<(String, Ir)> = defs
        .iter()
        .enumerate()
        .filter_map(|(i, def)| match def {
            // Don't inline away the last definition (entry point) or built-ins.
            // Native definitions have no body and are never inlined.
            IrDef::Bind { name, expr }
                if i != last_idx && inline_class(expr) == InlineHeuristic::Small =>
            {
                Some((name.clone(), expr.clone()))
            }
            _ => None,
        })
        .collect();

    if inline_map.is_empty() {
        return defs.to_vec();
    }

    let map: HashMap<String, Ir> = inline_map.into_iter().collect();

    // Resolve transitivity: for each replacement value, substitute any
    // names that are themselves being inlined.
    let resolved: HashMap<String, Ir> = map
        .iter()
        .map(|(k, v)| (k.clone(), substitute_vars(v, &map)))
        .collect();
    let mut out: Vec<IrDef> = Vec::new();
    let inlined_names: HashSet<String> = resolved.keys().cloned().collect();

    for def in defs {
        match def {
            IrDef::Bind { name, expr } if inlined_names.contains(name) => {
                // Skip — inlined away.
            }
            IrDef::Bind { name, expr } => {
                out.push(IrDef::Bind {
                    name: name.clone(),
                    expr: substitute_vars(expr, &resolved),
                });
            }
            IrDef::Native { .. } => {
                // Native definitions have no body — pass through unchanged.
                out.push(def.clone());
            }
            IrDef::Infix { op, strength, func } => {
                out.push(IrDef::Infix {
                    op: op.clone(),
                    strength: *strength,
                    func: substitute_vars(func, &resolved),
                });
            }
        }
    }

    out
}

fn substitute_vars(ir: &Ir, map: &HashMap<String, Ir>) -> Ir {
    match ir {
        Ir::Int(_) | Ir::Float(_) | Ir::Str(_) => ir.clone(),
        Ir::Var(v) => map.get(v).cloned().unwrap_or_else(|| ir.clone()),
        Ir::Lam { param, body } => Ir::Lam {
            param: param.clone(),
            body: Box::new(substitute_vars(body, map)),
        },
        Ir::App(f, a) => Ir::App(
            Box::new(substitute_vars(f, map)),
            Box::new(substitute_vars(a, map)),
        ),
    }
}

// ── Pass 4: Dead definition elimination ──────────────────────────

fn remove_dead(defs: &[IrDef]) -> Vec<IrDef> {
    if defs.is_empty() {
        return vec![];
    }

    let mut referenced: HashSet<String> = HashSet::new();
    for def in defs {
        match def {
            IrDef::Bind { expr, .. } => collect_refs(expr, &mut referenced),
            IrDef::Infix { func, .. } => collect_refs(func, &mut referenced),
            IrDef::Native { .. } => {} // native names are always live
        }
    }

    let last_idx = defs.len() - 1;
    defs.iter()
        .enumerate()
        .filter(|(i, def)| {
            if *i == last_idx {
                return true;
            }
            let name = match def {
                IrDef::Bind { name, .. } => name,
                IrDef::Infix { op, .. } => op,
                // Native definitions are always kept (never treated as dead).
                IrDef::Native { .. } => return true,
            };
            referenced.contains(name.as_str())
        })
        .map(|(_, def)| def.clone())
        .collect()
}

fn collect_refs(ir: &Ir, out: &mut HashSet<String>) {
    match ir {
        Ir::Var(v) => {
            out.insert(v.clone());
        }
        Ir::Lam { body, .. } => collect_refs(body, out),
        Ir::App(f, a) => {
            collect_refs(f, out);
            collect_refs(a, out);
        }
        _ => {}
    }
}

// ── Pipeline ─────────────────────────────────────────────────────

pub fn optimize(defs: &[IrDef]) -> Vec<IrDef> {
    let mut current = defs.to_vec();
    loop {
        let prev = current.clone();

        current = inline_defs(&current);

        current = current
            .into_iter()
            .map(|def| match def {
                IrDef::Bind { name, expr } => IrDef::Bind {
                    name,
                    expr: beta_reduce(&expr),
                },
                IrDef::Infix { op, strength, func } => IrDef::Infix {
                    op,
                    strength,
                    func: beta_reduce(&func),
                },
                IrDef::Native { .. } => def,
            })
            .collect();

        current = current
            .into_iter()
            .map(|def| match def {
                IrDef::Bind { name, expr } => IrDef::Bind {
                    name,
                    expr: eta_reduce(&expr),
                },
                IrDef::Infix { op, strength, func } => IrDef::Infix {
                    op,
                    strength,
                    func: eta_reduce(&func),
                },
                IrDef::Native { .. } => def,
            })
            .collect();

        current = remove_dead(&current);

        if current == prev {
            break;
        }
    }
    current
}

pub fn optimize_expr(ir: &Ir) -> Ir {
    let defs = vec![IrDef::Bind {
        name: "$tmp".into(),
        expr: ir.clone(),
    }];
    let out = optimize(&defs);
    match &out[0] {
        IrDef::Bind { expr, .. } => expr.clone(),
        _ => ir.clone(),
    }
}

// ── Self-test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_ir::Ir;

    fn lam(param: &str, body: Ir) -> Ir {
        Ir::Lam {
            param: param.into(),
            body: Box::new(body),
        }
    }

    fn app(f: Ir, a: Ir) -> Ir {
        Ir::App(Box::new(f), Box::new(a))
    }

    fn var(name: &str) -> Ir {
        Ir::Var(name.into())
    }
    fn int(n: i64) -> Ir {
        Ir::Int(n)
    }

    // ── η-reduction tests ─────────────────────────────────────

    #[test]
    fn test_eta_reduce_simple() {
        assert_eq!(eta_reduce(&lam("x", app(var("f"), var("x")))), var("f"));
    }

    #[test]
    fn test_eta_no_reduce_when_x_in_f() {
        let ir = lam("x", app(var("x"), var("x")));
        assert_eq!(eta_reduce(&ir), ir);
    }

    #[test]
    fn test_eta_no_reduce_non_app() {
        let ir = lam("x", var("x"));
        assert_eq!(eta_reduce(&ir), ir);
    }

    #[test]
    fn test_eta_nested() {
        // λx. (λy. (f y) x)  →  λx. (f x)  →  f
        let ir = lam("x", app(lam("y", app(var("f"), var("y"))), var("x")));
        assert_eq!(eta_reduce(&ir), var("f"));
    }

    // ── β-reduction tests ─────────────────────────────────────

    #[test]
    fn test_beta_reduce_simple() {
        assert_eq!(beta_reduce(&app(lam("x", var("x")), int(42))), int(42));
    }

    #[test]
    fn test_beta_reduce_non_literal() {
        assert_eq!(
            beta_reduce(&app(lam("x", var("x")), app(var("f"), var("y")))),
            app(var("f"), var("y"))
        );
    }

    #[test]
    fn test_beta_nested_fully() {
        assert_eq!(
            beta_reduce(&app(app(lam("x", lam("y", var("x"))), int(1)), int(2))),
            int(1)
        );
    }

    #[test]
    fn test_beta_shadows() {
        assert_eq!(
            beta_reduce(&app(lam("x", lam("x", var("x"))), int(1))),
            lam("x", var("x"))
        );
    }

    // ── Strictness-aware β-reduction tests ─────────────────────

    #[test]
    fn test_beta_keeps_effectful_unused_arg() {
        // (λp. print 42) (print 1) — `p` unused, argument is a native
        // call: β-reduction must NOT erase `print 1` (strict
        // evaluation evaluates the argument before the call).
        let ir = app(
            lam("p", app(var("print"), int(42))),
            app(var("print"), int(1)),
        );
        assert_eq!(beta_reduce(&ir), ir);
    }

    #[test]
    fn test_beta_reduces_effectful_single_use() {
        // (λx. print x) (print 1) — x used exactly once: reducing
        // keeps exactly one evaluation of the argument.
        let ir = app(
            lam("x", app(var("print"), var("x"))),
            app(var("print"), int(1)),
        );
        assert_eq!(
            beta_reduce(&ir),
            app(var("print"), app(var("print"), int(1)))
        );
    }

    #[test]
    fn test_beta_keeps_effectful_duplicated_arg() {
        // (λx. x x) (print 1) — x used twice: reduction would run the
        // effectful argument twice, so the redex is kept.
        let ir = app(
            lam("x", app(var("x"), var("x"))),
            app(var("print"), int(1)),
        );
        assert_eq!(beta_reduce(&ir), ir);
    }

    #[test]
    fn test_beta_still_folds_literal_duplication() {
        // (λx. x x) 42 — literals are pure, duplication is free.
        let ir = app(lam("x", app(var("x"), var("x"))), int(42));
        assert_eq!(beta_reduce(&ir), app(int(42), int(42)));
    }

    #[test]
    fn test_eta_keeps_effectful_function() {
        // λx. (print 1) x — η-reduction would move `print 1` from
        // application time to value-creation time, so it is kept.
        let ir = lam(
            "x",
            app(app(var("print"), int(1)), var("x")),
        );
        assert_eq!(eta_reduce(&ir), ir);
    }

    #[test]
    fn test_eta_reduces_pure_named_function() {
        // λx. f x → f, f a plain variable reference (pure).
        assert_eq!(eta_reduce(&lam("x", app(var("f"), var("x")))), var("f"));
    }

    // ── Free variables ────────────────────────────────────────

    #[test]
    fn test_free_in_var() {
        assert!(free_in(&var("x"), "x"));
        assert!(!free_in(&var("x"), "y"));
    }

    #[test]
    fn test_free_in_lam_bound() {
        assert!(!free_in(&lam("x", var("x")), "x"));
    }

    #[test]
    fn test_free_in_lam_free() {
        assert!(free_in(&lam("x", var("y")), "y"));
    }

    // ── Inlining tests ────────────────────────────────────────

    #[test]
    fn test_inline_small_literal() {
        let defs = vec![
            IrDef::Bind {
                name: "x".into(),
                expr: int(42),
            },
            IrDef::Bind {
                name: "y".into(),
                expr: var("x"),
            },
        ];
        let result = inline_defs(&defs);
        assert_eq!(result.len(), 1);
        match &result[0] {
            IrDef::Bind { name, expr } => {
                assert_eq!(name, "y");
                assert_eq!(*expr, int(42));
            }
            _ => panic!("expected Bind"),
        }
    }

    #[test]
    fn test_inline_keeps_complex() {
        let defs = vec![IrDef::Bind {
            name: "f".into(),
            expr: lam("x", app(var("g"), var("x"))),
        }];
        assert_eq!(inline_defs(&defs).len(), 1);
    }

    // ── Dead-code elimination tests ───────────────────────────

    #[test]
    fn test_remove_dead_keeps_referenced() {
        let defs = vec![
            IrDef::Bind {
                name: "x".into(),
                expr: int(1),
            },
            IrDef::Bind {
                name: "y".into(),
                expr: var("x"),
            },
        ];
        assert_eq!(remove_dead(&defs).len(), 2);
    }

    #[test]
    fn test_remove_dead_removes_unused() {
        let defs = vec![
            IrDef::Bind {
                name: "x".into(),
                expr: int(1),
            },
            IrDef::Bind {
                name: "y".into(),
                expr: var("z"),
            },
        ];
        let result = remove_dead(&defs);
        assert_eq!(result.len(), 1);
        match &result[0] {
            IrDef::Bind { name, .. } => assert_eq!(name, "y"),
            _ => panic!("expected y"),
        }
    }

    #[test]
    fn test_remove_dead_infix() {
        let defs = vec![
            IrDef::Infix {
                op: "+".into(),
                strength: 5,
                func: var("add"),
            },
            IrDef::Bind {
                name: "add".into(),
                expr: lam("x", lam("y", var("x"))),
            },
        ];
        // `+` is unreferenced → removed. `add` is last → kept.
        assert_eq!(remove_dead(&defs).len(), 1);
        match &remove_dead(&defs)[0] {
            IrDef::Bind { name, .. } => assert_eq!(name, "add"),
            _ => panic!("expected Bind"),
        }
    }

    // ── Full pipeline tests ───────────────────────────────────

    #[test]
    fn test_optimize_inline_beta_eta() {
        let defs = vec![
            IrDef::Bind {
                name: "f".into(),
                expr: lam("x", var("x")),
            },
            IrDef::Bind {
                name: "id".into(),
                expr: var("f"),
            },
            IrDef::Bind {
                name: "result".into(),
                expr: app(var("id"), int(42)),
            },
        ];
        let result = optimize(&defs);
        assert_eq!(result.len(), 1);
        match &result[0] {
            IrDef::Bind { name, expr } => {
                assert_eq!(name, "result");
                assert_eq!(*expr, int(42));
            }
            _ => panic!("expected Bind(result, 42)"),
        }
    }

    #[test]
    fn test_optimize_eta_then_beta() {
        let defs = vec![
            IrDef::Bind {
                name: "f".into(),
                expr: lam("x", app(var("g"), var("x"))),
            },
            IrDef::Bind {
                name: "result".into(),
                expr: app(var("f"), int(42)),
            },
        ];
        let result = optimize(&defs);
        assert_eq!(result.len(), 1);
        match &result[0] {
            IrDef::Bind { expr, .. } => {
                assert_eq!(*expr, app(var("g"), int(42)));
            }
            _ => panic!("expected Bind"),
        }
    }

    #[test]
    fn test_optimize_dead_code() {
        let defs = vec![
            IrDef::Bind {
                name: "unused".into(),
                expr: int(99),
            },
            IrDef::Bind {
                name: "live".into(),
                expr: int(42),
            },
        ];
        let result = optimize(&defs);
        assert_eq!(result.len(), 1);
        match &result[0] {
            IrDef::Bind { name, expr } => {
                assert_eq!(name, "live");
                assert_eq!(*expr, int(42));
            }
            _ => panic!("expected Bind(live, 42)"),
        }
    }

    #[test]
    fn test_optimize_expr() {
        assert_eq!(optimize_expr(&app(lam("x", var("x")), int(42))), int(42));
    }

    // ── Collect refs test ────────────────────────────────────

    #[test]
    fn test_collect_refs() {
        let ir = app(var("f"), app(var("g"), var("x")));
        let mut refs = HashSet::new();
        collect_refs(&ir, &mut refs);
        let mut sorted: Vec<_> = refs.into_iter().collect();
        sorted.sort();
        assert_eq!(sorted, vec!["f", "g", "x"]);
    }

    // ── Inline heuristic tests ───────────────────────────────

    #[test]
    fn test_inline_class_small() {
        assert_eq!(inline_class(&int(42)), InlineHeuristic::Small);
        assert_eq!(inline_class(&var("x")), InlineHeuristic::Small);
        assert_eq!(inline_class(&lam("x", var("x"))), InlineHeuristic::Small);
        assert_eq!(inline_class(&app(var("f"), int(1))), InlineHeuristic::Small);
    }

    #[test]
    fn test_inline_class_keep() {
        assert_eq!(
            inline_class(&app(var("f"), lam("x", var("x")))),
            InlineHeuristic::Keep
        );
        assert_eq!(
            inline_class(&app(var("f"), app(var("g"), int(1)))),
            InlineHeuristic::Keep
        );
    }
}
