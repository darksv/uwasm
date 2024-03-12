use alloc::vec::Vec;
use crate::{Context, FuncBody, FuncSignature, ParserError};
use crate::parser::{Reader, TypeKind};

pub struct VmContext {
    pub(crate) stack: VmStack,
    call_stack: Vec<StackFrame>,
}

impl VmContext {
    pub fn new() -> Self {
        Self {
            stack: VmStack { data: Vec::new() },
            call_stack: Vec::new(),
        }
    }
}

struct StackFrame {
    func_idx: usize,
}

pub struct VmStack {
    data: Vec<u8>,
}

impl VmStack {
    fn push_bytes<const N: usize>(&mut self, data: [u8; N]) {
        self.data.extend(data);
    }
    fn push_f64(&mut self, val: f64) {
        self.push_bytes(val.to_le_bytes());
    }

    fn push_i32(&mut self, val: i32) {
        self.data.extend(val.to_le_bytes());
    }

    #[track_caller]
    fn pop_bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut b = [0u8; N];
        for i in 0..N {
            b[N - i - 1] = self.data.pop().unwrap();
        }
        b
    }

    #[inline]
    fn pop_i32(&mut self) -> i32 {
        i32::from_le_bytes(self.pop_bytes())
    }

    #[inline]
    #[track_caller]
    pub fn pop_f64(&mut self) -> f64 {
        f64::from_le_bytes(self.pop_bytes())
    }

    fn slice_top(&self, n: usize) -> &'_ [u8] {
        &self.data[self.data.len() - n..]
    }
}

pub struct UntypedMemorySpan<'mem> {
    data: &'mem [u8],
}

impl<'mem> UntypedMemorySpan<'mem> {
    pub fn new(data: &'mem [u8]) -> Self {
        Self { data }
    }

    fn read_param_raw<const N: usize>(&self, func_signature: &FuncSignature, idx: usize) -> &[u8; N] {
        let offset: usize = func_signature.params.iter().take(idx).map(|t| t.len_bytes()).sum();
        self.data[offset..].first_chunk().unwrap()
    }

    fn push_into(&self, stack: &mut VmStack, local_idx: u8, sig: &FuncSignature) {
        match sig.params[local_idx as usize] {
            TypeKind::Func => {}
            TypeKind::F64 => stack.push_bytes(*self.read_param_raw::<8>(sig, local_idx as _)),
            TypeKind::I32 => {}
        }
    }
}

pub fn evaluate(ctx: &mut VmContext, func_body: &FuncBody, params: &UntypedMemorySpan, funcs: &[FuncBody], x: &mut impl Context) {
    let mut reader = Reader::new(func_body.code);
    loop {
        let pos = func_body.offset + reader.pos();
        let op = match reader.read_u8() {
            Ok(op) => op,
            Err(ParserError::EndOfStream { .. }) => break,
            Err(e) => panic!("other err: {e:?}"),
        };

        match op {
            0x04 => {
                // if
                let cond = match reader.read::<TypeKind>().unwrap() {
                    TypeKind::Func => todo!(),
                    TypeKind::F64 => {
                        let x = ctx.stack.pop_f64();
                        x != 0.0
                    }
                    TypeKind::I32 => todo!(),
                };

                if !cond {
                    reader.skip_to(func_body.jump_targets[&pos]);
                }
            }
            0x05 => {
                // else
                reader.skip_to(func_body.jump_targets[&pos] + 1);
            }
            0x0b => {
                // end
                // unimplemented!();
            }
            0x10 => {
                // call <func_idx>
                let func_idx = reader.read_usize().unwrap();
                let params: Vec<_> = funcs[func_idx].signature.params
                    .iter()
                    .flat_map(|t| {
                        match t {
                            TypeKind::Func => todo!(),
                            TypeKind::F64 => ctx.stack.pop_f64().to_le_bytes(),
                            TypeKind::I32 => todo!(),
                        }
                    }).collect();

                ctx.call_stack.push(StackFrame { func_idx });
                // TODO: get rid of recurrent calls in favour of a managed call stack
                evaluate(ctx, &funcs[func_idx], &UntypedMemorySpan {
                    data: &params,
                }, funcs, x);
            }
            0x20 => {
                // local.get <local>
                let local_idx = reader.read_u8().unwrap();
                params.push_into(&mut ctx.stack, local_idx, &func_body.signature);
            }
            0x44 => {
                // f64.const <literal>
                let val = reader.read_f64().unwrap();
                ctx.stack.push_f64(val);
            }
            0x63 => {
                // f64.lt
                let b = ctx.stack.pop_f64();
                let a = ctx.stack.pop_f64();
                ctx.stack.push_f64((a < b) as i32 as f64);
            }
            0x6a => {
                // i32.add
                let b = ctx.stack.pop_i32();
                let a = ctx.stack.pop_i32();
                ctx.stack.push_i32(a + b);
            }
            0xa1 => {
                // f64.sub
                let b = ctx.stack.pop_f64();
                let a = ctx.stack.pop_f64();
                ctx.stack.push_f64(a - b);
            }
            0xa2 => {
                // f64.mul
                let b = ctx.stack.pop_f64();
                let a = ctx.stack.pop_f64();
                ctx.stack.push_f64(a * b);
            }
            _ => unimplemented!("opcode {:02x?}", op),
        }
    }
}