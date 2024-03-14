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
    name: Option<&'code ByteStr>,
    pub signature: FuncSignature,
    offset: usize,
    pub code: &'code [u8],
    jump_targets: BTreeMap<usize, usize>, // if location => else location
}

pub fn parse<'code>(
    code: &'code [u8],
    ctx: &mut impl Context,
) -> Result<WasmModule<'code>, ParserError> {
    let mut reader = Reader::new(&code);
    reader.expect_bytes(b"\x00asm")?;

    let mut exported = Vec::new();
    let mut functions: Vec<FuncBody> = Vec::new();
    let mut signatures = Vec::new();

    writeln!(ctx, "Version: {:?}", reader.read_u32()?);
    while let Ok(section_type) = reader.read::<SectionKind>() {
        let _section_size = reader.read_usize()?;
        match section_type {
            SectionKind::Custom => {
                let name = reader.read_str()?;
                writeln!(ctx, "Found custom section: {}", name);

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
                        other => unimplemented!("{:?}", other),
                    }
                }
            }
            SectionKind::Function => {
                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let sig_index = reader.read_usize()?;
                    writeln!(ctx, "Function: {:?}", sig_index);
                }
            }
            SectionKind::Export => {
                let num_exports = reader.read_usize()?;
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
            SectionKind::Code => {
                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let body_len = reader.read_usize()?;
                    let locals_num = reader.read_usize()?;
                    let marker = reader.marker();
                    let mut last_if = None;
                    let mut last_else = None;
                    let mut jump_targets = BTreeMap::new();
                    loop {
                        let pos = reader.pos();
                        let op = reader.read_u8()?;
                        match op {
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
                                if let Some(le) = last_else {
                                    jump_targets.insert(le, pos + 1 - marker.pos());
                                    last_else = None;
                                } else {
                                    // end of function
                                    break;
                                }
                            }
                            0x10 => {
                                // call <func_idx>
                                let func_idx = reader.read_usize()?;
                                writeln!(ctx, "call {}", func_idx);
                            }
                            0x20 => {
                                // local.get <local>
                                let local_idx = reader.read_u8()?;
                                writeln!(ctx, "local.get {}", local_idx);
                            }
                            0x44 => {
                                // f64.const <literal>
                                let val = reader.read_f64()?;
                                writeln!(ctx, "f64.const {}", val);
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
                            0x7c => {
                                // f64
                                writeln!(ctx, "f64");
                            }
                            0xa1 => {
                                // f64.sub
                                writeln!(ctx, "f64.sub");
                            }
                            0xa2 => {
                                // f64.mul
                                writeln!(ctx, "f64.mul");
                            }
                            _ => unimplemented!("opcode {op:02x?} @ {pos:02x}"),
                        }
                    }
                    functions.push(FuncBody {
                        name: None,
                        signature: signatures[functions.len()].clone(),
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

            assert_eq!(ctx.stack.pop_f64() as u32, native_factorial(i));
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
                assert_eq!(ctx.stack.pop_i32(), j - i);
            }
        }
    }
}
