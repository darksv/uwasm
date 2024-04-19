use alloc::fmt;
use crate::parser::{Reader, TypeKind};
use crate::{Context, FuncBody, parse_opcode, ParserError, ParserState, WasmModule};
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

enum BlockType {
    Block,
    Loop,
}

pub struct StackFrame<'code> {
    func_idx: usize,
    reader: Reader<'code>,
    locals_offset: usize,
    curr_loop_start: Option<usize>,
    blocks: Vec<(usize, BlockType)>,
}

impl<'code> StackFrame<'code> {
    pub fn new(module: &'code WasmModule, idx: usize, locals_offset: usize) -> Self {
        Self {
            func_idx: idx,
            reader: Reader::new(module.functions[idx].code),
            locals_offset,
            curr_loop_start: None,
            blocks: Vec::new(),
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

    #[inline]
    fn push_i64(&mut self, val: i64) {
        self.push_bytes(TypeKind::I64, val.to_le_bytes());
    }

    fn pop_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (rest, &bytes) = self.data.split_last_chunk::<N>()?;
        self.data.drain(rest.len()..);
        Some(bytes)
    }

    pub fn peek_bytes<const N: usize>(&self) -> Option<[u8; N]> {
        let (rest, &bytes) = self.data.split_last_chunk::<N>()?;
        self.data[rest.len()..].first_chunk().copied()
    }

    #[inline]
    pub fn pop_i32(&mut self) -> Option<i32> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(i32::from_le_bytes)
    }

