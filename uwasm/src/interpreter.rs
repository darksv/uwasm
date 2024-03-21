use alloc::fmt;
use crate::parser::{Reader, TypeKind};
use crate::{Context, FuncSignature, ParserError, WasmModule};
use alloc::vec::Vec;
use core::fmt::Formatter;

pub struct VmContext<'code> {
    pub stack: VmStack,
    call_stack: Vec<StackFrame<'code>>,
    // temporary store for locals - TODO: maybe reuse values from the stack
    locals: Vec<u8>,
}

impl VmContext<'_> {
    pub fn new() -> Self {
        Self {
            stack: VmStack::new(),
            call_stack: Vec::new(),
            locals: Vec::new(),
        }
    }
}

pub struct StackFrame<'code> {
    func_idx: usize,
    reader: Reader<'code>,
    locals_offset: usize,
}

impl<'code> StackFrame<'code> {
    pub fn new(module: &'code WasmModule, idx: usize, locals_offset: usize) -> Self {
        Self {
            func_idx: idx,
            reader: Reader::new(&module.functions[idx].code),
            locals_offset,
        }
    }
}

pub struct VmStack {
    data: Vec<u8>,
    #[cfg(debug_assertions)]
    types: Vec<TypeKind>,
}

impl VmStack {
    #[inline]
    fn new() -> Self {
        Self {
            data: Vec::new(),
            #[cfg(debug_assertions)]
            types: Vec::new(),
        }
    }

    #[inline]
    pub(self) fn push_bytes<const N: usize>(&mut self, ty: TypeKind, data: [u8; N]) {
        self.data.extend(data);
        #[cfg(debug_assertions)]
        self.types.push(ty);
        #[cfg(not(debug_assertions))]
        let _ = ty;
    }

    #[inline]
    fn push_f64(&mut self, val: f64) {
        self.push_bytes(TypeKind::F64, val.to_le_bytes());
    }

    #[inline]
    fn push_i32(&mut self, val: i32) {
        self.push_bytes(TypeKind::I32, val.to_le_bytes());
    }

    fn pop_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (rest, &bytes) = self.data.split_last_chunk::<N>()?;
        self.data.drain(rest.len()..);
        Some(bytes)
    }

    #[inline]
    pub fn pop_i32(&mut self) -> Option<i32> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(i32::from_le_bytes)
    }

    #[inline]
    pub fn pop_f32(&mut self) -> Option<f32> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(f32::from_le_bytes)
    }

    #[inline]
    pub fn pop_f64(&mut self) -> Option<f64> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(f64::from_le_bytes)
    }

    #[inline]
    fn pop_many(&mut self, n_bytes: usize) {
        #[cfg(debug_assertions)]
        {
            let mut remaining_bytes = n_bytes;
            while remaining_bytes > 0 {
                let ty = self.types.pop().expect("enough types");
                remaining_bytes -= ty.len_bytes();
            }
        }
        self.data.drain(self.data.len() - n_bytes..);
    }
}

impl fmt::Debug for VmStack {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        {
            let mut reader = Reader::new(&self.data);
            let mut fmt = f.debug_list();
            for tk in &self.types {
                match tk {
                    TypeKind::Void => todo!(),
                    TypeKind::Func => todo!(),
                    TypeKind::FuncRef => todo!(),
                    TypeKind::F32 => {
                        fmt.entry(&reader.read_f32().unwrap());
                    }
                    TypeKind::F64 => {
                        fmt.entry(&reader.read_f64().unwrap());
                    }
                    TypeKind::I32 => {
                        fmt.entry(&reader.read_u32().unwrap());
                    }
                    TypeKind::I64 => {
                        fmt.entry(&reader.read_u64().unwrap());
                    }
                }
            }
            fmt.finish()?;
        }
        #[cfg(not(debug_assertions))]
        write!(f, "{:02X?}", &self.data)?;
        Ok(())
    }
}

pub struct UntypedMemorySpan<'mem> {
    data: &'mem [u8],
}

impl<'mem> UntypedMemorySpan<'mem> {
    pub fn new(data: &'mem [u8]) -> Self {
        Self { data }
    }

    #[inline]
    fn read_param_raw<const N: usize>(
        &self,
        func_signature: &FuncSignature,
        idx: usize,
    ) -> Option<&[u8; N]> {
        let offset = func_signature.param_offsets.get(idx).copied()?;
        self.data.get(offset..)?.first_chunk()
    }

