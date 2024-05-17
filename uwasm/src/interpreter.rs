use alloc::fmt;
use alloc::vec::Vec;
use core::fmt::Formatter;
use core::iter;

use crate::{ByteStr, Context, FuncBody, ParserError, WasmModule};
#[cfg(debug_assertions)]
use crate::{parse_opcode, ParserState};
use crate::operand::{EvaluationError, Operand};
use crate::parser::{Reader, TypeKind};

pub struct VmContext<'code> {
    pub stack: VmStack,
    call_stack: Vec<StackFrame<'code>>,
    // temporary store for locals - TODO: maybe reuse values from the stack
    locals: Vec<u8>,
    profile: ExecutionProfile,
}

impl VmContext<'_> {
    pub fn new() -> Self {
        Self {
            stack: VmStack::new(),
            call_stack: Vec::new(),
            locals: Vec::new(),
            profile: ExecutionProfile::new(),
        }
    }

    pub fn reset_profile(&mut self) {
        self.profile = ExecutionProfile::new();
    }

    pub fn profile(&self) -> &ExecutionProfile {
        &self.profile
    }
}

pub struct ExecutionProfile {
    executed_instr_count: [u32; 0xFF],
    executed_instr_time: [u64; 0xFF],
}

impl ExecutionProfile {
    fn new() -> Self {
        Self {
            executed_instr_count: core::array::from_fn(|_| 0),
            executed_instr_time: core::array::from_fn(|_| 0),
        }
    }
}

