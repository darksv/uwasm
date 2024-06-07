use alloc::fmt;
use alloc::vec::Vec;
use core::fmt::Formatter;
use core::iter;

use crate::{ByteStr, Context, FuncBody, ParserError, WasmModule};
#[cfg(debug_assertions)]
use crate::{parse_opcode, ParserState};
use crate::operand::Operand;
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
    pub fn new(module: &'code WasmModule, idx: usize, locals_offset: usize) -> Result<Self, InterpreterError> {
        let Some(func) = module.get_function_by_index(idx) else {
            return Err(InterpreterError::FunctionNotFound);
        };

        Ok(Self {
            func_idx: idx,
            reader: Reader::new(func.code),
            locals_offset,
            curr_loop_start: None,
            blocks: Vec::new(),
        })
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

    fn pop_bytes<const N: usize>(&mut self) -> Result<[u8; N], InterpreterError> {
        let (rest, &bytes) = self.data.split_last_chunk::<N>()
            .ok_or_else(|| InterpreterError::StackTooSmall)?;
        self.data.drain(rest.len()..);
        Ok(bytes)
    }

    pub fn peek_bytes<const N: usize>(&self) -> Result<[u8; N], InterpreterError> {
        self.data.split_last_chunk::<N>().map(|(_, bytes)| *bytes)
            .ok_or_else(|| InterpreterError::StackTooSmall)
    }

    #[inline]
    pub fn pop_i32(&mut self) -> Result<i32, InterpreterError> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(i32::from_le_bytes)
    }

    #[inline]
    pub fn pop_i64(&mut self) -> Result<i64, InterpreterError> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(i64::from_le_bytes)
    }

    #[inline]
    pub fn pop_u32(&mut self) -> Result<u32, InterpreterError> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(u32::from_le_bytes)
    }

    #[inline]
    pub fn pop_f32(&mut self) -> Result<f32, InterpreterError> {
        #[cfg(debug_assertions)]
        self.types.pop();
        self.pop_bytes().map(f32::from_le_bytes)
    }

    #[inline]
    pub fn pop_f64(&mut self) -> Result<f64, InterpreterError> {
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
    fn inplace_bin_op<T: Operand, U: Operand, R: Operand>(&mut self, op: impl FnOnce(T, U) -> R) -> Result<R, InterpreterError> {
        let b = U::pop(self)?;
        let a = T::pop(self)?;
        let result = op(a, b);
        R::push(self, result);
        Ok(result)
    }

    #[inline]
    #[track_caller]
    fn inplace_unary_op<T: Operand, R: Operand>(&mut self, op: impl FnOnce(T) -> R) -> Result<R, InterpreterError> {
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
                        if let Ok(val) = reader.read_f32() {
                            fmt.entry(&val);
                        }
                    }
                    TypeKind::F64 => {
                        if let Ok(val) = reader.read_f64() {
                            fmt.entry(&val);
                        }
                    }
                    TypeKind::I32 => {
                        if let Ok(val) = reader.read_i32() {
                            fmt.entry(&val);
                        }
                    }
                    TypeKind::I64 => {
                        if let Ok(val) = reader.read_i64() {
                            fmt.entry(&val);
                        }
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
        offsets: &[usize],
        idx: usize,
    ) -> Option<&[u8; N]> {
        let offset = offsets.get(idx).copied()?;
        self.data.get(offset..)?.first_chunk()
    }

    #[inline]
    fn write_param_raw<const N: usize>(
        &mut self,
        offsets: &[usize],
        idx: usize,
        data: [u8; N],
    ) -> Option<()> {
        // FIXME
        let offset = offsets.get(idx).copied()?;
        self.data.get_mut(offset..)?
            .first_chunk_mut::<N>()?
            .copy_from_slice(&data);
        Some(())
    }

    #[inline]
    fn push_into(&self, stack: &mut VmStack, var_idx: usize, var_type: TypeKind, offsets: &[usize]) {
        match var_type {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 => stack.push_bytes(TypeKind::F32, *self.read_param_raw::<4>(offsets, var_idx).unwrap()),
            TypeKind::F64 => stack.push_bytes(TypeKind::F64, *self.read_param_raw::<8>(offsets, var_idx).unwrap()),
            TypeKind::I32 => stack.push_bytes(TypeKind::I32, *self.read_param_raw::<4>(offsets, var_idx).unwrap()),
            TypeKind::I64 => stack.push_bytes(TypeKind::I64, *self.read_param_raw::<8>(offsets, var_idx).unwrap()),
        }
    }

    #[inline]
    fn pop_from(&mut self, stack: &mut VmStack, var_idx: usize, var_type: TypeKind, offsets: &[usize]) {
        match var_type {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 => self.write_param_raw::<4>(offsets, var_idx, stack.pop_f32().unwrap().to_ne_bytes()).expect("invalid var_index"),
            TypeKind::F64 => self.write_param_raw::<8>(offsets, var_idx, stack.pop_f64().unwrap().to_ne_bytes()).expect("invalid var_index"),
            TypeKind::I32 => self.write_param_raw::<4>(offsets, var_idx, stack.pop_i32().unwrap().to_ne_bytes()).expect("invalid var_index"),
            TypeKind::I64 => self.write_param_raw::<8>(offsets, var_idx, stack.pop_i64().unwrap().to_ne_bytes()).expect("invalid var_index"),
        }
    }

    #[inline]
    fn copy_from(&mut self, stack: &mut VmStack, var_idx: usize, var_type: TypeKind, offsets: &[usize]) {
        match var_type {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F32 | TypeKind::I32 => self.write_param_raw::<4>(offsets, var_idx, stack.peek_bytes().unwrap()).unwrap(),
            TypeKind::F64 | TypeKind::I64 => self.write_param_raw::<8>(offsets, var_idx, stack.peek_bytes().unwrap()).unwrap(),
        }
    }
}

