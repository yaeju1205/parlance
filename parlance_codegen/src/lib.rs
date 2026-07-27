// ── Code generation: Parlance IR → GraftVM bytecode ────────────
//
// CODEGEN THEORY:
//
//   Code generation translates the λ-calculus IR into executable
//   GraftVM bytecode.  The translation is a HOMOMORPHISM: each IR
//   node maps to a sequence of bytecode instructions.
//
//   MAPPING:
//
//     Ir::Int(n)    →  StoreData + LoadData     (constant pool)
//     Ir::Float(n)  →  StoreData + LoadData     (constant pool)
//     Ir::Str(s)    →  StoreData + LoadData     (constant pool)
//     Ir::Var(name) →  lookup in scope, or function call
//     Ir::Lam{p,b}  →  compile as an anonymous function
//     Ir::App(f,a)  →  apply: compile f and a, then call
//
//   FUNCTION APPLICATION:
//     In the core λ-calculus, function application is the only
//     operation.  The GraftVM calling convention uses:
//
//       1. PushArg(closure)    — push the function value
//       2. PushArg(argument)   — push the argument
//       3. Enter               — create callee window
//       4. PopArg(param)       — pop argument into param slot
//       5. PopArg(closure)     — pop closure into a temp slot
//       6. Call(body_label)    — call the function body
//       7. PopArg(result)      — pop return value
//
//     The callee's body ends with:
//       PushArg(result)
//       Exit (pop runtime window)
//       Ret  (return to caller)
//
//   TOP-LEVEL DEFINITIONS:
//     Each `define name = expr` becomes a function that can be
//     called by name.  Each `infix` is skipped (already handled
//     by semant at the AST level — infix declarations only exist
//     to populate the operator table and are erased in the IR).

use std::collections::HashMap;

use graftvm_bytecode::Width;
use graftvm_ir::IrBuilder;
use parlance_ir::{Ir, IrDef};
use parlance_optimize::optimize;

/// Compile an optimized IR program into GraftVM bytecode.
///
/// Returns the bytecode (list of opcodes) and debug annotations.
/// `program_name` is used to generate a top-level function name.
pub fn compile(defs: &[IrDef], program_name: &str) -> Vec<graftvm_bytecode::Opcode> {
    let mut cg = Codegen::new();
    cg.compile_program(defs, program_name);
    cg.builder.build()
}

/// Compile with debug annotations.
pub fn compile_with_annotations(
    defs: &[IrDef],
    program_name: &str,
) -> (Vec<graftvm_bytecode::Opcode>, Vec<(String, usize)>) {
    let mut cg = Codegen::new();
    cg.compile_program(defs, program_name);
    cg.builder.build_with_annotations()
}

// ── Codegen state ────────────────────────────────────────────────

struct Codegen {
    builder: IrBuilder,
    /// Set of function names that can be called directly.
    func_names: HashMap<String, usize>,
    /// Unique counter for anonymous labels.
    anon_counter: usize,
}

