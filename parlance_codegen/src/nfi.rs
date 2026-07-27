// ── Native Function Interface (NFI) ───────────────────────────────
//
//  NFI is a registry that maps function names to their codegen
//  behaviour.  The registry is populated from OUTSIDE the compiler
//  (CLI flags, config files, etc.) — the codegen itself knows
//  nothing about specific function names.
//
//  Each entry specifies:
//    - name:     the Parlance function name (e.g. "print", "add")
//    - arity:    1 (unary) or 2 (binary)
//    - emit_fn:  closure that emits the actual opcodes

use std::collections::HashMap;
use graftvm_bytecode::Width;
use graftvm_ir::{IrBuilder, Var};

/// How to compile a native function: a closure that receives the
/// builder and the compiled argument variables, and returns the
/// result variable.
pub type EmitFn = fn(&mut IrBuilder, &[&Var]) -> Var;

/// One native function entry.
pub struct NativeEntry {
    pub name: String,
    pub arity: u32,
    pub emit_fn: EmitFn,
}

/// Registry of native functions.
pub struct NativeRegistry {
    entries: HashMap<String, NativeEntry>,
}

/// Default set of native functions that ship with Parlance.
pub fn default_natives() -> Vec<(&'static str, u32, EmitFn)> {
    vec![
        // print x  →  syscall(0, x)  →  write to stdout
        ("print", 1, |b, args| {
            let r = b.var("nfi.print", Width::I64);
            b.syscall(0, args[0], &r);
            r
        }),
        // add x y  →  x + y
        ("add", 2, |b, args| {
            let r = b.var("nfi.add", Width::I64);
            b.add(&r, args[0], args[1]);
            r
        }),
        // sub x y  →  x - y
        ("sub", 2, |b, args| {
            let r = b.var("nfi.sub", Width::I64);
            b.sub(&r, args[0], args[1]);
            r
        }),
        // mul x y  →  x * y
        ("mul", 2, |b, args| {
            let r = b.var("nfi.mul", Width::I64);
            b.mul(&r, args[0], args[1]);
            r
        }),
        // div x y  →  x / y
        ("div", 2, |b, args| {
            let r = b.var("nfi.div", Width::I64);
            b.div(&r, args[0], args[1]);
            r
        }),
    ]
}

impl NativeRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Create a registry with default natives.
    pub fn with_defaults() -> Self {
        let mut reg = Self::empty();
        for (name, arity, emit_fn) in default_natives() {
            reg.add(name.to_string(), arity, emit_fn);
        }
        reg
    }

    /// Add or override a native entry.
    pub fn add(&mut self, name: String, arity: u32, emit_fn: EmitFn) {
        self.entries.insert(name.clone(), NativeEntry { name, arity, emit_fn });
    }

    /// Check whether a name is a known native function.
    pub fn is_native(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Get all registered names.
    pub fn all_names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Compile a native function call.
    ///
    /// `args` must already be compiled Var values.  Returns the
    /// result Var, or `None` if `name` is not registered.
    pub fn compile(&self, builder: &mut IrBuilder, name: &str, args: &[&Var]) -> Option<Var> {
        self.entries.get(name).map(|entry| (entry.emit_fn)(builder, args))
    }
}
