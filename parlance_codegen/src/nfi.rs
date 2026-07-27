// ── Native Function Interface ────────────────────────────────────
//
//  NFI (Native Function Interface) is a registry of built-in
//  functions that the codegen knows how to compile to native
//  opcodes instead of generating generic function calls.
//
//  THEORY:
//    In a purely functional language, every function is a λ-term.
//    But some functions (I/O, arithmetic, etc.) MUST eventually
//    map to actual machine operations — they cannot be λ-terms
//    all the way down.  NFI is the bridge: it declares "these
//    names are native" and provides the codegen with the exact
//    opcode sequence to emit for each one.
//
//  ADDING A NEW NATIVE FUNCTION:
//    1. Add an entry to `NativeRegistry::builtins()`
//    2. Implement the handler in `compile_unary` or `compile_binary`
//    3. If the function body in prelude.plc doesn't match the
//       arity, the generic call path will still produce wrong
//       results — keep the placeholder body consistent.

use std::collections::HashMap;

use graftvm_bytecode::Width;
use graftvm_ir::{IrBuilder, Var};

/// Describes a native function known to the codegen.
#[allow(dead_code)]
pub struct NativeFn {
    pub name: &'static str,
    /// Number of arguments (1 = unary like `print`, 2 = binary like `add`).
    pub arity: u32,
}

/// Central registry of all native functions.
pub struct NativeRegistry {
    entries: HashMap<&'static str, NativeFn>,
}

impl NativeRegistry {
    /// Create the registry with all built-in native functions.
    pub fn builtins() -> Self {
        let mut reg = Self { entries: HashMap::new() };
        reg.add("print", 1);
        reg.add("add", 2);
        reg.add("sub", 2);
        reg.add("mul", 2);
        reg.add("div", 2);
        reg
    }

    fn add(&mut self, name: &'static str, arity: u32) {
        self.entries.insert(name, NativeFn { name, arity });
    }

    /// Check whether a name is a known native function.
    pub fn is_native(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Return all registered native function names.
    #[allow(dead_code)]
    pub fn all_names(&self) -> Vec<&'static str> {
        self.entries.keys().copied().collect()
    }

    /// Compile a unary native call: `func_name(arg)`.
    ///
    /// Returns `Some(var)` if the name is a known unary native
    /// (arity=1) and was handled; `None` to fall through to the
    /// generic function-call path.
    pub fn compile_unary(
        &self,
        builder: &mut IrBuilder,
        func_name: &str,
        arg_var: &Var,
    ) -> Option<Var> {
        match func_name {
            // print: syscall(0, arg) → write to stdout, return arg.
            "print" => {
                let result = builder.var("nfi.print", Width::I64);
                builder.syscall(0, arg_var, &result);
                Some(result)
            }
            _ => None,
        }
    }

    /// Compile a binary native call: `(func_name lhs)(rhs)`.
    ///
    /// Returns `Some(var)` if the name is a known binary native
    /// (arity=2) and was handled; `None` to fall through.
    pub fn compile_binary(
        &self,
        builder: &mut IrBuilder,
        func_name: &str,
        lhs_var: &Var,
        rhs_var: &Var,
    ) -> Option<Var> {
        let result = builder.var(&format!("nfi.{}", func_name), Width::I64);
        match func_name {
            "add" => builder.add(&result, lhs_var, rhs_var),
            "sub" => builder.sub(&result, lhs_var, rhs_var),
            "mul" => builder.mul(&result, lhs_var, rhs_var),
            "div" => builder.div(&result, lhs_var, rhs_var),
            _ => return None,
        }
        Some(result)
    }
}
