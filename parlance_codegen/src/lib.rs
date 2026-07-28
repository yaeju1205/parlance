// ── Code generation: Parlance IR → GraftVM bytecode ────────────
//
// CODEGEN THEORY:
//
//   Code generation translates the λ-calculus IR into executable
//   GraftVM bytecode.  The translation is a HOMOMORPHISM: each IR
//   node maps to a sequence of bytecode instructions.
//
//     Ir::Int(n)    →  StoreData + LoadData     (constant pool)
//     Ir::Float(n)  →  StoreData + LoadData     (constant pool)
//     Ir::Str(s)    →  StoreData + LoadData     (constant pool)
//     Ir::Var(name) →  lookup in scope, or function call
//     Ir::Lam{p,b}  →  compile as an anonymous function
//     Ir::App(f,a)  →  apply: compile f and a, then call
//
//   NATIVE FUNCTIONS:
//     Functions declared with `native name : type` in Parlance
//     source are compiled to `CallNative` opcodes.  Their
//     implementations are provided by the host at VM runtime
//     via `VM::register_native()`.
//
//   ENTRY POINT:
//     The entry function name is configurable from outside.
//     Pass `Some("main")` to call main, or `None` for no entry.

use std::collections::HashMap;

use graftvm_bytecode::Width;
use graftvm_ir::IrBuilder;
use parlance_ir::{Ir, IrDef};
use parlance_optimize::optimize;

pub mod nfi;
use nfi::NativeRegistry;

/// Compile an optimized IR program into GraftVM bytecode.
/// Convenience wrapper: no entry point, empty native registry.
pub fn compile(defs: &[IrDef], _program_name: &str) -> Vec<graftvm_bytecode::Opcode> {
    let nfi = NativeRegistry::from_ir(defs);
    compile_with(defs, None, &nfi)
}

/// Full compile pipeline: optimize IR then generate bytecode.
/// Convenience wrapper: no entry point, empty native registry.
pub fn compile_program(defs: &[IrDef], _program_name: &str) -> Vec<graftvm_bytecode::Opcode> {
    let optimized = optimize(defs);
    let nfi = NativeRegistry::from_ir(&optimized);
    compile_with(&optimized, None, &nfi)
}

/// Compile with configurable entry point and native registry.
pub fn compile_with(
    defs: &[IrDef],
    entry_point: Option<&str>,
    nfi: &NativeRegistry,
) -> Vec<graftvm_bytecode::Opcode> {
    let mut cg = Codegen::new(nfi);
    cg.compile_program(defs, entry_point);
    cg.builder.build()
}

/// Compile with annotations, configurable entry and nfi.
pub fn compile_with_annotations(
    defs: &[IrDef],
    entry_point: Option<&str>,
    nfi: &NativeRegistry,
) -> (Vec<graftvm_bytecode::Opcode>, Vec<(String, usize)>) {
    let mut cg = Codegen::new(nfi);
    cg.compile_program(defs, entry_point);
    cg.builder.build_with_annotations()
}

// ── Codegen state ────────────────────────────────────────────────

struct Codegen<'a> {
    builder: IrBuilder,
    /// Set of function names that can be called directly.
    func_names: HashMap<String, usize>,
    /// Unique counter for anonymous labels.
    anon_counter: usize,
    /// Native function registry (provided from outside).
    nfi: &'a NativeRegistry,
}