impl Codegen {
    fn new() -> Self {
        Self {
            builder: IrBuilder::new(),
            func_names: HashMap::new(),
            anon_counter: 0,
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let n = self.anon_counter;
        self.anon_counter += 1;
        format!("{prefix}_{n}")
    }

    // ── Program compilation ─────────────────────────────────────

    fn compile_program(&mut self, defs: &[IrDef], _program_name: &str) {
        // First pass: register all function names.
        for def in defs {
            match def {
                IrDef::Bind { name, .. } => {
                    self.func_names.insert(name.clone(), 0);
                }
                IrDef::Infix { .. } => {} // skip — resolved by semant
            }
        }

        // Second pass: compile each definition as a callable function.
        for def in defs {
            match def {
                IrDef::Bind { name, expr } => {
                    self.compile_definition(name, expr);
                }
                IrDef::Infix { op, strength, func } => {
                    let _ = (op, strength, func);
                }
            }
        }

        // ── Entry point: call `main` if defined ─────────────────────
        if self.func_names.contains_key("main") {
            let dummy = self.builder.i64("entry.dummy", 0);
            self.builder.push_arg(&dummy);
            self.builder.enter();
            let slot = self.builder.var("entry.slot", Width::I64);
            self.builder.pop_arg(&slot);
            self.builder.call("def_main");
            let ret = self.builder.var("entry.ret", Width::I64);
            self.builder.pop_arg(&ret);
            self.builder.exit();
        }
    }

    // ── Definition compilation ──────────────────────────────────

    /// Compile a `define name = expr` as a function.
    ///
    /// If `name` is the last definition (entry point), compile it
    /// inline.  Otherwise, compile it as a callable function body.
    fn compile_definition(&mut self, name: &str, expr: &Ir) {
        let label = format!("def_{name}");

        // Skip over the function body.
        self.builder.jump(&format!("{label}_skip"));

        // Function entry label.
        self.builder.label(&label);

        // Enter function scope.
        self.builder.enter();

        // Allocate a return variable.
        let ret = self.builder.var(&format!("{name}.ret"), Width::I64);

        // Compile the expression body.
        let result = self.compile_expr(expr);

        // Move result into return var.
        self.builder.copy_var(&ret, &result);

        // Push return value, exit window, return to caller.
        self.builder.push_arg(&ret);
        self.builder.exit();
        self.builder.ret();

        // Skip label.
        self.builder.label(&format!("{label}_skip"));
    }

    // ── Expression compilation ──────────────────────────────────

    fn compile_expr(&mut self, ir: &Ir) -> graftvm_ir::Var {
        match ir {
            Ir::Int(n) => {
                let name = self.fresh_label("int");
                self.builder.i64(&name, *n)
            }

            Ir::Float(n) => {
                let name = self.fresh_label("float");
                self.builder.f64(&name, *n)
            }

            Ir::Str(s) => {
                let name = self.fresh_label("str");
                self.builder.constant(&name, graftvm_liternal::Liternal::String(s.clone()))
            }

            Ir::Var(v) => {
                // If it's a known function name, return a dummy var
                // (the function reference itself — in a full closure
                // implementation this would be a code-pointer value).
                if self.func_names.contains_key(v) {
                    let dummy = self.fresh_label("func_ref");
                    self.builder.i64(&dummy, 0)
                } else {
                    // Look up in the builder's scope.
                    match self.builder.find_var(v) {
                        Some(var) => var.clone(),
                        None => {
                            // Unknown variable — create a placeholder
                            // (would be a compile error in a full implementation).
                            self.builder.i64(&format!("undef_{v}"), 0)
                        }
                    }
                }
            }

            Ir::Lam { param, body } => {
                // Compile an anonymous function.
                // The lambda is compiled as a named function that we
                // jump over, then the lambda expression evaluates to
                // a reference (function pointer) to that label.
                let lam_label = self.fresh_label("lam");

                // Push a reference to the lambda (placeholder).
                let lam_ref = self.fresh_label("lam_ref");
                let lam_ref_var = self.builder.i64(&lam_ref, 0);

                // Define the lambda body.
                // Jump over it first.
                let skip = self.fresh_label("lam_skip");
                self.builder.jump(&skip);

                self.builder.label(&lam_label);
                self.builder.enter();

                // Pop the argument into the param slot.
                let param_var = self.builder.var(param, Width::I64);
                self.builder.pop_arg(&param_var);

                // Pop the closure reference (not used in simple model).
                let _closure_slot = self.builder.var("_closure", Width::I64);
                self.builder.pop_arg(&_closure_slot);

                // Compile body.
                let body_result = self.compile_expr(body);

                // Push result, exit, return.
                let lam_ret_label = self.fresh_label("lam_ret");
                let ret = self.builder.var(&lam_ret_label, Width::I64);
                self.builder.copy_var(&ret, &body_result);
                self.builder.push_arg(&ret);
                self.builder.exit();
                self.builder.ret();

                self.builder.label(&skip);

                lam_ref_var
            }

            Ir::App(f, a) => self.compile_apply(f, a),
        }
    }

    // ── Application compilation ─────────────────────────────────

    /// Compile a function application `(f a)`.
    ///
    /// If `f` is a known function (Var pointing to a definition),
    /// use direct Call.  Otherwise, compile as a general application
    /// using the lambda calling convention.
    fn compile_apply(&mut self, f: &Ir, a: &Ir) -> graftvm_ir::Var {
        // Check if `f` is a direct reference to a named function.
        if let Ir::Var(func_name) = f {
            if self.func_names.contains_key(func_name) {
                return self.compile_named_call(func_name, a);
            }
        }

        // ── Binary built-in: detect ((add|sub|mul|div) lhs) rhs ──
        // In Parlance, `1 + 2` desugars to App(App(Var("add"), 1), 2).
        // This pattern catches the outer App and emits a real arithmetic opcode.
        if let Ir::App(f_inner, lhs) = f {
            if let Ir::Var(ref func_name) = f_inner.as_ref() {
                if self.func_names.contains_key(func_name) {
                    match func_name.as_str() {
                        "add" | "sub" | "mul" | "div" => {
                            let lhs_var = self.compile_expr(lhs);
                            let rhs_var = self.compile_expr(a);
                            let result = self.fresh_label("binop");
                            let result_var = self.builder.var(&result, Width::I64);
                            match func_name.as_str() {
                                "add" => self.builder.add(&result_var, &lhs_var, &rhs_var),
                                "sub" => self.builder.sub(&result_var, &lhs_var, &rhs_var),
                                "mul" => self.builder.mul(&result_var, &lhs_var, &rhs_var),
                                "div" => self.builder.div(&result_var, &lhs_var, &rhs_var),
                                _ => unreachable!(),
                            }
                            return result_var;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Inline application: evaluate argument, then evaluate body.
        // This handles App(Lam{...}, arg) directly.
        if let Ir::Lam { param, body } = f {
            let arg_var = self.compile_expr(a);
            let result = self.fresh_label("app");
            let result_var = self.builder.var(&result, Width::I64);

            // Push the argument onto the arg stack.
            self.builder.push_arg(&arg_var);

            // Enter callee scope.
            self.builder.enter();

            // Pop argument into the param slot.
            let param_var = self.builder.var(param, Width::I64);
            self.builder.pop_arg(&param_var);

            // Compile body in the callee's scope.
            let body_result = self.compile_expr(body);

            // Push result, exit, pop return.
            let app_ret_label = self.fresh_label("app_ret");
            let ret_slot = self.builder.var(&app_ret_label, Width::I64);
            self.builder.copy_var(&ret_slot, &body_result);
            self.builder.push_arg(&ret_slot);
            self.builder.exit();

            // Pop return value into result var.
            self.builder.pop_arg(&result_var);

            // Note: because we used enter/exit manually rather than call/ret,
            // the scoping is correct.  The callee's body DOES NOT emit Exit+Ret
            // — only compile_expr fires.  The Exit here balances the Enter.
            // This is correct because Lam compilation with PopArg/PushArg/Exit/Ret
            // is only used when the lambda is called via the general path (PushArg + Call).
            // For direct inlining, we do Enter → PopArg → body → PushArg → Exit.

            return result_var;
        }

        // General application: compile both sides, call directly.
        // This is a fallback that pushes both values as args,
        // enters a temp window, then calls a "call" function.
        let f_var = self.compile_expr(f);
        let a_var = self.compile_expr(a);

        let result = self.fresh_label("app");
        let result_var = self.builder.var(&result, Width::I64);

        // Use the generic calling convention:
        // PushArg(f), PushArg(arg), Enter, PopArg(arg), PopArg(f), Call, PopArg(result)
        self.builder.push_arg(&f_var);
        self.builder.push_arg(&a_var);
        self.builder.enter();

        let arg_label = self.fresh_label("arg");
        let func_label = self.fresh_label("func");
        let temp_arg = self.builder.var(&arg_label, Width::I64);
        let temp_func = self.builder.var(&func_label, Width::I64);
        self.builder.pop_arg(&temp_arg);
        self.builder.pop_arg(&temp_func);
        // In a full implementation, temp_func would be a code pointer
        // and we'd use an indirect call.  For now, just return a dummy.
        self.builder.copy_var(&result_var, &temp_arg);
        self.builder.push_arg(&result_var);
        self.builder.exit();
        self.builder.pop_arg(&result_var);

        result_var
    }

    /// Compile a call to a named function.
    ///
    /// Special-cases built-in functions (e.g. `print`) to emit native
    /// opcodes instead of generating a function call.
    fn compile_named_call(&mut self, func_name: &str, arg: &Ir) -> graftvm_ir::Var {
        // ── print: built-in syscall (n=0: write to stdout) ──────
        if func_name == "print" {
            let arg_var = self.compile_expr(arg);
            let result = self.fresh_label("print_ret");
            let result_var = self.builder.var(&result, Width::I64);
            self.builder.syscall(0, &arg_var, &result_var);
            return result_var;
        }

        let arg_var = self.compile_expr(arg);
        let result = self.fresh_label("call");
        let result_var = self.builder.var(&result, Width::I64);

        // Push argument.
        self.builder.push_arg(&arg_var);

        // Enter callee window.
        // The function body will PopArg into its param.
        self.builder.enter();

        // Pop argument into callee's slot 0.
        let callee_arg = self.builder.var(&format!("{func_name}.arg"), Width::I64);
        self.builder.pop_arg(&callee_arg);

        // Call the function by its label.
        let label = format!("def_{func_name}");
        self.builder.call(&label);

        // Pop return value.
        self.builder.pop_arg(&result_var);

        result_var
    }
}

// ── High-level entry point ───────────────────────────────────────

/// Full compile pipeline: optimize IR then generate bytecode.
pub fn compile_program(defs: &[IrDef], program_name: &str) -> Vec<graftvm_bytecode::Opcode> {
    let optimized = optimize(defs);
    compile(&optimized, program_name)
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
    fn float(n: f64) -> Ir {
        Ir::Float(n)
    }

    #[test]
    fn smoke_int() {
        let defs = vec![IrDef::Bind {
            name: "x".into(),
            expr: int(42),
        }];
        let bc = compile(&defs, "test");
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_float() {
        let defs = vec![IrDef::Bind {
            name: "pi".into(),
            expr: float(3.14),
        }];
        let bc = compile(&defs, "test");
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_lambda_identity() {
        // define id = \x => x
        let defs = vec![IrDef::Bind {
            name: "id".into(),
            expr: lam("x", var("x")),
        }];
        let bc = compile(&defs, "test");
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_apply_lambda() {
        // define result = (\x => x) 42
        let defs = vec![IrDef::Bind {
            name: "result".into(),
            expr: app(lam("x", var("x")), int(42)),
        }];
        let bc = compile(&defs, "test");
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_named_call() {
        // define f = \x => x
        // define result = f 42
        let defs = vec![
            IrDef::Bind {
                name: "f".into(),
                expr: lam("x", var("x")),
            },
            IrDef::Bind {
                name: "result".into(),
                expr: app(var("f"), int(42)),
            },
        ];
        let bc = compile(&defs, "test");
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_compile_optimized() {
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
        let bc = compile_program(&defs, "test");
        assert!(!bc.is_empty());
        // After optimizations, there should be fewer ops (inlined).
        let unoptimized = compile(&defs, "test");
        assert!(bc.len() <= unoptimized.len());
    }

    #[test]
    fn smoke_infix_skipped() {
        let defs = vec![
            IrDef::Infix {
                op: "+".into(),
                strength: 5,
                func: lam("x", lam("y", var("x"))),
            },
            IrDef::Bind {
                name: "x".into(),
                expr: int(42),
            },
        ];
        let bc = compile(&defs, "test");
        // Only the definition compiles; infix generates nothing.
        assert!(!bc.is_empty());
    }

    #[test]
    fn smoke_irbuilder_compat() {
        // Test that the generated bytecode can be built by IrBuilder.
        let (bc, _annotations) = compile_with_annotations(
            &[IrDef::Bind {
                name: "x".into(),
                expr: int(99),
            }],
            "test",
        );
        assert!(!bc.is_empty());
    }
}
