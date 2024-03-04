#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use crate::parser::{Item, Reader, SectionKind, TypeKind};
pub use crate::parser::ParserError;

mod parser;
mod str;

#[derive(Debug)]
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

        Ok(FuncSignature {
            params,
            results,
        })
    }
}

pub trait Context {
    fn write_fmt(&mut self, args: fmt::Arguments);
}

pub fn parse(code: &[u8], ctx: &mut impl Context) -> Result<(), ParserError> {
    let mut reader = Reader::new(&code);

    const WASM_MAGIC: &'static [u8; 4] = b"\x00asm";

    if reader.read_bytes::<4>() != Ok(WASM_MAGIC) {
        panic!("missing magic");
    }

    writeln!(ctx, "Version: {:?}", reader.read_u32()?);
    while let Ok(section_type) = reader.read::<SectionKind>() {
        let _section_size = reader.read_usize()?;
        match section_type {
            SectionKind::Custom => {
                let name = reader.read_str()?;
                writeln!(ctx, "Found custom section: {}", name);

                let local_name_type = reader.read_u8()?;
                let subsection_size = reader.read_usize()?;
                let num_funcs = reader.read_u8()?;

                for _ in 0..num_funcs {
                    let func_idx = reader.read_u8()?;
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
                            writeln!(ctx, "Signature: {:?}", reader.read::<FuncSignature>()?);
                        }
                        other => unimplemented!("{:?}", other),
                    }
                }
            }
            SectionKind::Function => {
                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let _sig_index = reader.read_usize()?;
                }
            }
            SectionKind::Export => {
                let num_exports = reader.read_usize()?;
                for _ in 0..num_exports {
                    let name = reader.read_str()?;
                    let export_kind = reader.read_u8()?;
                    let export_func_idx = reader.read_usize()?;
                    writeln!(ctx, "Found exported: {name} | index: {export_func_idx} | kind: {export_kind}");
                }
            }
            SectionKind::Code => {
                let num_funcs = reader.read_usize()?;
                for _ in 0..num_funcs {
                    let body_len = reader.read_usize()?;
                    let locals_num = reader.read_usize()?;
                    loop {
                        let op = reader.read_u8()?;
                        match op {
                            0x0b => {
                                // end
                                writeln!(ctx, "end");
                                break;
                            }
                            0x20 => {
                                // local.get <local>
                                let local_idx = reader.read_u8()?;
                                writeln!(ctx, "local.get {}", local_idx);
                            }
                            0x6a => {
                                // i32.add
                                writeln!(ctx, "i32.add");
                            }
                            _ => unimplemented!("opcode {:02x?}", op),
                        }
                    }
                }
            }
        }
    }

    Ok(())
}