impl<'a> Codegen<'a> {
    fn new(nfi: &'a NativeRegistry) -> Self {
        Codegen {
            builder: IrBuilder::new(),
            func_names: HashMap::new(),
            anon_counter: 0,
            nfi,
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let n = self.anon_counter;
        self.anon_counter += 1;
        format!("{prefix}_{n}")
    }

    // ── Program compilation ─────────────────────────────────────

    fn compile_program(&mut self, defs: &[IrDef], entry_point: Option<&str>) {
        // First pass: register all function/native names.
        for def in defs {
            match def {
                IrDef::Bind { name, .. } | IrDef::Native { name, .. } => {
                    self.func_names.insert(name.clone(), 0);
                }
                IrDef::Infix { .. } => {}
            }
        }

        // Second pass: compile non-native definitions as callable functions.
        for def in defs {
            match def {
                IrDef::Bind { name, expr } => {
                    self.compile_definition(name, expr);
                }
                IrDef::Native { .. } | IrDef::Infix { .. } => {
                    // Native functions have no body — skip.
                }
            }
        }

        // ── Entry point ─────────────────────────────────────────
        if let Some(entry) = entry_point {
            if self.func_names.contains_key(entry) {
                let label = format!("def_{entry}");
                let dummy = self.builder.i64("entry.dummy", 0);
                self.builder.push_arg(&dummy);
                self.builder.enter();
                let slot = self.builder.var("entry.slot", Width::I64);
                self.builder.pop_arg(&slot);
                self.builder.call(&label);
                let ret = self.builder.var("entry.ret", Width::I64);
                self.builder.pop_arg(&ret);
                self.builder.exit();
            }
        }
    }

    // ── Definition compilation ──────────────────────────────────

    fn compile_definition(&mut self, name: &str, expr: &Ir) {
        let label = format!("def_{name}");

        self.builder.jump(&format!("{label}_skip"));

        self.builder.label(&label);
        self.builder.enter();

        let ret = self.builder.var(&format!("{name}.ret"), Width::I64);
        let result = self.compile_expr(expr);

        self.builder.copy_var(&ret, &result);
        self.builder.push_arg(&ret);
        self.builder.exit();
        self.builder.ret();

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
                self.builder
                    .constant(&name, graftvm_liternal::Liternal::String(s.clone()))
            }

            Ir::Var(v) => {
                if self.func_names.contains_key(v) {
                    let dummy = self.fresh_label("func_ref");
                    self.builder.i64(&dummy, 0)
                } else {
                    match self.builder.find_var(v) {
                        Some(var) => var.clone(),
                        None => self.builder.i64(&format!("undef_{v}"), 0),
                    }
                }
            }

            Ir::Lam { param, body } => {
                let lam_label = self.fresh_label("lam");
                let lam_ref = self.fresh_label("lam_ref");
                let lam_ref_var = self.builder.i64(&lam_ref, 0);

                let skip = self.fresh_label("lam_skip");
                self.builder.jump(&skip);

                self.builder.label(&lam_label);
                self.builder.enter();

                let param_var = self.builder.var(param, Width::I64);
                self.builder.pop_arg(&param_var);

                let _closure_slot = self.builder.var("_closure", Width::I64);
                self.builder.pop_arg(&_closure_slot);

                let body_result = self.compile_expr(body);

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

    fn compile_apply(&mut self, f: &Ir, a: &Ir) -> graftvm_ir::Var {
        // ── Unary native call: func_name(arg) ───────────────────
        if let Ir::Var(func_name) = f {
            if self.func_names.contains_key(func_name) {
                if self.nfi.is_native(func_name) {
                    return self.compile_native_call(func_name, 1, a);
                }
                let arg_var = self.compile_expr(a);
                return self.compile_generic_call(func_name, arg_var);
            }
        }

        // ── Binary native: ((func_name lhs) rhs) ────────────────
        if let Ir::App(f_inner, lhs) = f {
            if let Ir::Var(ref func_name) = f_inner.as_ref() {
                if self.func_names.contains_key(func_name) && self.nfi.is_native(func_name) {
                    let lhs_var = self.compile_expr(lhs);
                    let rhs_var = self.compile_expr(a);
                    let result = self.fresh_label("native");
                    let result_var = self.builder.var(&result, Width::I64);

                    // Push args in order, then CallNative pops them in reverse.
                    self.builder.push_arg(&lhs_var);
                    self.builder.push_arg(&rhs_var);
                    self.builder.call_native(func_name, 2);

                    // The result is on the arg stack — pop it.
                    self.builder.pop_arg(&result_var);
                    return result_var;
                }
            }
        }

        // ── Inline lambda application ───────────────────────────
        if let Ir::Lam { param, body } = f {
            let arg_var = self.compile_expr(a);
            let result = self.fresh_label("app");
            let result_var = self.builder.var(&result, Width::I64);

            self.builder.push_arg(&arg_var);
            self.builder.enter();

            let param_var = self.builder.var(param, Width::I64);
            self.builder.pop_arg(&param_var);

            let body_result = self.compile_expr(body);

            let app_ret_label = self.fresh_label("app_ret");
            let ret_slot = self.builder.var(&app_ret_label, Width::I64);
            self.builder.copy_var(&ret_slot, &body_result);
            self.builder.push_arg(&ret_slot);
            self.builder.exit();

            self.builder.pop_arg(&result_var);

            return result_var;
        }

        // ── Generic application (fallback) ──────────────────────
        let f_var = self.compile_expr(f);
        let a_var = self.compile_expr(a);

        let result = self.fresh_label("app");
        let result_var = self.builder.var(&result, Width::I64);

        self.builder.push_arg(&f_var);
        self.builder.push_arg(&a_var);
        self.builder.enter();

        let arg_label = self.fresh_label("arg");
        let func_label = self.fresh_label("func");
        let temp_arg = self.builder.var(&arg_label, Width::I64);
        let temp_func = self.builder.var(&func_label, Width::I64);
        self.builder.pop_arg(&temp_arg);
        self.builder.pop_arg(&temp_func);
        self.builder.copy_var(&result_var, &temp_arg);
        self.builder.push_arg(&result_var);
        self.builder.exit();
        self.builder.pop_arg(&result_var);

        result_var
    }

    /// Compile a call to a native function: push args, emit CallNative.
    ///
    /// For unary: compile the single arg, push it, emit CallNative.
    /// The receiver (main.rs / VM runtime) decides what to do.
    fn compile_native_call(&mut self, func_name: &str, arity: u32, arg: &Ir) -> graftvm_ir::Var {
        // For unary: compile the argument
        let arg_var = self.compile_expr(arg);

        let result = self.fresh_label("native");
        let result_var = self.builder.var(&result, Width::I64);

        // Push arg, emit CallNative, pop result.
        self.builder.push_arg(&arg_var);
        self.builder.call_native(func_name, arity);
        self.builder.pop_arg(&result_var);

        result_var
    }

    /// Generic call to a named Parlance function.
    fn compile_generic_call(
        &mut self,
        func_name: &str,
        arg_var: graftvm_ir::Var,
    ) -> graftvm_ir::Var {
        let result = self.fresh_label("call");
        let result_var = self.builder.var(&result, Width::I64);

        self.builder.push_arg(&arg_var);
        self.builder.enter();

        let callee_arg = self.builder.var(&format!("{func_name}.arg"), Width::I64);
        self.builder.pop_arg(&callee_arg);

        let label = format!("def_{func_name}");
        self.builder.call(&label);

        self.builder.pop_arg(&result_var);

        result_var
    }
}
