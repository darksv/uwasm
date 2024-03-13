use crate::parser::{Reader, TypeKind};
use crate::{Context, FuncBody, FuncSignature, ParserError, WasmModule};
use alloc::vec::Vec;

pub struct VmContext<'code> {
    pub stack: VmStack,
    pub call_stack: Vec<StackFrame<'code>>,
}

impl VmContext<'_> {
    pub fn new() -> Self {
        Self {
            stack: VmStack { data: Vec::new() },
            call_stack: Vec::new(),
        }
    }
}

pub struct StackFrame<'code> {
    func_idx: usize,
    reader: Reader<'code>,
    params: Vec<u8>,
}

impl<'code> StackFrame<'code> {
    pub fn new(module: &'code WasmModule, idx: usize, params: Vec<u8>) -> Self {
        Self {
            func_idx: idx,
            reader: Reader::new(&module.functions[idx].code),
            params,
        }
    }
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
        assert!(self.data.len() >= N);
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

    fn read_param_raw<const N: usize>(
        &self,
        func_signature: &FuncSignature,
        idx: usize,
    ) -> &[u8; N] {
        let offset: usize = func_signature
            .params
            .iter()
            .take(idx)
            .map(|t| t.len_bytes())
            .sum();
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

pub fn evaluate<'code>(
    ctx: &mut VmContext<'code>,
    func_idx: usize,
    funcs: &[FuncBody<'code>],
    x: &mut impl Context,
) {
    while let Some(frame) = ctx.call_stack.last_mut() {
        let params = UntypedMemorySpan {
            data: &frame.params,
        };
        let current_func = &funcs[func_idx];
        let reader = &mut frame.reader;
        let pos = current_func.offset + reader.pos();

        let op = match reader.read_u8() {
            Ok(op) => op,
            Err(ParserError::EndOfStream { .. }) => {
                let _ = ctx.call_stack.pop();
                // don't care if this is the last call - it will be taken care of before next iteration
                continue;
            }
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
                    reader.skip_to(current_func.jump_targets[&pos]);
                }
            }
            0x05 => {
                // else
                reader.skip_to(current_func.jump_targets[&pos] + 1);
            }
            0x0b => {
                // end
                // unimplemented!();
            }
            0x10 => {
                // call <func_idx>
                let func_idx = reader.read_usize().unwrap();
                let params: Vec<_> = current_func
                    .signature
                    .params
                    .iter()
                    .flat_map(|t| match t {
                        TypeKind::Func => todo!(),
                        TypeKind::F64 => ctx.stack.pop_f64().to_le_bytes(),
                        TypeKind::I32 => todo!(),
                    })
                    .collect();

                ctx.call_stack.push(StackFrame {
                    func_idx,
                    reader: Reader::new(&current_func.code),
                    params,
                });
            }
            0x20 => {
                // local.get <local>
                let local_idx = reader.read_u8().unwrap();
                params.push_into(&mut ctx.stack, local_idx, &current_func.signature);
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
