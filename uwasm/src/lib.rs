#![feature(debug_closure_helpers)]
#![feature(split_at_checked)]
#![feature(error_in_core)]
#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use core::ops::ControlFlow;

pub use crate::interpreter::{evaluate, StackFrame, UntypedMemorySpan, VmContext};
use crate::parser::{Item, Reader, SectionKind, TypeKind};
pub use crate::parser::ParserError;
pub use crate::str::ByteStr;

mod interpreter;
mod parser;
mod str;
mod operand;

#[derive(Debug, Clone)]
struct FuncSignature {
    params: Vec<TypeKind>,
    #[allow(unused)]
    results: Vec<TypeKind>,
}

impl Item for FuncSignature {
    fn read(reader: &mut Reader, _offset: usize) -> Result<Self, ParserError> {
        let num_params = reader.read_usize()?;
        let mut params = Vec::new();
        for _ in 0..num_params {
            params.push(reader.read::<TypeKind>()?);
        }

        let num_results = reader.read_usize()?;
        let mut results = Vec::new();
        for _ in 0..num_results {
            results.push(reader.read::<TypeKind>()?);
        }

        Ok(FuncSignature { params, results })
    }
}

pub trait Context {
    fn write_fmt(&mut self, args: fmt::Arguments);
}

#[derive(Debug)]
pub struct WasmModule<'code> {
    pub functions: Vec<FuncBody<'code>>,
}

pub struct FuncBody<'code> {
    #[allow(unused)]
    pub name: Option<&'code ByteStr>,
    signature: FuncSignature,
    offset: usize,
    pub code: &'code [u8],

    // if location => else location
    jump_targets: BTreeMap<usize, usize>,

    locals_types: Vec<TypeKind>,
    // params + locals
    locals_offsets: Vec<usize>,
    // total length of parameters that this function accepts
    params_len_in_bytes: usize,
}

impl fmt::Debug for FuncBody<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: fix formatting

        f
            .debug_struct("FuncBody")
            .field_with("code", |f| {
                struct Wrapper<'a, 'b>(&'a mut fmt::Formatter<'b>);
                impl Context for Wrapper<'_, '_> {
                    fn write_fmt(&mut self, args: fmt::Arguments) {
                        self.0.write_fmt(args).unwrap();
                    }
                }

                let mut reader = Reader::new(self.code);
                _ = parse_code(&mut reader, &mut Wrapper(f));
                Ok(())
            })
            .finish()
    }
}

impl<'code> FuncBody<'code> {
    fn non_param_locals(&self) -> impl Iterator<Item=TypeKind> + '_ {
        self.locals_types.iter().skip(self.signature.params.len()).copied()
    }
}

