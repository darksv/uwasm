mod reg_based {
    #[repr(u8)]
    #[derive(Copy, Clone)]
    enum Reg {
        R0 = 0x00,
        R1 = 0x01,
        R2 = 0x02,
        R3 = 0x03,
    }

    #[derive(Copy, Clone)]
    #[repr(u8)]
    enum Instr {
        SetReg { dst: Reg, val: u32 },
        Add { src_a: Reg, src_b: Reg, dst: Reg },
        Sub { src_a: Reg, src_b: Reg, dst: Reg },
        JumpNonZero { src: Reg, target: u16 },
        Yield,
        Halt,
        Call { func_idx: u16 },
    }

    struct StackFrame {
        func_idx: usize,
        pc: usize,
        regs: [u32; 4],
    }

    impl StackFrame {
        fn new(func: usize) -> Self {
            Self {
                func_idx: func,
                pc: 0,
                regs: [0u32; 4],
            }
        }
    }

    enum FuncBody {
        Code(&'static [Instr]),
        Native(u16),
    }

    struct Func {
        code: FuncBody,
    }

    pub(crate) fn run() {
        let functions = [
            Func {
                code: FuncBody::Code(&[
                    Instr::SetReg { dst: Reg::R0, val: 123 },
                    Instr::SetReg { dst: Reg::R1, val: 1 },
                    Instr::Sub { dst: Reg::R0, src_a: Reg::R0, src_b: Reg::R1 },
                    Instr::Yield,
                    Instr::JumpNonZero { src: Reg::R0, target: 2 },
                    Instr::SetReg { dst: Reg::R0, val: 10 },
                    Instr::Yield,
                    Instr::Call { func_idx: 1 },
                    Instr::Yield,
                    Instr::Halt,
                ])
            },
            Func {
                code: FuncBody::Native(123),
            }
        ];

        let mut call_stack: Vec<StackFrame> = Vec::new();
        call_stack.push(StackFrame::new(0));

        loop {
            let mut frame = call_stack.last_mut().unwrap();
            let code = match functions[frame.func_idx].code {
                FuncBody::Code(code) => code,
                FuncBody::Native(_) => unreachable!(),
            };

            let Some(op) = code.get(frame.pc).copied() else {
                call_stack.pop();
                continue;
            };

            match op {
                Instr::SetReg { dst, val } => {
                    frame.regs[dst as usize] = val;
                    frame.pc += 1;
                }
                Instr::Add { src_a, src_b, dst } => {
                    frame.regs[dst as usize] = frame.regs[src_a as usize].checked_add(frame.regs[src_b as usize]).unwrap();
                    frame.pc += 1;
                }
                Instr::Sub { src_a, src_b, dst } => {
                    frame.regs[dst as usize] = frame.regs[src_a as usize].checked_sub(frame.regs[src_b as usize]).unwrap();
                    frame.pc += 1;
                }
                Instr::JumpNonZero { src, target } => {
                    if frame.regs[src as usize] != 0 {
                        frame.pc = usize::from(target);
                    } else {
                        frame.pc += 1;
                    }
                }
                Instr::Yield => {
                    println!("regs: {:?}", frame.regs);
                    frame.pc += 1;
                }
                Instr::Halt => {
                    frame.pc += 1;
                    break;
                }
                Instr::Call { func_idx } => {
                    frame.pc += 1;
                    match functions[usize::from(func_idx)].code {
                        FuncBody::Code(_) => {
                            call_stack.push(StackFrame::new(usize::from(func_idx)))
                        }
                        FuncBody::Native(idx) => {
                            match idx {
                                123 => {
                                    frame.regs[0] = frame.regs[0].pow(2);
                                }
                                _ => unimplemented!("calling native #{idx}"),
                            }
                        }
                    }
                }
            }
        }
    }
}

mod stack_based {
    #[derive(Copy, Clone)]
    #[repr(u8)]
    enum Instr {
        Push { val: u32 },
        Add,
        Sub,
        JumpNonZero { target: u16 },
        Yield,
        Halt,
    }

    pub(crate) fn run() {
        let code = [
            Instr::Push { val: 123 },
            Instr::Push { val: 1 },
            Instr::Sub,
            Instr::Yield,
            Instr::JumpNonZero { target: 1 },
            Instr::Halt,
        ];

        let mut stack = Vec::new();
        let mut pc = 0;
        while pc < code.len() {
            match code[pc] {
                Instr::Push { val } => {
                    stack.push(val);
                    pc += 1;
                }
                Instr::Add => {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    stack.push(a + b);
                    pc += 1;
                }
                Instr::Sub => {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    stack.push(b - a);
                    pc += 1;
                }
                Instr::JumpNonZero { target } => {
                    if stack.last().copied().unwrap() != 0 {
                        pc = usize::from(target);
                    } else {
                        pc += 1;
                    }
                }
                Instr::Yield => {
                    println!("stack: {:?}", stack);
                    pc += 1;
                }
                Instr::Halt => {
                    pc += 1;
                    break;
                }
            }
        }
    }
}

fn main() {
    reg_based::run();
    // stack_based::run();
    // println!("Hello, world!");
}
