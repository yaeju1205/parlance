pub type Operator = u8;

pub const OPERATOR_MOVE: Operator = 0;
pub const OPERATOR_CALL: Operator = 1;
pub const OPERATOR_RET: Operator = 2;
pub const OPERATOR_LOAD_INT: Operator = 3;
pub const OPERATOR_LOAD_STR: Operator = 4;

pub struct Instruction {
    pub operator: Operator,
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

pub type Bytecode = Vec<Instruction>;

#[derive(Clone)]
pub enum VirtualMachineData {
    Int(i32),
    StrPtr(*const str),
    None,
}

struct FrameInfo {
    return_pc: usize,
    old_fp: usize,
    dest_register: usize,
}

pub struct VirtualMachine {
    bytecode: Bytecode,
    data_pool: Vec<VirtualMachineData>,
    register_file: Vec<VirtualMachineData>,
    call_stack: Vec<FrameInfo>,
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self {
            bytecode: Vec::new(),
            data_pool: Vec::new(),
            register_file: vec![VirtualMachineData::None; 1024],
            call_stack: Vec::with_capacity(32),
        }
    }

    pub fn load(&mut self, bytecode: Bytecode, data_pool: Vec<VirtualMachineData>) {
        self.bytecode = bytecode;
        self.data_pool = data_pool;
    }

    pub fn run(&mut self) {
        let mut pc = 0;
        let mut fp = 0;

        let code_len = self.bytecode.len();

        while pc < code_len {
            let inst = unsafe { self.bytecode.get_unchecked(pc) };

            match inst.operator {
                OPERATOR_MOVE => unsafe {
                    *self.register_file.get_unchecked_mut(fp + inst.a) =
                        self.register_file.get_unchecked(fp + inst.b).clone();
                },
                OPERATOR_CALL => {
                    self.call_stack.push(FrameInfo {
                        return_pc: pc + 1,
                        old_fp: fp,
                        dest_register: inst.a,
                    });

                    fp += inst.c;
                    pc = inst.b;
                }
                OPERATOR_RET => {
                    let ret = unsafe { self.register_file.get_unchecked(fp + inst.a).clone() };

                    let frame = unsafe { self.call_stack.pop().unwrap_unchecked() };
                    fp = frame.old_fp;

                    unsafe {
                        *self
                            .register_file
                            .get_unchecked_mut(fp + frame.dest_register) = ret;
                    }

                    pc = frame.return_pc;
                }
                OPERATOR_LOAD_INT => unsafe {
                    *self.register_file.get_unchecked_mut(fp + inst.a) =
                        VirtualMachineData::Int(inst.b as i32)
                },
                OPERATOR_LOAD_STR => unsafe {
                    *self.register_file.get_unchecked_mut(fp + inst.a) =
                        self.data_pool.get_unchecked(inst.b).clone()
                },
                _ => unimplemented!(),
            }
        }
    }
}