    #[inline]
    pub fn pop_i64(&mut self) -> Option<i64> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(i64::from_le_bytes)
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
                        fmt.entry(&reader.read_i32().unwrap());
                    }
                    TypeKind::I64 => {
                        fmt.entry(&reader.read_i64().unwrap());
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

#[repr(transparent)]
pub struct UntypedMemorySpan {
    data: [u8],
}

impl UntypedMemorySpan {
    pub fn from_slice(data: &[u8]) -> &Self {
        unsafe { core::mem::transmute(data) }
    }

    pub fn from_slice_mut(data: &mut [u8]) -> &mut Self {
        unsafe { core::mem::transmute(data) }
    }

    #[inline]
    fn read_param_raw<const N: usize>(
        &self,
        func: &FuncBody,
        idx: usize,
    ) -> Option<&[u8; N]> {
        let offset = func.locals_offsets.get(idx).copied()?;
        self.data.get(offset..)?.first_chunk()
    }

    #[inline]
    fn write_param_raw<const N: usize>(
        &mut self,
        func: &FuncBody,
        idx: usize,
        data: [u8; N],
    ) -> Option<()> {
        // FIXME
        let offset = func.locals_offsets.get(idx).copied()?;
        self.data.get_mut(offset..)?
            .first_chunk_mut::<N>()?
            .copy_from_slice(&data);
        Some(())
    }

    #[inline]
    fn push_into(&self, stack: &mut VmStack, local_idx: usize, func: &FuncBody) {
        match func.locals_types[local_idx] {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 => stack.push_bytes(TypeKind::F32, *self.read_param_raw::<4>(&func, local_idx).unwrap()),
            TypeKind::F64 => stack.push_bytes(TypeKind::F64, *self.read_param_raw::<8>(&func, local_idx).unwrap()),
            TypeKind::I32 => stack.push_bytes(TypeKind::I32, *self.read_param_raw::<4>(&func, local_idx).unwrap()),
            TypeKind::I64 => stack.push_bytes(TypeKind::I64, *self.read_param_raw::<8>(&func, local_idx).unwrap()),
        }
    }

    #[inline]
    fn pop_from(&mut self, stack: &mut VmStack, local_idx: usize, func: &FuncBody) {
        match func.locals_types[local_idx] {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 => self.write_param_raw::<4>(&func, local_idx, stack.pop_f32().unwrap().to_ne_bytes()).unwrap(),
            TypeKind::F64 => self.write_param_raw::<8>(&func, local_idx, stack.pop_f64().unwrap().to_ne_bytes()).unwrap(),
            TypeKind::I32 => self.write_param_raw::<4>(&func, local_idx, stack.pop_i32().unwrap().to_ne_bytes()).unwrap(),
            TypeKind::I64 => self.write_param_raw::<8>(&func, local_idx, stack.pop_i64().unwrap().to_ne_bytes()).unwrap(),
        }
    }

    #[inline]
    fn copy_from(&mut self, stack: &mut VmStack, local_idx: usize, func: &FuncBody) {
        match func.locals_types[local_idx] {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 | TypeKind::I32 => self.write_param_raw::<4>(&func, local_idx, stack.peek_bytes().unwrap()).unwrap(),
            TypeKind::F64 | TypeKind::I64 => self.write_param_raw::<8>(&func, local_idx, stack.peek_bytes().unwrap()).unwrap(),
        }
    }
}

fn copy_locals(locals: &mut Vec<u8>, params_data: &[u8], func_body: &FuncBody) {
    let non_params_locals_bytes: usize = func_body.non_param_locals().map(|ty| ty.len_bytes()).sum();
    locals.extend_from_slice(params_data);
    locals.resize(locals.len() + non_params_locals_bytes, 0);
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
    copy_locals(&mut ctx.locals, args, &module.functions[func_idx]);
    ctx.call_stack.clear();
    ctx.call_stack.push(StackFrame::new(
        module,
        func_idx,
        0,
    ));

    while let Some(frame) = ctx.call_stack.last_mut() {
        let current_func = &module.functions[frame.func_idx];
        let reader = &mut frame.reader;
        let pos = current_func.offset + reader.pos();

        let opcode_reader = reader.clone();
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

        #[cfg(debug_assertions)]
        {
            let mut reader = opcode_reader;
            let pos = reader.pos();
            write!(x, "{:02x?} @ {pos:02X} ({func_idx}) :: {:?} :: ", op, &ctx.stack);
            _ = parse_opcode::<true>(&mut reader, pos, x, &mut ParserState::default());
            drop(reader);
        }

        match op {
            0x00 => {
                writeln!(x, "entered unreachable");
                break;
            }
            0x02 => {
                // block
                let ty = reader.read_usize().unwrap();
                frame.curr_loop_start = Some(pos);
                frame.blocks.push((reader.pos(), BlockType::Block));
            }
            0x03 => {
                // loop
                let ty = reader.read_usize().unwrap();
                frame.curr_loop_start = Some(pos);
                frame.blocks.push((reader.pos(), BlockType::Loop));
            }
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
                    }
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
                if let Some((start, BlockType::Loop)) = frame.blocks.last() {
                    reader.skip_to(*start);
                } else {
                    frame.blocks.pop();
                }
                continue;
            }
            0x0c => {
                // br
                let depth = reader.read_usize().unwrap();
                reader.skip_to(frame.blocks[depth].0);
                writeln!(x, "taken");
            }
            0x0d => {
                // br_if
                let depth = reader.read_usize().unwrap();
                if ctx.stack.pop_i32().unwrap() == 1 {
                    reader.skip_to(frame.blocks[depth].0);
                    writeln!(x, "taken");
                } else {
                    writeln!(x, "not taken");
                }
            }
            0x0f => {
                // return
                reader.skip_to_end();
            }
            0x10 => {
                // call <func_idx>
                let func_idx = reader.read_usize().unwrap();
                writeln!(x, "calling {}", func_idx);
                let len_locals = current_func
                    .signature
                    .params
                    .iter()
                    .map(|t| t.len_bytes())
                    .sum();

                ctx.call_stack.push(StackFrame {
                    func_idx,
                    reader: Reader::new(module.functions[func_idx].code),
                    locals_offset: ctx.stack.data.len() - len_locals,
                    curr_loop_start: None,
                    blocks: Vec::new(),
                });
                let params_mem = &ctx.stack.data[ctx.stack.data.len() - len_locals..];
                copy_locals(&mut ctx.locals, params_mem, current_func);
                ctx.stack.pop_many(len_locals);
            }
            0x20 => {
                // local.get <local>
                let locals = UntypedMemorySpan::from_slice(
                    &ctx.locals[frame.locals_offset..]
                );

                let local_idx = reader.read_u8().unwrap();
                locals.push_into(&mut ctx.stack, local_idx as usize, &current_func);
            }
            0x21 => {
                // local.set <local>
                let local_idx = reader.read_u8().unwrap();
                writeln!(x, "11 {:?}", &ctx.locals[frame.locals_offset..]);
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).pop_from(&mut ctx.stack, local_idx as usize, &current_func);

                writeln!(x, "22 {:?}", &ctx.locals[frame.locals_offset..]);
            }
            0x22 => {
                // local.tee <local>
                let local_idx = reader.read_u8().unwrap();
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).copy_from(&mut ctx.stack, local_idx as usize, &current_func);
            }
            0x41 => {
                // i32.const <literal>
                let val = reader.read_isize().unwrap();
                ctx.stack.push_i32(i32::try_from(val).unwrap());
            }
            0x42 => {
                // i64.const <literal>
                let val = reader.read_usize().unwrap();
                ctx.stack.push_i64(i64::try_from(val).unwrap());
            }
            0x44 => {
                // f64.const <literal>
                let val = reader.read_f64().unwrap();
                ctx.stack.push_f64(val);
            }
            0x45 => {
                // i32.eqz
                let val = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32((val == 0) as i32);
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
            0x6c => {
                // i32.mul
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a.wrapping_mul(b));
            }
            0x6d => {
                // i32.div_s
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a / b);
            }
            0x71 => {
                // i32.and
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a & b);
            }
            0x72 => {
                // i32.or
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a | b);
            }
            0x73 => {
                // i32.xor
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a ^ b);
            }
            0x74 => {
                // i32.shl
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a << b);
            }
            0x76 => {
                // i32.shr_u
                let b = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i32(a >> b);
            }
            0x7e => {
                // i64.mul
                let b = ctx.stack.pop_i64().unwrap();
                let a = ctx.stack.pop_i64().unwrap();
                ctx.stack.push_i64(a * b);
            }
            0x88 => {
                // i64.shr_u
                let b = ctx.stack.pop_i64().unwrap();
                let a = ctx.stack.pop_i64().unwrap();
                ctx.stack.push_i64(a >> b);
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
            0xa7 => {
                // i32.wrap_i64
                let a = ctx.stack.pop_i64().unwrap();
                ctx.stack.push_i32(i32::try_from(a & 0xffffffff).unwrap());
            }
            0xad => {
                // i64.extend_i32_u
                let a = ctx.stack.pop_i32().unwrap();
                ctx.stack.push_i64(i64::from(a));
            }
            _ => todo!("opcode {:02x?}", op),
        }
    }
}
