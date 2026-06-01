use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::Diagnostics;
use parlance_parser::Statement;
use parlance_vm::{Instruction, OPERATOR_CALL, OPERATOR_LOAD_INT, OPERATOR_MOVE};

use crate::{
    allocator::{Allocator, RegisterMap},
    desugarer::Desugarer,
    flattener::{Flatten, FlattenValueKind, Flattener},
};

mod allocator;
mod desugarer;
mod flattener;

pub type InstructionIndex = usize;

pub struct Compiler {
    register_map: RegisterMap,
    flatten: Rc<Flatten>,
}

impl Compiler {
    pub fn new(stats: Vec<Statement>) -> Result<Self, Diagnostics> {
        let bindings = Desugarer::default().desugar(stats)?;
        let flatten = Rc::new(Flattener::default().flatten(bindings)?);
        Ok(Self {
            register_map: Allocator::new(flatten.clone()).alloc(),
            flatten,
        })
    }

    pub fn compile(&self) -> Result<Vec<Instruction>, Diagnostics> {}
}