#[allow(unused)]
pub fn parse<'code>(
    code: &'code [u8],
    ctx: &mut impl Context,
) -> Result<WasmModule<'code>, ParserError> {
    let mut reader = Reader::new(code);
    reader.expect_bytes(b"\x00asm")?;

    let mut exported = Vec::new();
    let mut functions: Vec<FuncBody> = Vec::new();
    let mut signatures = Vec::new();
    let mut func_signatures = Vec::new();

    writeln!(ctx, "Version: {:?}", reader.read_u32()?);
    while let Ok(section_type) = reader.read::<SectionKind>() {
        let _section_size = reader.read_usize()?;
        match section_type {
            SectionKind::Custom => {
                let name = reader.read_str()?;
                writeln!(ctx, "Found custom section: {}", name);

                break; // FIXME

                let _local_name_type = reader.read_u8()?;
                let _subsection_size = reader.read_usize()?;
                let num_funcs = reader.read_u8()?;

                for _ in 0..num_funcs {
                    let _func_idx = reader.read_u8()?;
                    let num_locals = reader.read_u8()?;
                    for _ in 0..num_locals {
                        // TODO: read locals
                    }
                }
            }
            SectionKind::Type => {
                writeln!(ctx, "Found type section");

                let num_types = reader.read_usize()?;
                for _ in 0..num_types {
                    let kind = reader.read::<TypeKind>()?;
                    match kind {
                        TypeKind::Func => {
                            let sig = reader.read::<FuncSignature>()?;
                            writeln!(ctx, "Signature: {:?}", sig);
                            signatures.push(sig);
                        }
                        other => todo!("{:?}", other),
                    }
                }
            }
            SectionKind::Function => {
                writeln!(ctx, "Found function section");

                let num_funcs = reader.read_usize()?;
                for func_idx in 0..num_funcs {
                    let sig_index = reader.read_usize()?;
                    writeln!(ctx, "Function #{func_idx} | signature #{sig_index}: {:?}", &signatures[sig_index]);
                    func_signatures.push(sig_index);
                }
            }
            SectionKind::Table => {
                writeln!(ctx, "Found table section");

                let num_tables = reader.read_usize()?;
                for _ in 0..num_tables {
                    let kind = reader.read::<TypeKind>()?;
                    let limits_flags = reader.read_u8()?;
                    let limits_initial = reader.read_u8()?;
                    let limits_max = reader.read_u8()?;
                }
            }
            SectionKind::Memory => {
                writeln!(ctx, "Found memory section");
                let num_memories = reader.read_usize()?;
                for _ in 0..num_memories {
                    let limits_flags = reader.read_u8()?;
                    let limits_initial = reader.read_u8()?;
                }
            }
            SectionKind::Global => {
                writeln!(ctx, "Found global section");
                let num_globals = reader.read_usize()?;
                for _ in 0..num_globals {
                    let kind = reader.read::<TypeKind>()?;
                    let global_mut = reader.read_u8()?;
                    let _ = reader.read_delimited(0x0B); // FIXME
                }
            }
            SectionKind::Export => {
                writeln!(ctx, "Found export section");
                let num_exports = reader.read_usize()?;
                writeln!(ctx, "{num_exports}");
                for _ in 0..num_exports {
                    let name = reader.read_str()?;
                    let export_kind = reader.read_u8()?;
                    let export_func_idx = reader.read_usize()?;
                    writeln!(
                        ctx,
                        "Found exported: {name} | index: {export_func_idx} | kind: {export_kind}"
                    );
                    exported.push(name);
                }
            }
            SectionKind::Elem => {
                writeln!(ctx, "Found elem section");
                let num_elem_segments = reader.read_usize()?;
                for _ in 0..num_elem_segments {
                    let segment_flags = reader.read_u8()?;
                    loop {
                        let opcode = reader.read_u8()?;
                        match opcode {
                            0x41 => {
                                // i32.const
                                _ = reader.read_usize()?;
                            }
                            0x0b => {
                                // end
                                break;
                            }
                            _ => todo!(),
                        };
                    }
                    let num_elements = reader.read_usize()?;
                    for _ in 0..num_elements {
                        _ = reader.read_usize()?;
                    }
                    // FIXME
                }
            }
            SectionKind::Code => {
                writeln!(ctx, "Found code section");

                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let signature = signatures[func_signatures[functions.len()]].clone();

                    let body_len = reader.read_usize()?;
                    let locals_num = reader.read_usize()?;

                    let mut locals_types = Vec::new();
                    // Copy params into params
                    locals_types.extend(signature.params.iter().copied());

                    // Copy actual function locals
                    for _ in 0..locals_num {
                        let n = reader.read_usize()?;
                        let ty = reader.read::<TypeKind>()?;
                        for _ in 0..n {
                            locals_types.push(ty);
                        }
                    }

                    let mut offsets = Vec::with_capacity(signature.params.len() + locals_types.len());
                    let mut offset = 0;
                    for param in locals_types.iter() {
                        offsets.push(offset);
                        offset += param.len_bytes();
                    }

                    writeln!(ctx, "offsets={:?}", offsets);

                    let CodeInfo { offset, code, jump_targets } = parse_code(&mut reader, ctx)?;
                    let params_len_in_bytes = signature
                        .params
                        .iter()
                        .map(|t| t.len_bytes())
                        .sum();
                    functions.push(FuncBody {
                        name: None,
                        signature,
                        locals_offsets: offsets,
                        locals_types,
                        offset,
                        code,
                        jump_targets,
                        params_len_in_bytes,
                    })
                }
            }
        }
    }

    Ok(WasmModule { functions })
}

struct CodeInfo<'code> {
    offset: usize,
    code: &'code [u8],
    jump_targets: BTreeMap<usize, usize>,
}

fn parse_code<'c>(reader: &mut Reader<'c>, ctx: &mut impl Context) -> Result<CodeInfo<'c>, ParserError> {
    let marker = reader.marker();
    let mut state = ParserState::default();

    loop {
        match parse_opcode::<false>(reader, marker.pos(), ctx, &mut state)? {
            ControlFlow::Continue(_) => continue,
            ControlFlow::Break(_) => break,
        }
    }

    Ok(CodeInfo {
        offset: marker.pos(),
        code: marker.into_slice(&mut *reader),
        jump_targets: state.jump_targets,
    })
}

#[derive(Debug, PartialEq)]
enum BlockType {
    Block,
    Loop,
    If,
    Else,
}