    #[inline]
    fn push_into(&self, stack: &mut VmStack, local_idx: usize, sig: &FuncSignature) {
        match sig.params[local_idx] {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 => stack.push_bytes(TypeKind::F32, *self.read_param_raw::<4>(sig, local_idx).unwrap()),
            TypeKind::F64 => stack.push_bytes(TypeKind::F64, *self.read_param_raw::<8>(sig, local_idx).unwrap()),
            TypeKind::I32 => stack.push_bytes(TypeKind::I32, *self.read_param_raw::<4>(sig, local_idx).unwrap()),
            TypeKind::I64 => stack.push_bytes(TypeKind::I64, *self.read_param_raw::<8>(sig, local_idx).unwrap()),
        }
    }
}

pub fn evaluate<'code>(
    ctx: &mut VmContext<'code>,
    module: &'code WasmModule<'code>,
    func_idx: usize,
    args: &[u8],
    #[allow(unused)]
    x: &mut impl Context,
) {
    ctx.stack.data.clear();
    ctx.locals.extend(args);
    ctx.call_stack.clear();
    ctx.call_stack.push(StackFrame::new(
        &module,
        func_idx,
        0,
    ));

    while let Some(frame) = ctx.call_stack.last_mut() {
        let current_func = &module.functions[frame.func_idx];
        let reader = &mut frame.reader;
        let pos = current_func.offset + reader.pos();

        let op = match reader.read_u8() {
            Ok(op) => op,
            Err(ParserError::EndOfStream { .. }) => {
                if let Some(frame) = ctx.call_stack.pop() {
                    ctx.locals.drain(frame.locals_offset..);
                }
                // don't care if this is the last call - it will be taken care of before next iteration
                continue;
            }
            Err(e) => panic!("other err: {e:?}"),
        };

        //writeln!(x, "{:02x?} @ {pos:02X} ({func_idx}) :: {:?}", op, &ctx.stack);

        match op {
            0x04 => {
                // if
                let cond = match reader.read::<TypeKind>().unwrap() {
                    TypeKind::Void => todo!(),
                    TypeKind::Func => todo!(),
                    TypeKind::FuncRef => todo!(),
                    TypeKind::F32 => {
                        let x = ctx.stack.pop_f32().unwrap();
                        x != 0.0
                    }
                    TypeKind::F64 => {
                        let x = ctx.stack.pop_f64().unwrap();
                        x != 0.0
                    },
                    TypeKind::I32 => todo!(),
                    TypeKind::I64 => todo!(),
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
                continue;
            }
            0x10 => {
                // call <func_idx>
                let func_idx = reader.read_usize().unwrap();
                let len_locals = current_func
                    .signature
                    .params
                    .iter()
                    .map(|t| t.len_bytes())
                    .sum();

                ctx.call_stack.push(StackFrame {
                    func_idx,
                    reader: Reader::new(&module.functions[func_idx].code),
                    locals_offset: ctx.stack.data.len() - len_locals,
                });
                ctx.locals.extend(&ctx.stack.data[ctx.stack.data.len() - len_locals..]);
                ctx.stack.pop_many(len_locals);
            }
            0x20 => {
                // local.get <local>
                let params = UntypedMemorySpan {
                    data: &ctx.locals[frame.locals_offset..],
                };

                let local_idx = reader.read_u8().unwrap();
                params.push_into(&mut ctx.stack, local_idx as usize, &current_func.signature);
            }
            0x44 => {
                // f64.const <literal>
                let val = reader.read_f64().unwrap();
                ctx.stack.push_f64(val);
            }
            0x63 => {
                // f64.lt
                let b = ctx.stack.pop_f64().unwrap();
                let a = ctx.stack.pop_f64().unwrap();
                ctx.stack.push_f64((a < b) as i32 as f64);
            }
            0x6a => {
                // i32.add
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a + b);
            }
            0x6b => {
                // i32.sub
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a - b);
            }
            0xa1 => {
                // f64.sub
                let b = ctx.stack.pop_f64().unwrap();
                let a = ctx.stack.pop_f64().unwrap();
                ctx.stack.push_f64(a - b);
            }
            0xa2 => {
                // f64.mul
                let b = ctx.stack.pop_f64().unwrap();
                let a = ctx.stack.pop_f64().unwrap();
                ctx.stack.push_f64(a * b);
            }
            _ => todo!("opcode {:02x?}", op),
        }
    }
}
