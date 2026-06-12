use std::{fmt, mem, ops, rc::Rc};

pub type Register = u16;
pub type ProgramCount = u32; // u24
pub type DataPoolIndex = u32; // u24

pub trait RustCallFunction {
    fn call(&self, arg: VirtualMachineData) -> VirtualMachineData;
}

impl fmt::Debug for dyn RustCallFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rust native function")
    }
}

#[repr(u8)]
#[derive(Debug)]
pub enum Opcode {
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
    Cmp,
    JmpEq,

    RustCall,
}

pub type InstructionSize = u64;

#[repr(transparent)]
#[derive(Debug)]
pub struct Instruction(InstructionSize);

impl Instruction {
    #[inline(always)]
    pub fn opcode(&self) -> Opcode {
        unsafe { mem::transmute((self.0 & 0xff) as u8) }
    }

    #[inline(always)]
    pub fn goto(pc: ProgramCount) -> Self {
        Self((Opcode::Goto as InstructionSize) | ((pc as InstructionSize) << 8))
    }

    #[inline(always)]
    pub fn mov(dest: Register, src: Register) -> Self {
        Self(
            (Opcode::Mov as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((src as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn ret(src: Register) -> Self {
        Self((Opcode::Ret as InstructionSize) | ((src as InstructionSize) << 8))
    }

    #[inline(always)]
    pub fn call(dest: Register, pc: ProgramCount) -> Self {
        Self(
            (Opcode::Call as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((pc as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn call_reg(dest: Register, func: Register, arg: Register) -> Self {
        Self(
            (Opcode::CallReg as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((func as InstructionSize) << 24)
                | ((arg as InstructionSize) << 40),
        )
    }

    #[inline(always)]
    pub fn tail_call_reg(func: Register, arg: Register) -> Self {
        Self(
            (Opcode::TailCallReg as InstructionSize)
                | ((func as InstructionSize) << 8)
                | ((arg as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn load_func(dest: Register, pc: ProgramCount, param: Register) -> Self {
        Self(
            (Opcode::LoadFunc as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((pc as InstructionSize) << 24)
                | ((param as InstructionSize) << 48),
        )
    }

    #[inline(always)]
    pub fn load_int(dest: Register, value: i32) -> Self {
        Self(
            (Opcode::LoadInt as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | (((value as u32) as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn load_str(dest: Register, pool_index: DataPoolIndex) -> Self {
        Self(
            (Opcode::LoadStr as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((pool_index as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn add_int(dest: Register, lhs: Register, rhs: Register) -> Self {
        Self(
            (Opcode::AddInt as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((lhs as InstructionSize) << 24)
                | ((rhs as InstructionSize) << 40),
        )
    }

    #[inline(always)]
    pub fn cmp(dest: Register, lhs: Register, rhs: Register) -> Self {
        Self(
            (Opcode::Cmp as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((lhs as InstructionSize) << 24)
                | ((rhs as InstructionSize) << 40),
        )
    }

    #[inline(always)]
    pub fn jmp_eq(cond: Register, pc: ProgramCount) -> Self {
        Self(
            (Opcode::JmpEq as InstructionSize)
                | ((cond as InstructionSize) << 8)
                | ((pc as InstructionSize) << 24),
        )
    }

    #[inline(always)]
    pub fn rust_call(dest: Register, func: DataPoolIndex, arg: Register) -> Self {
        Self(
            (Opcode::RustCall as InstructionSize)
                | ((dest as InstructionSize) << 8)
                | ((func as InstructionSize) << 24)
                | ((arg as InstructionSize) << 48),
        )
    }
}

pub type Bytecode = Vec<Instruction>;

#[derive(Debug, Clone)]
pub enum VirtualMachineData {
    Bool(bool),
    Int(i32),
    StrPtr(Rc<str>),
    FuncPtr {
        pc: ProgramCount,
        param_register: Register,
    },
    None,

    RustFnPtr(fn(VirtualMachineData) -> VirtualMachineData),
    RustFnTrait(Rc<dyn RustCallFunction>),
}

impl PartialEq for VirtualMachineData {
    fn eq(&self, other: &Self) -> bool {
        match self {
            VirtualMachineData::RustFnTrait(_) => {
                if let VirtualMachineData::RustFnTrait(_) = other {
                    true
                } else {
                    false
                }
            }
            _ => self == other,
        }
    }
}

#[derive(Debug)]
pub struct FrameInfo {
    return_pc: ProgramCount,
    dest_register: Register,
}

struct RegisterFile(Vec<VirtualMachineData>);

impl ops::Index<Register> for RegisterFile {
    type Output = VirtualMachineData;

    fn index(&self, index: Register) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl ops::IndexMut<Register> for RegisterFile {
    fn index_mut(&mut self, index: Register) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

#[derive(Debug, Default)]
pub struct DataPool(Vec<VirtualMachineData>);

impl DataPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, data_pool: &mut DataPool) {
        self.0.append(&mut data_pool.0);
    }

    pub fn len(&mut self) -> DataPoolIndex {
        self.0.len() as DataPoolIndex
    }

    pub fn push(&mut self, data: VirtualMachineData) {
        self.0.push(data);
    }
}

impl ops::Index<DataPoolIndex> for DataPool {
    type Output = VirtualMachineData;

    fn index(&self, index: DataPoolIndex) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl ops::IndexMut<DataPoolIndex> for DataPool {
    fn index_mut(&mut self, index: DataPoolIndex) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

pub struct VirtualMachine {
    bytecode: Bytecode,
    data_pool: DataPool,
    register_file: RegisterFile,
    pc: ProgramCount,
    call_stack: Vec<FrameInfo>,
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self {
            bytecode: Vec::new(),
            data_pool: DataPool(Vec::new()),
            register_file: RegisterFile(vec![VirtualMachineData::None; 256]),
            pc: 0,
            call_stack: Vec::with_capacity(32),
        }
    }

    pub fn with_load(mut self, (pc, bytecode, data_pool): (u32, Bytecode, DataPool)) -> Self {
        self.pc = pc;
        self.bytecode = bytecode;
        self.data_pool = data_pool;
        self
    }

    pub unsafe fn run(&mut self) {
        let mut pc = self.pc;

        while self.bytecode.len() > pc as usize {
            let inst = &self.bytecode[pc as usize];

            match inst.opcode() {
                Opcode::Goto => {
                    pc = (inst.0 >> 8) as ProgramCount;
                    continue;
                }

                Opcode::Mov => {
                    let dst = (inst.0 >> 8) as Register;
                    let src = (inst.0 >> 24) as Register;

                    self.register_file[dst] = self.register_file[src].clone();
                }

                Opcode::Ret => {
                    let ret = self.register_file[(inst.0 >> 8) as Register].clone();

                    let frame = self.call_stack.pop().unwrap();

                    self.register_file[frame.dest_register] = ret;

                    pc = frame.return_pc;

                    continue;
                }

                Opcode::Call => {
                    self.call_stack.push(FrameInfo {
                        return_pc: pc + 1,
                        dest_register: (inst.0 >> 8) as Register,
                    });

                    pc = ((inst.0 >> 24) & 0xFFFFFF) as ProgramCount;

                    continue;
                }

                Opcode::CallReg => {
                    let func = self.register_file[(inst.0 >> 24) as Register].clone();

                    match func {
                        VirtualMachineData::FuncPtr {
                            pc: target_pc,
                            param_register,
                        } => {
                            self.register_file[param_register] =
                                self.register_file[(inst.0 >> 40) as Register].clone();

                            self.call_stack.push(FrameInfo {
                                return_pc: pc + 1,
                                dest_register: (inst.0 >> 8) as Register,
                            });

                            pc = target_pc;
                        }

                        _ => unreachable!(),
                    }

                    continue;
                }

                Opcode::TailCallReg => {
                    let func = self.register_file[(inst.0 >> 8) as Register].clone();

                    match func {
                        VirtualMachineData::FuncPtr {
                            pc: target_pc,
                            param_register,
                        } => {
                            self.register_file[param_register] =
                                self.register_file[(inst.0 >> 24) as Register].clone();

                            pc = target_pc;
                        }

                        _ => unreachable!(),
                    }

                    continue;
                }

                Opcode::LoadFunc => {
                    self.register_file[(inst.0 >> 8) as Register] = VirtualMachineData::FuncPtr {
                        pc: ((inst.0 >> 24) & 0xFFFFFF) as ProgramCount,
                        param_register: (inst.0 >> 48) as Register,
                    }
                }
                Opcode::LoadInt => {
                    self.register_file[(inst.0 >> 8) as Register] =
                        VirtualMachineData::Int((inst.0 >> 24) as u32 as i32);
                }

                Opcode::LoadStr => {
                    self.register_file[(inst.0 >> 8) as Register] =
                        self.data_pool[(inst.0 >> 24) as DataPoolIndex].clone();
                }

                Opcode::AddInt => {
                    let lhs = match &self.register_file[(inst.0 >> 24) as Register] {
                        VirtualMachineData::Int(v) => *v,
                        _ => unreachable!(),
                    };

                    let rhs = match &self.register_file[(inst.0 >> 40) as Register] {
                        VirtualMachineData::Int(v) => *v,
                        _ => unreachable!(),
                    };

                    self.register_file[(inst.0 >> 8) as Register] =
                        VirtualMachineData::Int(lhs + rhs);
                }
                Opcode::Cmp => {
                    let lhs = &self.register_file[(inst.0 >> 24) as Register];
                    let rhs = &self.register_file[(inst.0 >> 40) as Register];

                    self.register_file[(inst.0 >> 8) as Register] =
                        VirtualMachineData::Bool(rhs == lhs);
                }
                Opcode::RustCall => {
                    let callee = &self.register_file[(inst.0 >> 48) as Register];
                    self.register_file[(inst.0 >> 8) as Register] =
                        match &self.data_pool[((inst.0 >> 24) & 0xFFFFFF) as DataPoolIndex] {
                            VirtualMachineData::RustFnPtr(f) => f(callee.clone()),
                            VirtualMachineData::RustFnTrait(nf) => nf.call(callee.clone()),
                            _ => unreachable!(),
                        };
                }
                Opcode::JmpEq => {
                    let cond = &self.register_file[(inst.0 >> 8) as Register];
                    match cond {
                        VirtualMachineData::Bool(cond_bool) => {
                            if *cond_bool {
                                pc = (inst.0 >> 24) as ProgramCount;
                            }
                        }
                        VirtualMachineData::None => {}
                        _ => {
                            pc = (inst.0 >> 24) as ProgramCount;
                        }
                    }

                    continue;
                }
            }

            pc += 1;
        }
    }
}