struct BlockMeta {
    kind: BlockType,
    offset: usize,
}

#[derive(Default)]
struct ParserState {
    blocks: Vec<BlockMeta>,
    jump_targets: BTreeMap<usize, usize>,
}

fn parse_opcode<const ONLY_PRINT: bool>(reader: &mut Reader, func_offset: usize, ctx: &mut impl Context, state: &mut ParserState) -> Result<ControlFlow<(), ()>, ParserError> {
    let pos = reader.pos();
    let op = reader.read_u8()?;
    match op {
        0x00 => {
            // unreachable
            writeln!(ctx, "unreachable");
        }
        0x01 => {
            // nop
            writeln!(ctx, "nop");
        }
        0x02 => {
            // block
            let block_type = reader.read_u8()?;
            writeln!(ctx, "block {:02x}", block_type);
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::Block, offset: pos });
            }
        }
        0x03 => {
            // loop
            writeln!(ctx, "loop");
            let loop_type = reader.read_u8()?;
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::Loop, offset: pos });
            }
        }
        0x04 => {
            // if
            writeln!(ctx, "if");
            let ty = reader.read::<TypeKind>()?;
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::If, offset: pos });
            }
        }
        0x05 => {
            // else
            writeln!(ctx, "else");
            if !ONLY_PRINT {
                let BlockMeta { kind, offset } = state.blocks.pop().unwrap();
                assert_eq!(kind, BlockType::If);
                state.jump_targets.insert(offset, pos + 1 - func_offset);
                state.blocks.push(BlockMeta { kind: BlockType::Else, offset: pos });
            }
        }
        0x0b => {
            // end
            writeln!(ctx, "end");
            if !ONLY_PRINT {
                if let Some(BlockMeta { kind, offset: start_offset }) = state.blocks.pop() {
                    writeln!(ctx, "// end {:?} @ {:02X}", kind, pos);
                    state.jump_targets.insert(start_offset, pos + 1 - func_offset);
                } else {
                    // end of function
                    writeln!(ctx, "// end of function");
                    return Ok(ControlFlow::Break(()));
                }
            }
        }
        0x0c => {
            // br
            let break_depth = reader.read_usize()?;
            writeln!(ctx, "br {}", break_depth);
        }
        0x0d => {
            // br_if
            let break_depth = reader.read_usize()?;
            writeln!(ctx, "br_if {}", break_depth);
        }
        0x0e => {
            // br_table
            // FIXME
            let n = reader.read_usize()?;
            write!(ctx, "br_table");
            for i in 0..n {
                let n = reader.read_usize()?;
                write!(ctx, " {}", n);
            }
            let else_c = reader.read_usize()?;
            writeln!(ctx, " {} ", else_c);
        }
        0x0f => {
            // return
            writeln!(ctx, "return");
        }
        0x10 => {
            // call <func_idx>
            let func_idx = reader.read_usize()?;
            writeln!(ctx, "call {}", func_idx);
        }
        0x11 => {
            // call_indirect <func_idx>
            let sig_idx = reader.read_usize()?;
            let table_idx = reader.read_usize()?;
            writeln!(ctx, "call_indirect {} {}", sig_idx, table_idx);
        }
        0x1a => {
            // drop
            writeln!(ctx, "drop");
        }
        0x1b => {
            // select
            writeln!(ctx, "select");
        }
        0x20 => {
            // local.get <local>
            let local_idx = reader.read_usize()?;
            writeln!(ctx, "local.get {}", local_idx);
        }
        0x21 => {
            // local.set <local>
            let local_idx = reader.read_usize()?;
            writeln!(ctx, "local.set {}", local_idx);
        }
        0x22 => {
            // local.tee <local>
            let local_idx = reader.read_usize()?;
            writeln!(ctx, "local.tee {}", local_idx);
        }
        0x23 => {
            // global.get <global>
            let global_idx = reader.read_usize()?;
            writeln!(ctx, "global.get {}", global_idx);
        }
        0x24 => {
            // global.set <global>
            let global_idx = reader.read_usize()?;
            writeln!(ctx, "global.set {}", global_idx);
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
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            let name = match op {
                0x28 => "i32",
                0x29 => "i64",
                0x2a => "f32",
                0x2b => "f64",
                0x2c => "i32.load8_s",
                0x2d => "i32.load8_u",
                0x2e => "i32.load16_s",
                0x2f => "i32.load16_u",
                0x30 => "i64.load8_s",
                0x31 => "i64.load8_u",
                0x32 => "i64.load16_s",
                0x33 => "i64.load16_u",
                0x34 => "i64.load32_s",
                0x35 => "i64.load32_u",
                _ => unreachable!(),
            };
            writeln!(ctx, "{name} align={align} offset={offset}");
        }
        0x36 => {
            // i32.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "i32.store {} {}", align, offset);
        }
        0x37 => {
            // i64.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "i64.store {} {}", align, offset);
        }
        0x39 => {
            // f64.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "f64.store {} {}", align, offset);
        }
        0x3a => {
            // i32.store8
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "i32.store8 {} {}", align, offset);
        }
        0x3b => {
            // i32.store16
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "i32.store16 {} {}", align, offset);
        }
        0x3d => {
            // i64.store16
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(ctx, "i64.store16 {} {}", align, offset);
        }
        0x40 => {
            // memory.grow
            let mem_idx = reader.read_usize()?;
            writeln!(ctx, "memory.grow {}", mem_idx);
        }
        0x41 => {
            // i32.const <literal>
            let val = reader.read_signed()?;
            writeln!(ctx, "i32.const {}", val);
        }
        0x42 => {
            // i64.const <literal>
            let val = reader.read_signed()?;
            writeln!(ctx, "i64.const {}", val);
        }
        0x43 => {
            // f32.const <literal>
            let val = reader.read_f32()?;
            writeln!(ctx, "f32.const {}", val);
        }
        0x44 => {
            // f64.const <literal>
            let val = reader.read_f64()?;
            writeln!(ctx, "f64.const {}", val);
        }
        0x45 => {
            // i32.eqz
            writeln!(ctx, "i32.eqz");
        }
        0x46 => {
            // i32.eq
            writeln!(ctx, "i32.eq");
        }
        0x47 => {
            // i32.ne
            writeln!(ctx, "i32.ne");
        }
        0x48 => {
            // i32.lt_s
            writeln!(ctx, "i32.lt_s");
        }
        0x49 => {
            // i32.lt_u
            writeln!(ctx, "i32.lt_u");
        }
        0x4a => {
            // i32.le_s
            writeln!(ctx, "i32.le_s");
        }
        0x4b => {
            // i32.gt_s
            writeln!(ctx, "i32.gt_s");
        }
        0x4c => {
            // i32.gt_u
            writeln!(ctx, "i32.gt_u");
        }
        0x4d => {
            // i32.le_u
            writeln!(ctx, "i32.le_u");
        }
        0x4e => {
            // i32.ge_s
            writeln!(ctx, "i32.ge_s");
        }
        0x4f => {
            // i32.ge_u
            writeln!(ctx, "i32.ge_u");
        }
        0x5c => {
            // f32.ne
            writeln!(ctx, "f32.ne");
        }
        0x63 => {
            // f64.lt
            writeln!(ctx, "f64.lt");
        }
        0x65 => {
            // f64.le
            writeln!(ctx, "f64.le");
        }
        0x67 => {
            // i32.clz
            writeln!(ctx, "i32.clz");
        }
        0x68 => {
            // i32.ctz
            writeln!(ctx, "i32.ctz");
        }
        0x69 => {
            // i32.popcnt
            writeln!(ctx, "i32.popcnt");
        }
        0x6a => {
            // i32.add
            writeln!(ctx, "i32.add");
        }
        0x6b => {
            // i32.sub
            writeln!(ctx, "i32.sub");
        }
        0x6c => {
            // i32.mul
            writeln!(ctx, "i32.mul");
        }
        0x6d => {
            // i32.div_s
            writeln!(ctx, "i32.div_s");
        }
        0x6e => {
            // i32.div_u
            writeln!(ctx, "i32.div_u");
        }
        0x6f => {
            // i32.rem_s
            writeln!(ctx, "i32.rem_s");
        }
        0x70 => {
            // i32.rem_u
            writeln!(ctx, "i32.rem_u");
        }
        0x71 => {
            // i32.and
            writeln!(ctx, "i32.and");
        }
        0x72 => {
            // i32.or
            writeln!(ctx, "i32.or");
        }
        0x73 => {
            // i32.xor
            writeln!(ctx, "i32.xor");
        }
        0x74 => {
            // i32.shl
            writeln!(ctx, "i32.shl");
        }
        0x75 => {
            // i32.shr_s
            writeln!(ctx, "i32.shr_s");
        }
        0x76 => {
            // i32.shr_u
            writeln!(ctx, "i32.shr_u");
        }
        0x77 => {
            // i32.rotl
            writeln!(ctx, "i32.rotl");
        }
        0x78 => {
            // i32.rotr
            writeln!(ctx, "i32.rotr");
        }
        0x7a => {
            // i64.ctz
            writeln!(ctx, "i64.ctz");
        }
        0x7c => {
            // i64.add
            writeln!(ctx, "i64.add");
        }
        0x7d => {
            // i64.sub
            writeln!(ctx, "i64.sub");
        }
        0x7e => {
            // i64.mul
            writeln!(ctx, "i64.mul");
        }
        0x88 => {
            // i64.shr_u
            writeln!(ctx, "i64.shr_u");
        }
        0x8c => {
            // f32.neg
            writeln!(ctx, "f32.neg");
        }
        0x92 => {
            // f32.add
            writeln!(ctx, "f32.add");
        }
        0x9a => {
            // f64.neg
            writeln!(ctx, "f64.neg");
        }
        0xa0 => {
            // f64.add
            writeln!(ctx, "f64.add");
        }
        0xa1 => {
            // f64.sub
            writeln!(ctx, "f64.sub");
        }
        0xa2 => {
            // f64.mul
            writeln!(ctx, "f64.mul");
        }
        0xa7 => {
            // i32.wrap_i64
            writeln!(ctx, "i32.wrap_i64");
        }
        0xad => {
            // i64.extend_i32_u
            writeln!(ctx, "i64.extend_i32_u");
        }
        0xbe => {
            // f32.reinterpret_i32
            writeln!(ctx, "f32.reinterpret_i32");
        }
        0xc0 => {
            // i32.extend8_s
            writeln!(ctx, "i32.extend8_s");
        }
        0xc1 => {
            // i32.extend16_s
            writeln!(ctx, "i32.extend16_s");
        }
        _ => {
            writeln!(ctx, "{:?}", &reader);
            todo!("opcode {op:02x?} @ {pos:02x}")
        }
    }

    Ok(ControlFlow::Continue(()))
}

