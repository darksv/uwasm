#![feature(debug_closure_helpers)]
#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

pub use crate::interpreter::{evaluate, StackFrame, UntypedMemorySpan, VmContext};
pub use crate::parser::ParserError;
use crate::parser::{Item, Reader, SectionKind, TypeKind};
use crate::str::ByteStr;

mod interpreter;
mod parser;
mod str;

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
    name: Option<&'code ByteStr>,
    signature: FuncSignature,
    offset: usize,
    pub code: &'code [u8],

    // if location => else location
    jump_targets: BTreeMap<usize, usize>,

    locals_types: Vec<TypeKind>,
    // params + locals
    locals_offsets: Vec<usize>,
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

                    functions.push(FuncBody {
                        name: None,
                        signature,
                        locals_offsets: offsets,
                        locals_types,
                        offset,
                        code,
                        jump_targets,
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
    let mut last_if = None;
    let mut last_else = None;
    let mut last_block = None;
    let mut last_loop = None;
    let mut block_depth = 0;

    let mut jump_targets = BTreeMap::new();
    loop {
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
                block_depth += 1;
            }
            0x03 => {
                // loop
                writeln!(ctx, "loop");
                let loop_type = reader.read_u8()?;
                last_loop = Some(pos);
            }
            0x04 => {
                // if
                writeln!(ctx, "if");
                let ty = reader.read::<TypeKind>()?;
                last_if = Some(pos);
            }
            0x05 => {
                // else
                writeln!(ctx, "else");
                jump_targets.insert(last_if.unwrap(), pos + 1 - marker.pos());
                last_else = Some(pos);
            }
            0x0b => {
                // end
                writeln!(ctx, "end");
                if let Some(le) = last_else.take() {
                    jump_targets.insert(le, pos + 1 - marker.pos());
                } else if let Some(le) = last_block.take() {
                    jump_targets.insert(le, pos + 1 - marker.pos());
                } else if let Some(le) = last_loop.take() {
                    jump_targets.insert(le, pos + 1 - marker.pos());
                } else {
                    if block_depth == 0 {
                        // end of function
                        writeln!(ctx, "// end of function");
                        break;
                    } else {
                        block_depth -= 1;
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
            0x2a => {
                // f32.load
                let align = reader.read_usize()?;
                let offset = reader.read_usize()?;
                writeln!(ctx, "f32.load {} {}", align, offset);
            }
            0x30 => {
                // i64.load8_s
                let align = reader.read_usize()?;
                let offset = reader.read_usize()?;
                writeln!(ctx, "i64.load8_s {} {}", align, offset);
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
            0x4d => {
                // i32.le_u
                writeln!(ctx, "i32.le_u");
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
            0x68 => {
                // i32.ctz
                writeln!(ctx, "i32.ctz");
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
                // i32.and
                writeln!(ctx, "i32.and");
            }
            0x74 => {
                // i32.shl
                writeln!(ctx, "i32.shl");
            }
            0x76 => {
                // i32.shr_u
                writeln!(ctx, "i32.shr_u");
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
            _ => {
                writeln!(ctx, "{:?}", &reader);
                todo!("opcode {op:02x?} @ {pos:02x}")
            }
        }
    }

    Ok(CodeInfo {
        offset: marker.pos(),
        code: marker.into_slice(&mut *reader),
        jump_targets,
    })
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, parse, Context, VmContext};
    use core::fmt::Arguments;

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
            evaluate(&mut ctx, &module, 0, &(i as f64).to_le_bytes(), &mut MyCtx);

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
                evaluate(&mut ctx, &module, 1, &[i.to_le_bytes(), j.to_le_bytes()].concat(), &mut MyCtx);
                assert_eq!(ctx.stack.pop_i32(), Some(j - i));
            }
        }
    }
}
