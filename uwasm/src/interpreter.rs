use alloc::vec::Vec;
use crate::parser::Reader;

struct VmContext {
    stack: VmStack,
}

struct VmStack {
    data: Vec<u8>,
}

impl VmStack {
    fn push_f64(&mut self, val: f64) {
        self.data.extend(val.to_le_bytes());
    }

    fn push_i32(&mut self, val: i32) {
        self.data.extend(val.to_le_bytes());
    }

    fn pop_bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut b = [0u8; N];
        for i in 0..N {
            b[N - i] = self.data.pop().unwrap();
        }
        b
    }

    #[inline]
    fn pop_i32(&mut self) -> i32 {
        i32::from_le_bytes(self.pop_bytes())
    }

    #[inline]
    fn pop_f64(&mut self) -> f64 {
        f64::from_le_bytes(self.pop_bytes())
    }
}

fn evaluate(ctx: &mut VmContext, reader: &mut Reader<'_>, params: &[u32]) {
    loop {
        let op = reader.read_u8().unwrap();
        match op {
            0x04 => {
                // if
                unimplemented!();
            }
            0x05 => {
                // else
                unimplemented!();
            }
            0x0b => {
                // end
                unimplemented!();
            }
            0x10 => {
                // call <func_idx>
                let func_idx = reader.read_usize().unwrap();
                unimplemented!();
            }
            0x20 => {
                // local.get <local>
                let local_idx = reader.read_u8().unwrap();
                ctx.stack.push_f64(params[local_idx as usize] as f64);
            }
            0x44 => {
                // f64.const <literal>
                let val = reader.read_f64().unwrap();
                ctx.stack.push_f64(val);
            }
            0x63 => {
                // f64.lt
                let a = ctx.stack.pop_f64();
                let b = ctx.stack.pop_f64();
                ctx.stack.push_f64((a < b) as i32 as f64);
            }
            0x6a => {
                // i32.add
                let a = ctx.stack.pop_i32();
                let b = ctx.stack.pop_i32();
                ctx.stack.push_i32(a + b);
            }
            0x7c => {
                // f64
                unimplemented!();
            }
            0xa1 => {
                // f64.sub
                let a = ctx.stack.pop_f64();
                let b = ctx.stack.pop_f64();
                ctx.stack.push_f64(a - b);
            }
            0xa2 => {
                // f64.mul
                let a = ctx.stack.pop_f64();
                let b = ctx.stack.pop_f64();
                ctx.stack.push_f64(a * b);
            }
            _ => unimplemented!("opcode {:02x?}", op),
        }
    }
}