#[cfg(test)]
mod tests {
    use core::fmt::Arguments;

    use crate::{Context, evaluate, parse, VmContext};

    struct MyCtx;

    impl Context for MyCtx {
        fn write_fmt(&mut self, _args: Arguments) {}
    }

    fn native_factorial(n: u32) -> u32 {
        (1..=n).product()
    }

    #[test]
    fn factorial() {
        let module =
            parse(include_bytes!("../../tests/factorial.wasm"), &mut MyCtx).expect("parse module");
        let mut ctx = VmContext::new();
        for i in 0..10 {
            evaluate(&mut ctx, &module, 0, &(i as f64).to_le_bytes(), &[], &mut MyCtx);

            assert_eq!(ctx.stack.pop_f64(), Some(native_factorial(i) as f64));
        }
    }

    #[test]
    fn multivalue_sub() {
        let module =
            parse(include_bytes!("../../tests/multivalue.wasm"), &mut MyCtx).expect("parse module");
        let mut ctx = VmContext::new();
        for i in 0..10i32 {
            for j in 10..20i32 {
                evaluate(&mut ctx, &module, 1, &[i.to_le_bytes(), j.to_le_bytes()].concat(), &[], &mut MyCtx);
                assert_eq!(ctx.stack.pop_i32(), Some(j - i));
            }
        }
    }

