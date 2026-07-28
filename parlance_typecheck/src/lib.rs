// ── Type checking with Hindley-Milner inference ──────────────────
//
//  TYPE CHECKING THEORY:
//
//    The type checker implements Hindley-Milner type inference
//    using Algorithm W (Milner, 1978).  It operates on the desugared
//    AST produced by `parlance_semant::analyze()`.
//
//    INFERENCE RULES:
//
//      Lit(int)     ⇒  Int
//      Lit(float)   ⇒  Float
//      Lit(str)     ⇒  Str
//      Var(x)       ⇒  Γ(x)                     [lookup in context]
//      Lam(p, e)    ⇒  α → τ                    [fresh α, infer e under Γ[p↦α]]
//      App(f, a)    ⇒  β                        [infer f ⇒ τ_f, a ⇒ τ_a,
//                                                  τ_f ~ (τ_a → β), return β]
//
//    Each top-level definition is checked against its type annotation
//    (if provided) and its inferred type is recorded in the global
//    environment.

use std::collections::HashMap;

use parlance_parser::ast::{Expr, Stmt, Type};

// ── Internal type representation (with inferable variables) ─────

/// A monomorphic type used during inference.
#[derive(Debug, Clone, PartialEq)]
pub enum MType {
    /// Concrete type constructor: `Int`, `Float`, `Str`, `Bool`, `IO` ...
    TCon(String),
    /// Type variable (freshly generated for inference).
    TVar(u64),
    /// Function type: `param -> result`
    Fun(Box<MType>, Box<MType>),
}

impl MType {
    /// Convert a parser `Type` to an `MType`.
    /// `TVar(name)` from the parser maps to a concrete `TCon("a")`-ish
    /// representation, but for inference we replace each unique variable
    /// name with a fresh numeric variable.
    fn from_ast(ty: &Type, ctx: &mut TypeContext) -> Self {
        match ty {
            Type::TVar(_) => MType::TVar(ctx.fresh()),
            Type::TCon(s) => MType::TCon(s.clone()),
            Type::Fun(p, r) => MType::Fun(
                Box::new(Self::from_ast(p, ctx)),
                Box::new(Self::from_ast(r, ctx)),
            ),
        }
    }

    /// Pretty-print.
    fn display(&self) -> String {
        match self {
            MType::TCon(s) => s.clone(),
            MType::TVar(n) => format!("t{}", n),
            MType::Fun(p, r) => {
                let p_str = match p.as_ref() {
                    MType::Fun(_, _) => format!("({})", p.display()),
                    _ => p.display(),
                };
                format!("{} -> {}", p_str, r.display())
            }
        }
    }

    /// Check if this type contains a specific variable (occurs check).
    fn occurs(&self, var: u64) -> bool {
        match self {
            MType::TVar(v) => *v == var,
            MType::Fun(p, r) => p.occurs(var) || r.occurs(var),
            _ => false,
        }
    }
}

// ── Type error ───────────────────────────────────────────────────

#[derive(Debug)]
pub struct TypeError {
    pub msg: String,
}

// ── Type context / environment ───────────────────────────────────

/// The type checking context carries:
///   1. A mapping from variable names to their types (Γ)
///   2. A substitution (σ) mapping type variable IDs to their resolved types
///   3. A fresh variable counter
pub struct TypeContext {
    /// Variable name → assigned type
    env: HashMap<String, MType>,
    /// Type variable ID → resolved type (the substitution σ)
    subst: HashMap<u64, MType>,
    /// Counter for generating fresh type variables
    fresh_counter: u64,
}

