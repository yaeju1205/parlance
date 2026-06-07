#[repr(u8)]
#[derive(Debug)]
pub enum Operator {
    Goto,
    Mov,
    Ret,
    Call,
    CallReg,
    TailCallReg,
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
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

pub type Bytecode = Vec<Instruction>;
pub type DataPool = Vec<VirtualMachineData>;

#[derive(Debug, Clone)]
pub enum VirtualMachineData {
    Int(i32),
    StrPtr(*const str),
    FuncPtr { pc: u32, param_register: u32 },
    None,
}

struct FrameInfo {
    return_pc: u32,
    dest_register: u32,
}

pub struct VirtualMachine {
    bytecode: Bytecode,
    data_pool: DataPool,
    pc: u32,
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

    pub fn load(&mut self, (pc, bytecode, data_pool): (u32, Bytecode, DataPool)) {
        self.pc = pc;
        self.bytecode = bytecode;
        self.data_pool = data_pool;
    }

    pub fn with_load(mut self, (pc, bytecode, data_pool): (u32, Bytecode, DataPool)) -> Self {
        self.pc = pc;
        self.bytecode = bytecode;
        self.data_pool = data_pool;
        self
    }

    #[inline(always)]
    pub unsafe fn run(&mut self) {
        let mut pc = self.pc;

        let code_len = self.bytecode.len();

        while (pc as usize) < code_len {
            let inst = unsafe { self.bytecode.get_unchecked(pc as usize) };
            match inst.operator {
                Operator::Goto => pc = inst.a,
                Operator::Mov => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a as usize) =
                        self.register_file.get_unchecked(inst.b as usize).clone();
                },
                Operator::Ret => {
                    let ret = unsafe { self.register_file.get_unchecked(inst.a as usize).clone() };

                    let frame = unsafe { self.call_stack.pop().unwrap_unchecked() };

                    unsafe {
                        *self
                            .register_file
                            .get_unchecked_mut(frame.dest_register as usize) = ret;
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
                        match self.register_file[inst.b as usize] {
                            VirtualMachineData::FuncPtr {
                                pc: target_pc,
                                param_register,
                            } => {
                                *self
                                    .register_file
                                    .get_unchecked_mut(param_register as usize) =
                                    self.register_file.get_unchecked(inst.c as usize).clone();
                                pc = target_pc;
                            }
                            _ => std::hint::unreachable_unchecked(),
                        }
                    };

                    continue;
                }
                Operator::TailCallReg => {
                    unsafe {
                        match self.register_file[inst.b as usize] {
                            VirtualMachineData::FuncPtr {
                                pc: target_pc,
                                param_register,
                            } => {
                                *self
                                    .register_file
                                    .get_unchecked_mut(param_register as usize) =
                                    self.register_file.get_unchecked(inst.c as usize).clone();

                                pc = target_pc;
                            }
                            _ => std::hint::unreachable_unchecked(),
                        }
                    };
                    continue;
                }
                Operator::LoadFunc => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a as usize) =
                        VirtualMachineData::FuncPtr {
                            pc: inst.b,
                            param_register: inst.c,
                        };
                },
                Operator::LoadInt => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a as usize) =
                        VirtualMachineData::Int(inst.b as i32);
                },
                Operator::LoadStr => unsafe {
                    *self.register_file.get_unchecked_mut(inst.a as usize) =
                        self.data_pool.get_unchecked(inst.b as usize).clone()
                },
                Operator::AddInt => {
                    let lhs = unsafe { self.register_file.get_unchecked(inst.b as usize) };
                    let rhs = unsafe { self.register_file.get_unchecked(inst.c as usize) };

                    unsafe {
                        let l_val = match lhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        let r_val = match rhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        *self.register_file.get_unchecked_mut(inst.a as usize) =
                            VirtualMachineData::Int(l_val + r_val);
                    }
                }
                Operator::SubInt => {
                    let lhs = unsafe { self.register_file.get_unchecked(inst.b as usize) };
                    let rhs = unsafe { self.register_file.get_unchecked(inst.c as usize) };

                    unsafe {
                        let l_val = match lhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        let r_val = match rhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        *self.register_file.get_unchecked_mut(inst.a as usize) =
                            VirtualMachineData::Int(l_val - r_val);
                    }
                }
                Operator::MulInt => {
                    let lhs = unsafe { self.register_file.get_unchecked(inst.b as usize) };
                    let rhs = unsafe { self.register_file.get_unchecked(inst.c as usize) };

                    unsafe {
                        let l_val = match lhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        let r_val = match rhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        *self.register_file.get_unchecked_mut(inst.a as usize) =
                            VirtualMachineData::Int(l_val * r_val);
                    }
                }
                Operator::DivInt => {
                    let lhs = unsafe { self.register_file.get_unchecked(inst.b as usize) };
                    let rhs = unsafe { self.register_file.get_unchecked(inst.c as usize) };

                    unsafe {
                        let l_val = match lhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        let r_val = match rhs {
                            VirtualMachineData::Int(v) => v.clone(),
                            _ => std::hint::unreachable_unchecked(),
                        };

                        *self.register_file.get_unchecked_mut(inst.a as usize) =
                            VirtualMachineData::Int(l_val / r_val);
                    }
                }
                Operator::Print => unsafe {
                    println!(
                        "parlance print > {:?}",
                        self.register_file.get_unchecked_mut(inst.a as usize)
                    );
                },
            }

            pc += 1;
        }
    }
}
