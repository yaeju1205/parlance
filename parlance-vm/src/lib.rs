pub type Operator = u8;

pub const OPERATOR_GOTO: Operator = 0;
pub const OPERATOR_MOVE: Operator = 1;
pub const OPERATOR_CALL: Operator = 2;
pub const OPERATOR_RET: Operator = 3;
pub const OPERATOR_LOAD_INT: Operator = 4;
pub const OPERATOR_LOAD_STR: Operator = 5;
pub const OPERATOR_ADD_INT: Operator = 6;
pub const OPERATOR_PRINT: Operator = 7;
pub const OPERATOR_STOP: Operator = 8;

#[derive(Debug)]
pub struct Instruction {
    pub operator: Operator,
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

pub type Bytecode = Vec<Instruction>;
pub type DataPool = Vec<VirtualMachineData>;

#[derive(Debug, Clone)]
pub enum VirtualMachineData {
    Int(i32),
    StrPtr(*const str),
    None,
}

struct FrameInfo {
    return_pc: usize,
    dest_register: usize,
}

pub struct VirtualMachine {
    bytecode: Bytecode,
    data_pool: DataPool,
    pc: usize,
    register_file: Vec<VirtualMachineData>,
    call_stack: Vec<FrameInfo>,
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self {
            bytecode: Vec::new(),
            data_pool: Vec::new(),
            pc: 0,
            register_file: vec![VirtualMachineData::None; 1024],
            call_stack: Vec::with_capacity(32),
        }
    }

    pub fn load(&mut self, pc: usize, bytecode: Bytecode, data_pool: DataPool) {
        self.pc = pc;
        self.bytecode = bytecode;
        self.data_pool = data_pool;
    }

    #[inline(always)]
    pub unsafe fn run(&mut self) {
        let mut pc = self.pc;

        let code_len = self.bytecode.len();

        while pc < code_len {
            let inst = unsafe { self.bytecode.get_unchecked(pc) };
            println!("running pc: {pc}");
            println!("operator: {}", inst.operator);
            match inst.operator {
                OPERATOR_GOTO => pc = inst.a,
                OPERATOR_MOVE => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        self.register_file.get_unchecked(inst.b).clone();
                },
                OPERATOR_CALL => {
                    self.call_stack.push(FrameInfo {
                        return_pc: pc + 1,
                        dest_register: inst.a,
                    });

                    pc = inst.b;

                    continue;
                }
                OPERATOR_RET => {
                    let ret = unsafe { self.register_file.get_unchecked(inst.a).clone() };

                    let frame = unsafe { self.call_stack.pop().unwrap_unchecked() };

                    unsafe {
                        *self.register_file.get_unchecked_mut(frame.dest_register) = ret;
                    }

                    pc = frame.return_pc;

                    continue;
                }
                OPERATOR_LOAD_INT => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        VirtualMachineData::Int(inst.b as i32);
                },
                OPERATOR_LOAD_STR => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        self.data_pool.get_unchecked(inst.b).clone()
                },
                OPERATOR_ADD_INT => {
                    let lhs = unsafe { self.register_file.get_unchecked(inst.b) };
                    let rhs = unsafe { self.register_file.get_unchecked(inst.c) };

                    unsafe {
                        let l_val = match lhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        let r_val = match rhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        *self.register_file.get_unchecked_mut(inst.a) =
                            VirtualMachineData::Int(l_val + r_val);
                    }
                }
                OPERATOR_PRINT => unsafe {
                    println!(
                        "parlance print > {:?}",
                        self.register_file.get_unchecked_mut(inst.a)
                    );
                },
                OPERATOR_STOP => {
                    return;
                }
                _ => unimplemented!(),
            }

            pc += 1;
        }
    }
}