#[inline]
fn copy_params_and_locals(locals: &mut Vec<u8>, params_data: &[u8], func_body: &FuncBody) {
    locals.extend_from_slice(params_data);
    locals.resize(locals.len() + func_body.non_param_locals_len_in_bytes, 0);
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

    #[inline]
    #[track_caller]
    fn write_bytes_at<const N: usize>(&mut self, offset: usize, bytes: &[u8; N]) {
        assert!(offset + N <= self.data.len(), "out of bounds write: offset={offset} data={n}", n = self.data.len());
        let (_, data) = self.data.split_at_mut_checked(offset).unwrap();
        let (raw, _) = data.split_first_chunk_mut::<N>().unwrap();
        raw.copy_from_slice(bytes);
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

    fn write_u8(&mut self, offset: usize, value: u8) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    fn write_i8(&mut self, offset: usize, value: i8) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    fn write_u16(&mut self, offset: usize, value: u16) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    fn write_i16(&mut self, offset: usize, value: i16) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    fn write_i32(&mut self, offset: usize, value: i32) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    fn write_i64(&mut self, offset: usize, value: i64) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
    }

    #[track_caller]
    fn write_u64(&mut self, offset: usize, value: u64) {
        self.write_bytes_at(offset, &value.to_ne_bytes());
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
pub enum InterpreterError {
    ParserError(ParserError),
    FunctionNotFound,
    FunctionWithoutBody,
    InvalidSignature,
    StackEmpty,
    StackTooSmall,
    Unreachable,
}

impl From<ParserError> for InterpreterError {
    fn from(value: ParserError) -> Self {
        Self::ParserError(value)
    }
}

pub type ImportedFunc<TContext> = fn(&mut TContext, &mut VmStack, &mut [u8]);

pub fn init_globals(globals: &mut Vec<u8>, module: &WasmModule) {
    for global in &module.globals {
        // TODO: run full interpreter here
        let mut reader = Reader::new(global.initializer.code);
        loop {
            let op = reader.read_u8().unwrap();
            match op {
                0x0b => {
                    // end
                    break;
                }
                0x41 => {
                    // i32.const <literal>
                    let val = reader.read_isize().unwrap();
                    let val = i32::try_from(val).unwrap();
                    globals.extend_from_slice(&val.to_ne_bytes());
                }
                0x42 => {
                    // i64.const <literal>
                    let val = reader.read_usize().unwrap();
                    let val = i64::try_from(val).unwrap();
                    globals.extend_from_slice(&val.to_ne_bytes());
                }
                0x43 => {
                    // f32.const <literal>
                    let val = reader.read_f32().unwrap();
                    globals.extend_from_slice(&val.to_ne_bytes());
                }
                0x44 => {
                    // f64.const <literal>
                    let val = reader.read_f64().unwrap();
                    globals.extend_from_slice(&val.to_ne_bytes());
                }
                _ => todo!("opcode {:02x?}", op),
            }
        }
    }
}

pub fn execute_function<'code, TContext: Context, TArgs: FunctionArgs, TResult: Operand>(
    ctx: &mut VmContext<'code>,
    module: &'code WasmModule<'code>,
    func_name: &ByteStr,
    args: TArgs,
    memory: &mut [u8],
    globals: &mut [u8],
    imports: &[ImportedFunc<TContext>],
    execution_ctx: &mut TContext,
) -> Result<TResult, InterpreterError> {
    let Some(func_idx) = module.get_function_index_by_name(func_name) else {
        return Err(InterpreterError::FunctionNotFound);
    };

    let Some(func) = &module.functions[func_idx].body else {
        return Err(InterpreterError::FunctionWithoutBody);
    };

    if func.signature.params.len() != TArgs::TYPE.len() {
        return Err(InterpreterError::InvalidSignature);
    }

    for (expected, actual) in iter::zip(&func.signature.params, TArgs::TYPE) {
        if expected != actual {
            return Err(InterpreterError::InvalidSignature);
        }
    }

    // TODO: check result types

    let mut args_mem = Serializer {
        buf: Vec::new(),
    };
    args.write_to(&mut args_mem);
    evaluate(ctx, module, func_idx, &args_mem.buf, globals, memory, imports, execution_ctx)?;
    TResult::pop(&mut ctx.stack)
}

pub fn evaluate<'code, TContext: Context>(
    ctx: &mut VmContext<'code>,
    module: &'code WasmModule<'code>,
    func_idx: usize,
    args: &[u8],
    globals: &mut [u8],
    memory: &mut [u8],
    imports: &[ImportedFunc<TContext>],
    #[allow(unused)]
    x: &mut TContext,
) -> Result<(), InterpreterError> {
    ctx.stack.data.clear();

    let Some(func) = module.get_function_by_index(func_idx) else {
        return Err(InterpreterError::FunctionNotFound);
    };

    copy_params_and_locals(&mut ctx.locals, args, func);
    ctx.call_stack.clear();
    ctx.call_stack.push(StackFrame::new(
        module,
        func_idx,
        0,
    )?);

    while let Some(frame) = ctx.call_stack.last_mut() {
        let current_func = module.get_function_by_index(frame.func_idx)
            .expect("function existed at time of the call");
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
            Err(e) => panic!("other error: {e:?}"),
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
                return Err(InterpreterError::Unreachable);
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
                        let x = ctx.stack.pop_f32()?;
                        x != 0.0
                    }
                    TypeKind::F64 => {
                        let x = ctx.stack.pop_f64()?;
                        x != 0.0
                    }
                    TypeKind::I32 => {
                        let x = ctx.stack.pop_i32()?;
                        x != 0
                    }
                    TypeKind::I64 => {
                        let x = ctx.stack.pop_i64()?;
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
                    writeln!(x, "end of {:?}", block.kind);
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
                let block = &frame.blocks[block_idx];
                let target = match block.kind {
                    BlockType::Block => current_func.jump_targets[&block.offset] - 1,
                    BlockType::Loop => block.body_offset,
                };
                reader.skip_to(target);
                // skip blocks that we are no longer executing due to the jump
                // TODO: check if this is correct
                frame.blocks.drain(block_idx + 1..);
                #[cfg(debug_assertions)]
                writeln!(x, "taken");
            }
            0x0d => {
                // br_if
                // TODO: dedup with br?
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

                if let Some(callee) = module.get_function_by_index(func_idx) {
                    ctx.call_stack.push(StackFrame {
                        func_idx,
                        reader: Reader::new(callee.code),
                        locals_offset: ctx.locals.len(),
                        curr_loop_start: None,
                        blocks: Vec::new(),
                    });
                    let params_mem = &ctx.stack.data[ctx.stack.data.len() - callee.params_len_in_bytes..];
                    copy_params_and_locals(&mut ctx.locals, params_mem, callee);
                    ctx.stack.pop_many(callee.params_len_in_bytes);
                } else {
                    #[cfg(debug_assertions)]
                    writeln!(x, "calling imported function {}", func_idx);
                    imports[func_idx](x, &mut ctx.stack, memory);
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

                let local_idx = reader.read_usize().unwrap();
                locals.push_into(&mut ctx.stack, local_idx, current_func.locals_types[local_idx], &current_func.locals_offsets);
            }
            0x21 => {
                // local.set <local>
                let local_idx = reader.read_usize().unwrap();
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).pop_from(&mut ctx.stack, local_idx, current_func.locals_types[local_idx], &current_func.locals_offsets);
            }
            0x22 => {
                // local.tee <local>
                let local_idx = reader.read_usize().unwrap();
                UntypedMemorySpan::from_slice_mut(
                    &mut ctx.locals[frame.locals_offset..]
                ).copy_from(&mut ctx.stack, local_idx, current_func.locals_types[local_idx], &current_func.locals_offsets);
            }
            0x23 => {
                // global.get <global>
                let global_idx = reader.read_usize().unwrap();
                UntypedMemorySpan::from_slice(globals)
                    .push_into(
                        &mut ctx.stack,
                        global_idx,
                        module.globals[global_idx].kind,
                        &module.globals_offsets,
                    );
            }
            0x24 => {
                // global.set <global>
                let global_idx = reader.read_usize().unwrap();
                UntypedMemorySpan::from_slice_mut(globals)
                    .pop_from(
                        &mut ctx.stack,
                        global_idx,
                        module.globals[global_idx].kind,
                        &module.globals_offsets,
                    );
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
                let alignment = reader.read_usize().unwrap();
                let fixed_offset = reader.read_usize().unwrap();
                let dyn_offset = ctx.stack.pop_i32().unwrap();
                let offset = fixed_offset.checked_add_signed(dyn_offset as _).unwrap();
                let mem = Memory::from_slice(memory);
                #[cfg(debug_assertions)]
                writeln!(x, "load: mem[{fixed_offset}{dyn_offset:+}]");
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
            0x36..=0x3e => {
                // i32.store 	0x36
                // i64.store 	0x37
                // f32.store 	0x38
                // f64.store 	0x39
                // i32.store8 	0x3a
                // i32.store16 	0x3b
                // i64.store8 	0x3c
                // i64.store16 	0x3d
                // i64.store32 	0x3e
                let mem = Memory::from_slice_mut(memory);
                let alignment = reader.read_usize().unwrap();
                let fixed_offset = reader.read_usize().unwrap();

                match op {
                    0x36 => {
                        // i32.store
                        let val = ctx.stack.pop_i32().unwrap();
                        let dyn_offset = ctx.stack.pop_i32().unwrap() as isize;
                        #[cfg(debug_assertions)]
                        writeln!(x, "i32.store: mem[{fixed_offset}{dyn_offset:+}] <- {val}");
                        let offset = fixed_offset.checked_add_signed(dyn_offset).unwrap();
                        mem.write_i32(offset, val);
                    }
                    0x37 => {
                        // i64.store
                        let val = ctx.stack.pop_i64().unwrap();
                        let idx = ctx.stack.pop_i32().unwrap() as isize;
                        #[cfg(debug_assertions)]
                        writeln!(x, "i64.store: mem[{fixed_offset}{idx:+}] <- {val}");
                        let offset = fixed_offset.checked_add_signed(idx).unwrap();
                        mem.write_i64(offset, val);
                    }
                    0x38 => todo!(), // f32.store
                    0x39 => todo!(), // f64.store
                    0x3a => {
                        // i32.store8
                        let val = ctx.stack.pop_i32().unwrap() as i8;
                        let idx = ctx.stack.pop_i32().unwrap() as isize;
                        #[cfg(debug_assertions)]
                        writeln!(x, "i32.store8: mem[{fixed_offset}{idx:+}] <- {val}");
                        let offset = fixed_offset.checked_add_signed(idx).unwrap();
                        mem.write_i8(offset, val);
                    }
                    0x3b => {
                        // i32.store16
                        let val = ctx.stack.pop_i32().unwrap() as i16;
                        let idx = ctx.stack.pop_i32().unwrap() as isize;
                        #[cfg(debug_assertions)]
                        writeln!(x, "i32.store16: mem[{fixed_offset}{idx:+}] <- {val}");
                        let offset = fixed_offset.checked_add_signed(idx).unwrap();
                        mem.write_i16(offset, val);
                    }
                    0x3c => todo!(), // i64.store8
                    0x3d => todo!(), // i64.store16
                    0x3e => todo!(), // i64.store32
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
            0x6e => {
                // i32.div_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a / b).unwrap();
            }
            0x70 => {
                // i32.rem_u
                ctx.stack.inplace_bin_op(|a: u32, b: u32| a % b).unwrap();
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
            0x84 => {
                // i64.or
                ctx.stack.inplace_bin_op(|a: i64, b: i64| a | b).unwrap();
            }
            0x86 => {
                // i64.shl
                ctx.stack.inplace_bin_op(|a: i64, b: i64| a << b).unwrap();
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
            0xc0 => {
                // i32.extend8_s
                ctx.stack.inplace_unary_op(|a: i32| a).unwrap();
            }
            _ => todo!("opcode {:02x?}", op),
        }

        ctx.profile.executed_instr_time[op as usize] += x.ticks() - start;
    }

    Ok(())
}