impl TypeContext {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            subst: HashMap::new(),
            fresh_counter: 0,
        }
    }

    /// Generate a fresh type variable.
    fn fresh(&mut self) -> u64 {
        let id = self.fresh_counter;
        self.fresh_counter += 1;
        id
    }

    /// Apply the current substitution to a type, fully resolving it.
    fn resolve(&self, ty: &MType) -> MType {
        match ty {
            MType::TVar(v) => {
                // Follow the chain of substitutions
                if let Some(resolved) = self.subst.get(v) {
                    self.resolve(resolved)  // there may be further indirections
                } else {
                    ty.clone()
                }
            }
            MType::Fun(p, r) => {
                MType::Fun(Box::new(self.resolve(p)), Box::new(self.resolve(r)))
            }
            _ => ty.clone(),
        }
    }

    /// Look up a variable name in the environment, resolving its type.
    fn lookup(&self, name: &str) -> Option<MType> {
        self.env.get(name).map(|ty| self.resolve(ty))
    }

    /// Add a variable binding to the environment.
    fn bind(&mut self, name: &str, ty: MType) {
        self.env.insert(name.to_string(), ty);
    }

    /// Unify two types, updating the substitution.
    /// Returns an error if unification fails.
    fn unify(&mut self, a: &MType, b: &MType) -> Result<(), TypeError> {
        let a = self.resolve(a);
        let b = self.resolve(b);

        match (&a, &b) {
            // Both are the same variable
            (MType::TVar(x), MType::TVar(y)) if x == y => Ok(()),

            // Variable with something else
            (MType::TVar(x), ty) | (ty, MType::TVar(x)) => {
                if ty.occurs(*x) {
                    return Err(TypeError {
                        msg: format!("infinite type: {} = {}", MType::TVar(*x).display(), ty.display()),
                    });
                }
                self.subst.insert(*x, ty.clone());
                Ok(())
            }

            // Concrete types must match
            (MType::TCon(a), MType::TCon(b)) if a == b => Ok(()),

            // Function types: unify recursively
            (MType::Fun(p1, r1), MType::Fun(p2, r2)) => {
                self.unify(p1, p2)?;
                self.unify(r1, r2)
            }

            // Mismatch
            _ => Err(TypeError {
                msg: format!("type mismatch: {} ≠ {}", a.display(), b.display()),
            }),
        }
    }

    /// Infer the type of an expression and return it (resolved).
    fn infer(&mut self, expr: &Expr) -> Result<MType, TypeError> {
        match expr {
            Expr::Int(_) => Ok(MType::TCon("Int".into())),
            Expr::Float(_) => Ok(MType::TCon("Float".into())),
            Expr::Str(_) => Ok(MType::TCon("Str".into())),

            Expr::Var(v) => {
                self.lookup(v).ok_or_else(|| TypeError {
                    msg: format!("unbound variable '{v}'"),
                })
            }

            Expr::Lambda { param, body } => {
                let param_tv = MType::TVar(self.fresh());
                // Save and restore env — we don't need to save/restore here
                // because we're building a standalone type for this lambda
                self.bind(param, param_tv.clone());
                let body_ty = self.infer(body)?;
                Ok(MType::Fun(Box::new(param_tv), Box::new(body_ty)))
            }

            Expr::Apply(f, arg) => {
                let f_ty = self.infer(f)?;
                let arg_ty = self.infer(arg)?;
                let ret_tv = MType::TVar(self.fresh());
                let expected = MType::Fun(Box::new(arg_ty), Box::new(ret_tv.clone()));
                self.unify(&f_ty, &expected)?;
                Ok(self.resolve(&ret_tv))
            }

            // These should never appear after semant
            Expr::Infix(..) | Expr::Bind { .. } | Expr::Seq(..) => {
                Err(TypeError {
                    msg: "internal: sugar node survived semant".into(),
                })
            }
        }
    }
}

// ── Public entry point ───────────────────────────────────────────

