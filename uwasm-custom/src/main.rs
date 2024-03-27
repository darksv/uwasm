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
    }

    pub(crate) fn run() {
        let code = [
            Instr::SetReg { dst: Reg::R0, val: 123 },
            Instr::SetReg { dst: Reg::R1, val: 1 },
            Instr::Sub { dst: Reg::R0, src_a: Reg::R0, src_b: Reg::R1 },
            Instr::Yield,
            Instr::JumpNonZero { src: Reg::R0, target: 2 },
            Instr::Halt,
        ];

        let mut regs = [0u32; 4];
        let mut pc = 0;
        while pc < code.len() {
            match code[pc] {
                Instr::SetReg { dst, val } => {
                    regs[dst as usize] = val;
                    pc += 1;
                }
                Instr::Add { src_a, src_b, dst } => {
                    regs[dst as usize] = regs[src_a as usize].checked_add(regs[src_b as usize]).unwrap();
                    pc += 1;
                }
                Instr::Sub { src_a, src_b, dst } => {
                    regs[dst as usize] = regs[src_a as usize].checked_sub(regs[src_b as usize]).unwrap();
                    pc += 1;
                }
                Instr::JumpNonZero { src, target } => {
                    if regs[src as usize] != 0 {
                        pc = usize::from(target);
                    } else {
                        pc += 1;
                    }
                }
                Instr::Yield => {
                    println!("regs: {:?}", regs);
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
                Instr::Add  => {
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
    stack_based::run();

    println!("Hello, world!");
}