impl fmt::Debug for ExecutionProfile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let total_count = self.executed_instr_count.iter().sum::<u32>() as u64;
        let total_time = self.executed_instr_time.iter().sum::<u64>() as u64;
        for (instr, &count) in self.executed_instr_count.iter().enumerate() {
            let time = self.executed_instr_time[instr];
            if count > 0 {
                writeln!(f, "{:02X} {:>12} ({:>6.02}%) | {:>12} ({:>6.02}%)",
                         instr, count, (count as f32) * 100.0 / (total_count as f32),
                         time,
                         (time as f32) * 100.0 / (total_time as f32)
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
enum BlockType {
    Block,
    Loop,
}

pub struct StackFrame<'code> {
    func_idx: usize,
    reader: Reader<'code>,
    locals_offset: usize,
    curr_loop_start: Option<usize>,
    blocks: Vec<BlockMeta>,
}

impl<'code> StackFrame<'code> {
    pub fn new(module: &'code WasmModule, idx: usize, locals_offset: usize) -> Self {
        Self {
            func_idx: idx,
            reader: Reader::new(module.get_function_by_index(idx).unwrap().code),
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
    pub fn push_f32(&mut self, val: f32) {
        self.push_bytes(TypeKind::F32, val.to_le_bytes());
    }

    #[inline]
    pub fn push_f64(&mut self, val: f64) {
        self.push_bytes(TypeKind::F64, val.to_le_bytes());
    }

    #[inline]
    pub fn push_i32(&mut self, val: i32) {
        self.push_bytes(TypeKind::I32, val.to_le_bytes());
    }

    #[inline]
    pub fn push_i64(&mut self, val: i64) {
        self.push_bytes(TypeKind::I64, val.to_le_bytes());
    }

    fn pop_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (rest, &bytes) = self.data.split_last_chunk::<N>()?;
        self.data.drain(rest.len()..);
        Some(bytes)
    }

    pub fn peek_bytes<const N: usize>(&self) -> Option<[u8; N]> {
        self.data.split_last_chunk::<N>().map(|(_, bytes)| *bytes)
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
    pub fn pop_u32(&mut self) -> Option<u32> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(u32::from_le_bytes)
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

    #[inline]
    #[track_caller]
    fn inplace_bin_op<T: Operand, U: Operand, R: Operand>(&mut self, op: impl FnOnce(T, U) -> R) -> Result<R, EvaluationError> {
        let b = U::pop(self)?;
        let a = T::pop(self)?;
        let result = op(a, b);
        R::push(self, result);
        Ok(result)
    }

    #[inline]
    #[track_caller]
    fn inplace_unary_op<T: Operand, R: Operand>(&mut self, op: impl FnOnce(T) -> R) -> Result<R, EvaluationError> {
        let a = T::pop(self)?;
        let result = op(a);
        R::push(self, result);
        Ok(result)
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

#[derive(Debug)]
struct BlockMeta {
    offset: usize,
    body_offset: usize,
    kind: BlockType,
}

#[repr(transparent)]
pub struct Memory {
    data: [u8],
}

impl Memory {
    pub fn from_slice(data: &[u8]) -> &Self {
        unsafe { core::mem::transmute(data) }
    }

    #[allow(unused)]
    pub fn from_slice_mut(data: &mut [u8]) -> &mut Self {
        unsafe { core::mem::transmute(data) }
    }

    #[inline]
    fn read_bytes_at<const N: usize>(&self, offset: usize) -> Option<[u8; N]> {
        let (_, data) = self.data.split_at_checked(offset)?;
        let (&raw, _) = data.split_first_chunk()?;
        Some(raw)
    }

    fn read_i8(&self, offset: usize) -> Option<i8> {
        self.read_bytes_at(offset).map(i8::from_ne_bytes)
    }

    fn read_u8(&self, offset: usize) -> Option<u8> {
        self.read_bytes_at(offset).map(u8::from_ne_bytes)
    }

    fn read_i16(&self, offset: usize) -> Option<i16> {
        self.read_bytes_at(offset).map(i16::from_ne_bytes)
    }

    fn read_u16(&self, offset: usize) -> Option<u16> {
        self.read_bytes_at(offset).map(u16::from_ne_bytes)
    }

    fn read_i32(&self, offset: usize) -> Option<i32> {
        self.read_bytes_at(offset).map(i32::from_ne_bytes)
    }

    fn read_u32(&self, offset: usize) -> Option<u32> {
        self.read_bytes_at(offset).map(u32::from_ne_bytes)
    }

    fn read_i64(&self, offset: usize) -> Option<i64> {
        self.read_bytes_at(offset).map(i64::from_ne_bytes)
    }

    fn read_f32(&self, offset: usize) -> Option<f32> {
        self.read_bytes_at(offset).map(f32::from_ne_bytes)
    }

    fn read_f64(&self, offset: usize) -> Option<f64> {
        self.read_bytes_at(offset).map(f64::from_ne_bytes)
    }
}

pub struct Serializer {
    buf: Vec<u8>,
}

impl Serializer {
    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }
}

pub trait FunctionArgs {
    const TYPE: &'static [TypeKind];

    fn write_to(&self, serializer: &mut Serializer);
}

macro_rules! tuple_impls {
    ( $( $name:ident )+ ) => {
        impl<$($name: Operand),+> FunctionArgs for ($($name,)+) {
            const TYPE: &'static [TypeKind] = &[$($name::TYPE),+];
            #[allow(nonstandard_style)]
            fn write_to(&self, serializer: &mut Serializer) {
                let (
                    $($name,)+
                ) = self;
                $(
                    <$name as Operand>::write_to($name, serializer);
                )+
            }
        }
    };
}

tuple_impls! { A }
tuple_impls! { A B }
tuple_impls! { A B C }
tuple_impls! { A B C D }
tuple_impls! { A B C D E }
tuple_impls! { A B C D E F }
tuple_impls! { A B C D E F G }
tuple_impls! { A B C D E F G H }
tuple_impls! { A B C D E F G H I }
tuple_impls! { A B C D E F G H I J }
tuple_impls! { A B C D E F G H I J K }
tuple_impls! { A B C D E F G H I J K L }

// variadic tuples... 🙏

#[derive(Debug)]
pub enum ExecutionError {
    FunctionNotExists,
    InvalidSignature,
    EmptyStack,
    EvaluationError(EvaluationError),
    MissingFunctionBody,
}

pub type ImportedFunc = for<'f> fn(&'f mut VmStack);

pub fn execute_function<'code, TArgs: FunctionArgs, TResult: Operand>(
    ctx: &mut VmContext<'code>,
    module: &'code WasmModule<'code>,
    func_name: &ByteStr,
    args: TArgs,
    memory: &[u8],
    imports: &[ImportedFunc],
    execution_ctx: &mut impl Context,
) -> Result<TResult, ExecutionError> {
    let Some(func_idx) = module.get_function_index_by_name(func_name) else {
        return Err(ExecutionError::FunctionNotExists);
    };

    let Some(func) = &module.functions[func_idx].body else {
        return Err(ExecutionError::MissingFunctionBody);
    };

    if func.signature.params.len() != TArgs::TYPE.len() {
        return Err(ExecutionError::InvalidSignature);
    }

    for (expected, actual) in iter::zip(&func.signature.params, TArgs::TYPE) {
        if expected != actual {
            return Err(ExecutionError::InvalidSignature);
        }
    }

    // TODO: check result types

    let mut args_mem = Serializer {
        buf: Vec::new(),
    };
    args.write_to(&mut args_mem);
    evaluate(ctx, module, func_idx, &args_mem.buf, memory, imports, execution_ctx);
    TResult::pop(&mut ctx.stack).map_err(ExecutionError::EvaluationError)
}

pub fn evaluate<'code>(
    ctx: &mut VmContext<'code>,
    module: &'code WasmModule<'code>,
    func_idx: usize,
    args: &[u8],
    memory: &[u8],
    imports: &[ImportedFunc],
    #[allow(unused)]
    x: &mut impl Context,
) {
    ctx.stack.data.clear();
    copy_locals(&mut ctx.locals, args, module.get_function_by_index(func_idx).as_ref().unwrap());
    ctx.call_stack.clear();
    ctx.call_stack.push(StackFrame::new(
        module,
        func_idx,
        0,
    ));

    while let Some(frame) = ctx.call_stack.last_mut() {
        let current_func = module.get_function_by_index(frame.func_idx).unwrap();
        let reader = &mut frame.reader;
        let pos = current_func.offset + reader.pos();

        #[cfg(debug_assertions)]
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

        ctx.profile.executed_instr_count[op as usize] += 1;

        let start = x.ticks();
        match op {
            0x00 => {
                writeln!(x, "entered unreachable");
                break;
            }
            0x02 => {
                // block
                let _ty = reader.read_usize().unwrap();
                frame.curr_loop_start = Some(pos);
                frame.blocks.push(BlockMeta {
                    offset: pos,
                    body_offset: reader.pos(),
                    kind: BlockType::Block,
                });
            }
            0x03 => {
                // loop
                let _ty = reader.read_usize().unwrap();
                frame.curr_loop_start = Some(pos);
                frame.blocks.push(BlockMeta {
                    offset: pos,
                    body_offset: reader.pos(),
                    kind: BlockType::Loop,
                });
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
                    TypeKind::I32 => {
                        let x = ctx.stack.pop_i32().unwrap();
                        x != 0
                    }
                    TypeKind::I64 => {
                        let x = ctx.stack.pop_i64().unwrap();
                        x != 0
                    }
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
                if let Some(block) = frame.blocks.pop() {
                    #[cfg(debug_assertions)]
                    writeln!(x, "end {:?}", block.kind);
                    _ = block;
                } else {
                    #[cfg(debug_assertions)]
                    writeln!(x, "exit function");
                }
                continue;
            }
            0x0c => {
                // br
                let depth = reader.read_usize().unwrap();
                let block_idx = frame.blocks.len() - 1 - depth;
                reader.skip_to(current_func.jump_targets[&frame.blocks[block_idx].offset] - 1);
                // skip blocks that we are no longer executing due to the jump
                // TODO: check if this is correct
                frame.blocks.drain(block_idx + 1..);
                #[cfg(debug_assertions)]
                writeln!(x, "taken");
            }
            0x0d => {
                // br_if
                let depth = reader.read_usize().unwrap();
                let block_idx = frame.blocks.len() - 1 - depth;
                let block = &frame.blocks[block_idx];
                if ctx.stack.pop_i32().unwrap() != 0 {
                    let target = match block.kind {
                        BlockType::Block => current_func.jump_targets[&block.offset] - 1,
                        BlockType::Loop => block.body_offset
                    };
                    reader.skip_to(target);
                    // skip blocks that we are no longer executing due to the jump
                    // TODO: check if this is correct
                    frame.blocks.drain(block_idx + 1..);
                    #[cfg(debug_assertions)]
                    writeln!(x, "taken");
                } else {
                    #[cfg(debug_assertions)]
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
                #[cfg(debug_assertions)]
                writeln!(x, "calling {}", func_idx);

                if let Some(func) = module.get_function_by_index(func_idx) {
                    ctx.call_stack.push(StackFrame {
                        func_idx,
                        reader: Reader::new(func.code),
                        locals_offset: ctx.locals.len(),
                        curr_loop_start: None,
                        blocks: Vec::new(),
                    });
                    let params_mem = &ctx.stack.data[ctx.stack.data.len() - current_func.params_len_in_bytes..];
                    copy_locals(&mut ctx.locals, params_mem, current_func);
                    ctx.stack.pop_many(current_func.params_len_in_bytes);
                } else {
                    #[cfg(debug_assertions)]
                    writeln!(x, "calling imported function {}", func_idx);
                    imports[func_idx](&mut ctx.stack);
                }
            }
            0x1b => {
                // select
                let cond = ctx.stack.pop_i32().unwrap();
                let a = ctx.stack.pop_i32().unwrap();
                let b = ctx.stack.pop_i32().unwrap();
                let res = match cond {
                    0 => a,
                    _ => b,
                };
                ctx.stack.push_i32(res);
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
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).pop_from(&mut ctx.stack, local_idx as usize, &current_func);
            }
            0x22 => {
                // local.tee <local>
                let local_idx = reader.read_u8().unwrap();
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).copy_from(&mut ctx.stack, local_idx as usize, &current_func);
            }
            0x28..=0x35 => {
                // i32.load     0x28
                // i64.load     0x29
                // f32.load     0x2a
                // f64.load     0x2b
                // i32.load8_s  0x2c
                // i32.load8_u  0x2d
                // i32.load16_s 0x2e
                // i32.load16_u 0x2f
                // i64.load8_s 	0x30
                // i64.load8_u 	0x31
                // i64.load16_s 0x32
                // i64.load16_u 0x33
                // i64.load32_s 0x34
                // i64.load32_u 0x35
                let idx = ctx.stack.pop_u32().unwrap() as usize;
                let _align = reader.read_usize().unwrap();
                let offset = reader.read_usize().unwrap() + idx;
                let mem = Memory::from_slice(memory);
                match op {
                    0x28 => ctx.stack.push_i32(mem.read_i32(offset).unwrap()),
                    0x29 => ctx.stack.push_i64(mem.read_i64(offset).unwrap()),
                    0x2a => ctx.stack.push_f32(mem.read_f32(offset).unwrap()),
                    0x2b => ctx.stack.push_f64(mem.read_f64(offset).unwrap()),
                    0x2c => ctx.stack.push_i32(mem.read_i8(offset).unwrap() as i32),
                    0x2d => ctx.stack.push_i32(mem.read_u8(offset).unwrap() as i32),
                    0x2e => ctx.stack.push_i32(mem.read_i16(offset).unwrap() as i32),
                    0x2f => ctx.stack.push_i32(mem.read_u16(offset).unwrap() as i32),
                    0x30 => ctx.stack.push_i64(mem.read_i8(offset).unwrap() as i64),
                    0x31 => ctx.stack.push_i64(mem.read_u8(offset).unwrap() as i64),
                    0x32 => ctx.stack.push_i64(mem.read_i16(offset).unwrap() as i64),
                    0x33 => ctx.stack.push_i64(mem.read_u16(offset).unwrap() as i64),
                    0x34 => ctx.stack.push_i64(mem.read_i32(offset).unwrap() as i64),
                    0x35 => ctx.stack.push_i64(mem.read_u32(offset).unwrap() as i64),
                    _ => unreachable!(),
                }
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
            0x43 => {
                // f32.const <literal>
                let val = reader.read_f32().unwrap();
                ctx.stack.push_f32(val);
            }
            0x44 => {
                // f64.const <literal>
                let val = reader.read_f64().unwrap();
                ctx.stack.push_f64(val);
            }
            0x45 => {
                // i32.eqz
                ctx.stack.inplace_unary_op(|a: i32| a == 0).unwrap();
            }
            0x46 => {
                // i32.eq
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a == b).unwrap();
            }
            0x47 => {
                // i32.ne
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a != b).unwrap();
            }
            0x48 => {
                // i32.lt_s
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a < b).unwrap();
            }
            0x49 => {
                // i32.lt_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a < b).unwrap();
            }
            0x4a => {
                // i32.le_s
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a <= b).unwrap();
            }
            0x4b => {
                // i32.gt_s
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a > b).unwrap();
            }
            0x4c => {
                // i32.gt_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a > b).unwrap();
            }
            0x4d => {
                // i32.le_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a <= b).unwrap();
            }
            0x4e => {
                // i32.ge_s
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a >= b).unwrap();
            }
            0x4f => {
                // i32.ge_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a >= b).unwrap();
            }
            0x63 => {
                // f64.lt
                ctx.stack.inplace_bin_op(|a: f64, b: f64| (a < b) as i32 as f64).unwrap();
            }
            0x6a => {
                // i32.add
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a + b).unwrap();
            }
            0x6b => {
                // i32.sub
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a - b).unwrap();
            }
            0x6c => {
                // i32.mul
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a.wrapping_mul(b)).unwrap();
            }
            0x6d => {
                // i32.div_s
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a / b).unwrap();
            }
            0x71 => {
                // i32.and
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a & b).unwrap();
            }
            0x72 => {
                // i32.or
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a | b).unwrap();
            }
            0x73 => {
                // i32.xor
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a ^ b).unwrap();
            }
            0x74 => {
                // i32.shl
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a << b).unwrap();
            }
            0x76 => {
                // i32.shr_u
                ctx.stack.inplace_bin_op(|a: i32, b: i32| a >> b).unwrap();
            }
            0x7e => {
                // i64.mul
                ctx.stack.inplace_bin_op(|a: i64, b: i64| a * b).unwrap();
            }
            0x88 => {
                // i64.shr_u
                ctx.stack.inplace_bin_op(|a: u64, b: u64| a >> b).unwrap();
            }
            0x92 => {
                // f32.add
                ctx.stack.inplace_bin_op(|a: f32, b: f32| a + b).unwrap();
            }
            0xa1 => {
                // f64.sub
                ctx.stack.inplace_bin_op(|a: f64, b: f64| a - b).unwrap();
            }
            0xa2 => {
                // f64.mul
                ctx.stack.inplace_bin_op(|a: f64, b: f64| a * b).unwrap();
            }
            0xa7 => {
                // i32.wrap_i64
                ctx.stack.inplace_unary_op(|a: i64| i32::try_from(a & 0xffffffff).unwrap()).unwrap();
            }
            0xad => {
                // i64.extend_i32_u
                ctx.stack.inplace_unary_op(|a: i32| i64::from(a)).unwrap();
            }
            0xbe => {
                // f32.reinterpret_i32
                ctx.stack.inplace_unary_op(|a: i32| f32::from_ne_bytes(a.to_ne_bytes())).unwrap();
            }
            _ => todo!("opcode {:02x?}", op),
        }

        ctx.profile.executed_instr_time[op as usize] += x.ticks() - start;
    }
}