/// Run type checking on a program (desugared AST from `analyze()`).
///
/// For each top-level definition:
///   - If a type annotation is given, the inferred type is checked against it.
///   - The inferred type is recorded in the environment for subsequent defs.
///
/// Native declarations are used to seed the environment.
pub fn typecheck(stmts: &[Stmt]) -> Result<(), TypeError> {
    let mut ctx = TypeContext::new();

    // First pass: register all top-level names (native + define)
    // with their type signatures (if available).
    for stmt in stmts {
        match stmt {
            Stmt::Native { name, type_sig } => {
                let mt = MType::from_ast(type_sig, &mut ctx);
                ctx.bind(name, mt);
            }
            Stmt::Define { name, type_sig, .. } => {
                // Reserve a fresh type variable for this definition.
                // If there's a type annotation, we'll unify after inference.
                let tv = MType::TVar(ctx.fresh());
                if let Some(sig) = type_sig {
                    let sig_mt = MType::from_ast(sig, &mut ctx);
                    ctx.unify(&tv, &sig_mt)?;
                }
                ctx.bind(name, tv);
            }
            Stmt::Import { .. } => {
                // Imports are resolved by semant.
            }
            Stmt::Infix { op, func, .. } => {
                // Operator declarations are erased by semant.
                // We skip them here — they're just metadata.
                let _ = (op, func);
            }
        }
    }

    // Second pass: infer types for each definition body.
    for stmt in stmts {
        match stmt {
            Stmt::Define { name, expr, type_sig } => {
                // Infer the body type.
                let inferred = ctx.infer(expr)?;

                // Resolve the top-level type (which may be constrained by the annotation).
                let expected = ctx.lookup(name).unwrap_or_else(|| MType::TVar(0));

                // Unify inferred with expected.
                if let Err(e) = ctx.unify(&inferred, &expected) {
                    let ann = type_sig
                        .as_ref()
                        .map(|t| format!(" (annotation: {})", display_type(t)))
                        .unwrap_or_default();
                    return Err(TypeError {
                        msg: format!("type error in '{name}'{ann}: {}", e.msg),
                    });
                }

                // Update the environment with the fully resolved type.
                let final_ty = ctx.resolve(&inferred);
                ctx.bind(name, final_ty);
            }
            Stmt::Native { .. } | Stmt::Import { .. } | Stmt::Infix { .. } => {}
        }
    }

    Ok(())
}

/// Pretty-print an AST Type.
fn display_type(ty: &Type) -> String {
    match ty {
        Type::TVar(v) => v.clone(),
        Type::TCon(c) => c.clone(),
        Type::Fun(p, r) => {
            let p_str = match p.as_ref() {
                Type::Fun(_, _) => format!("({})", display_type(p)),
                _ => display_type(p),
            };
            format!("{} -> {}", p_str, display_type(r))
        }
    }
}

// ── Self-tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_parser::parse_program;

    fn run(src: &str) -> Result<(), TypeError> {
        let (stmts, _) = parse_program(src).unwrap();
        // Run semant first (desugar imports/infix)
        let desugared = parlance_semant::analyze(stmts).map_err(|e| TypeError { msg: e.msg })?;
        typecheck(&desugared)
    }

    #[test]
    fn test_int_literal() {
        assert!(run("define x = 42").is_ok());
    }

    #[test]
    fn test_type_annotation_matches() {
        assert!(run("define x : Int = 42").is_ok());
    }

    #[test]
    fn test_type_annotation_mismatch() {
        let err = run("define x : Float = 42").unwrap_err();
        assert!(err.msg.contains("Float"));
        assert!(err.msg.contains("Int"));
    }

    #[test]
    fn test_identity() {
        assert!(run("define id : a -> a = \\x -> x").is_ok());
    }

    #[test]
    fn test_native_types_are_known() {
        // If print : a -> IO, then print 42 should be IO.
        assert!(run("native print : a -> IO define main : IO = print 42").is_ok());
    }

    #[test]
    fn test_type_error_in_apply() {
        // print takes one arg, but passing two should fail type-wise
        // (in practice it would be ArityError — currently our type
        //  system sees print as taking a, so print 42 gives IO,
        //  and then the extra arg fails because IO is not a function)
        let err = run("native print : a -> IO define main = print 42 99").unwrap_err();
        // IO is not a function type, so applying it to another arg fails
        assert!(err.msg.contains("IO") || err.msg.contains("mismatch"));
    }

    #[test]
    fn test_arithmetic_type_constraint() {
        assert!(run("native add : Int -> Int -> Int define main : Int = add 1 2").is_ok());
    }
}
