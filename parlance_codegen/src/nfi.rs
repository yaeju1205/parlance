// ── Native Function Interface (NFI) ──────────────────────────────
//
//  NFI identifies which functions are "native" (implemented by the
//  host runtime).  The codegen uses this to decide whether to emit
//  a `CallNative` opcode or a generic `Call` to a compiled function.
//
//  The registry is populated from OUTSIDE the compiler — the codegen
//  itself knows nothing about specific function names.

use std::collections::HashSet;

/// Registry of native function names (the codegen only needs to know
/// which names are native — the actual implementation lives in the
/// VM's native table at runtime).
pub struct NativeRegistry {
    names: HashSet<String>,
}

impl NativeRegistry {
    pub fn empty() -> Self {
        Self { names: HashSet::new() }
    }

    /// Create registry from a list of native names declared in IR.
    pub fn from_ir(defs: &[parlance_ir::IrDef]) -> Self {
        let mut reg = Self::empty();
        for def in defs {
            if let parlance_ir::IrDef::Native { name, .. } = def {
                reg.names.insert(name.clone());
            }
        }
        reg
    }

    pub fn is_native(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Get all registered native names.
    pub fn all_names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }
}
