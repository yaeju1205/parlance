#[repr(u8)]
#[derive(Debug)]
pub enum Operator {
    Goto,
    Mov,
    Ret,
    Call,
    CallReg,
    LoadFunc,
    LoadInt,
    LoadStr,
    AddInt,
    SubInt,
    MulInt,
    DivInt,
    Print,
}

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
    FuncPtr { pc: usize, param_register: usize },
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
            match inst.operator {
                Operator::Goto => pc = inst.a,
                Operator::Mov => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        self.register_file.get_unchecked(inst.b).clone();
                },
                Operator::Ret => {
                    let ret = unsafe { self.register_file.get_unchecked(inst.a).clone() };

                    let frame = unsafe { self.call_stack.pop().unwrap_unchecked() };

                    unsafe {
                        *self.register_file.get_unchecked_mut(frame.dest_register) = ret;
                    }

                    pc = frame.return_pc;

                    continue;
                }
                Operator::Call => {
                    self.call_stack.push(FrameInfo {
                        return_pc: pc + 1,
                        dest_register: inst.a,
                    });

                    pc = inst.b;

                    continue;
                }
                Operator::CallReg => {
                    self.call_stack.push(FrameInfo {
                        return_pc: pc + 1,
                        dest_register: inst.a,
                    });

                    unsafe {
                        match self.register_file[inst.b] {
                            VirtualMachineData::FuncPtr {
                                pc: target_pc,
                                param_register,
                            } => {
                                *self.register_file.get_unchecked_mut(param_register) =
                                    self.register_file.get_unchecked(inst.c).clone();
                                pc = target_pc;
                            }
                            _ => std::hint::unreachable_unchecked(),
                        }
                    };

                    continue;
                }
                Operator::LoadFunc => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) = VirtualMachineData::FuncPtr {
                        pc: inst.b,
                        param_register: inst.c,
                    };
                },
                Operator::LoadInt => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        VirtualMachineData::Int(inst.b as i32);
                },
                Operator::LoadStr => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a) =
                        self.data_pool.get_unchecked(inst.b).clone()
                },
                Operator::AddInt => {
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
                Operator::SubInt => {
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
                            VirtualMachineData::Int(l_val - r_val);
                    }
                }
                Operator::MulInt => {
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
                            VirtualMachineData::Int(l_val * r_val);
                    }
                }
                Operator::DivInt => {
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
                            VirtualMachineData::Int(l_val / r_val);
                    }
                }
                Operator::Print => unsafe {
                    println!(
                        "parlance print > {:?}",
                        self.register_file.get_unchecked_mut(inst.a)
                    );
                },
            }

            pc += 1;
        }
    }
}
