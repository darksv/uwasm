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

#[derive(Debug)]
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
                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let sig_index = reader.read_usize()?;
                    writeln!(ctx, "Function: {:?}", sig_index);
                    func_signatures.push(sig_index);
                }
            }
            SectionKind::Table => {
                let num_tables = reader.read_usize()?;
                for _ in 0..num_tables {
                    let kind = reader.read::<TypeKind>()?;
                    let limits_flags = reader.read_u8()?;
                    let limits_initial = reader.read_u8()?;
                    let limits_max = reader.read_u8()?;
                }
            }
            SectionKind::Memory => {
                writeln!(ctx, "Memory section");
                let num_memories = reader.read_usize()?;
                for _ in 0..num_memories {
                    let limits_flags = reader.read_u8()?;
                    let limits_initial = reader.read_u8()?;
                }
            }
            SectionKind::Global => {
                writeln!(ctx, "Global section");
                let num_globals = reader.read_usize()?;
                for _ in 0..num_globals {
                    let kind = reader.read::<TypeKind>()?;
                    let global_mut = reader.read_u8()?;
                    let _ = reader.read_delimited(0x0B); // FIXME
                }
            }
            SectionKind::Export => {
                writeln!(ctx, "Export section");
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
                let _ = reader.read_bytes::<7>()?; // FIXME
            }
            SectionKind::Code => {
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

                    writeln!(ctx, "{:?}", offsets);

                    let marker = reader.marker();
                    let mut last_if = None;
                    let mut last_else = None;
                    let mut last_block = None;
                    let mut last_loop = None;

                    let mut jump_targets = BTreeMap::new();
                    loop {
                        let pos = reader.pos();
                        let op = reader.read_u8()?;
                        match op {
                            0x01 => {
                                // nop
                                writeln!(ctx, "nop");
                            }
                            0x02 => {
                                // block
                                let block_type = reader.read_u8()?;
                                last_block = Some(pos);
                            }
                            0x03 => {
                                // loop
                                let loop_type = reader.read_u8()?;
                                last_loop = Some(pos);
                            }
                            0x04 => {
                                // if
                                writeln!(ctx, "if");
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
                                    writeln!(ctx, "// end of function");
                                    // end of function
                                    break;
                                }
                            }
                            0x0c => {
                                // br
                                let break_depth = reader.read_u8()?;
                            }
                            0x0d => {
                                // br_if
                                let break_depth = reader.read_u8()?;
                            }
                            0x10 => {
                                // call <func_idx>
                                let func_idx = reader.read_usize()?;
                                writeln!(ctx, "call {}", func_idx);
                            }
                            0x1a => {
                                // drop
                                writeln!(ctx, "drop");
                            }
                            0x20 => {
                                // local.get <local>
                                let local_idx = reader.read_u8()?;
                                writeln!(ctx, "local.get {}", local_idx);
                            }
                            0x21 => {
                                // local.set <local>
                                let local_idx = reader.read_u8()?;
                                writeln!(ctx, "local.set {}", local_idx);
                            }
                            0x41 => {
                                // i32.const <literal>
                                let val = reader.read_u32()?;
                                writeln!(ctx, "i32.const {}", val);
                            }
                            0x42 => {
                                // i64.const <literal>
                                let val = reader.read_u64()?;
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
                            0x63 => {
                                // f64.lt
                                writeln!(ctx, "f64.lt");
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
                            0x68 => {
                                // i32.ctz
                                writeln!(ctx, "i32.ctz");
                            }
                            0x7a => {
                                // i64.ctz
                                writeln!(ctx, "i64.ctz");
                            }
                            0x7c => {
                                // f64
                                writeln!(ctx, "f64");
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
                            _ => {
                                writeln!(ctx, "{:?}", &reader);
                                todo!("opcode {op:02x?} @ {pos:02x}")
                            }
                        }
                    }

                    functions.push(FuncBody {
                        name: None,
                        signature,
                        locals_offsets: offsets,
                        locals_types,
                        offset: marker.pos(),
                        code: marker.into_slice(&mut reader),
                        jump_targets,
                    })
                }
            }
        }
    }

    Ok(WasmModule { functions })
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