    #[test]
    fn sum_array_of_f32() {
        let module =
            parse(include_bytes!("../../tests/sum_array.wasm"), &mut MyCtx).expect("parse module");
        let mut ctx = VmContext::new();
        let numbers = [1.23f32, 4.56];
        let data = unsafe { core::slice::from_raw_parts(numbers.as_ptr().cast(), numbers.len() * 4) };
        evaluate(&mut ctx, &module, 0, &[0u32.to_le_bytes(), (numbers.len() as u32).to_le_bytes()].concat(), data, &mut MyCtx);
        assert_eq!(ctx.stack.pop_f32(), Some(5.79));
    }

    #[test]
    fn sum_array_of_f32_recurrent() {
        let module =
            parse(include_bytes!("../../tests/sum_array_rec.wasm"), &mut MyCtx).expect("parse module");
        let mut ctx = VmContext::new();
        let numbers = [1.23f32, 4.56, -10.0];
        let data = unsafe { core::slice::from_raw_parts(numbers.as_ptr().cast(), numbers.len() * 4) };
        evaluate(&mut ctx, &module, 0, &[0u32.to_le_bytes(), (numbers.len() as u32).to_le_bytes()].concat(), data, &mut MyCtx);
        assert_eq!(ctx.stack.pop_f32(), Some(-4.21));
    }
}
