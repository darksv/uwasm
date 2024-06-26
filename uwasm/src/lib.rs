#![feature(debug_closure_helpers)]
#![feature(error_in_core)]
#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use core::ops::ControlFlow;

pub use crate::interpreter::{init_globals, init_memory, evaluate, execute_function, StackFrame, UntypedMemorySpan, VmContext, VmStack, ImportedFunc};
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

pub trait Environment {
    fn write_fmt(&mut self, args: fmt::Arguments);
    fn ticks(&self) -> u64;
}

#[derive(Debug)]
pub struct WasmModule<'code> {
    functions: Vec<Func<'code>>,
    globals: Vec<Global<'code>>,
    data_segments: Vec<DataSegment<'code>>,
    globals_offsets: Vec<usize>,
    tables: Vec<Table>,
}

impl<'code> WasmModule<'code> {
    fn get_function_by_index(&self, index: usize) -> Option<&FuncBody<'code>> {
        self.functions.get(index)?.body.as_ref()
    }

    pub fn get_function_index_by_name(&self, name: &ByteStr) -> Option<usize> {
        self.functions
            .iter()
            .position(|f| f.name.is_some_and(|b| b.as_bytes() == name.as_bytes()))
    }

    pub fn get_imports(&self) -> impl Iterator<Item=&ByteStr> {
        self.functions.iter().filter_map(|f| f.body.is_none().then(|| f.name.as_deref().unwrap()))
    }
}

pub struct FuncBody<'code> {
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
    // total length of internal function locals
    non_param_locals_len_in_bytes: usize,
}

impl fmt::Debug for FuncBody<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: fix formatting

        f
            .debug_struct("FuncBody")
            .field("signature", &self.signature)
            .field_with("code", |f| {
                struct DummyEnv<'a, 'b>(&'a mut fmt::Formatter<'b>);
                impl Environment for DummyEnv<'_, '_> {
                    fn write_fmt(&mut self, args: fmt::Arguments) {
                        self.0.write_fmt(args).unwrap();
                    }

                    fn ticks(&self) -> u64 {
                        0
                    }
                }

                let mut reader = Reader::new(self.code);
                _ = parse_code(&mut reader, &mut DummyEnv(f));
                Ok(())
            })
            .field("locals_types", &self.locals_types)
            .finish()
    }
}

#[derive(Debug)]
pub struct Func<'code> {
    body: Option<FuncBody<'code>>,
    pub name: Option<&'code ByteStr>,
    signature: Option<usize>,
}

struct Global<'c> {
    kind: TypeKind,
    mutability: u8,
    initializer: CodeInfo<'c>,
}

impl fmt::Debug for Global<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Global")
            .field("kind", &self.kind)
            .field("mutability", &self.mutability)
            .finish()
    }
}

fn offsets_of_types(types: impl ExactSizeIterator<Item=TypeKind>) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(types.len());
    let mut offset = 0;
    for param in types {
        offsets.push(offset);
        offset += param.len_bytes();
    }
    offsets
}

struct DataSegment<'code> {
    flags: u8,
    offset: CodeInfo<'code>,
    data: &'code [u8],
}

impl fmt::Debug for DataSegment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataSegment")
            .field("flags", &self.flags)
            .finish()
    }
}

#[derive(Debug)]
struct Table {
    kind: TypeKind,
    limits_flags: u8,
    limits_initial: u8,
    limits_max: u8
}

pub fn parse<'code>(
    code: &'code [u8],
    env: &mut impl Environment,
) -> Result<WasmModule<'code>, ParserError> {
    let mut reader = Reader::new(code);
    reader.expect_bytes(b"\x00asm")?;

    let mut functions: Vec<_> = Vec::new();
    let mut signatures = Vec::new();
    let mut imports = 0;
    let mut globals = Vec::new();
    let mut data_segments = Vec::new();
    let mut tables = Vec::new();

    writeln!(env, "Version: {:?}", reader.read_u32()?);
    while let Ok(section_type) = reader.read::<SectionKind>() {
        let _section_size = reader.read_usize()?;
        match section_type {
            #[allow(unused)]
            SectionKind::Custom => {
                let name = reader.read_str()?;
                writeln!(env, "Found custom section: {}", name);

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
                writeln!(env, "Found type section");

                let num_types = reader.read_usize()?;
                for _ in 0..num_types {
                    let kind = reader.read::<TypeKind>()?;
                    match kind {
                        TypeKind::Func => {
                            let sig = reader.read::<FuncSignature>()?;
                            writeln!(env, "Signature: {:?}", sig);
                            signatures.push(sig);
                        }
                        other => todo!("{:?}", other),
                    }
                }
            }
            SectionKind::Import => {
                writeln!(env, "Found import section");
                let num_imports = reader.read_usize()?;
                writeln!(env, "{num_imports}");
                for _ in 0..num_imports {
                    let module_name = reader.read_str()?;
                    let field_name = reader.read_str()?;
                    let import_kind = reader.read_u8()?;
                    let import_sig_idx = reader.read_usize()?;
                    writeln!(
                        env,
                        "Found imported: {module_name}.{field_name} | signature index: {import_sig_idx} | kind: {import_kind}"
                    );
                    if import_kind == 0 {
                        // function
                        functions.push(Func {
                            body: None,
                            name: Some(field_name),
                            signature: Some(import_sig_idx),
                        });
                        imports += 1;
                    }
                }
            }
            SectionKind::Function => {
                writeln!(env, "Found function section");

                let num_funcs = reader.read_usize()?;
                writeln!(env, "{:?}", num_funcs);
                for func_idx in 0..num_funcs {
                    let sig_index = reader.read_usize()?;
                    writeln!(env, "Function #{func_idx} | signature #{sig_index}: {:?}", &signatures[sig_index]);
                    functions.push(Func {
                        body: None,
                        name: None,
                        signature: Some(sig_index),
                    });
                }
            }
            SectionKind::Table => {
                writeln!(env, "Found table section");

                let num_tables = reader.read_usize()?;
                for _ in 0..num_tables {
                    let kind = reader.read::<TypeKind>()?;
                    let limits_flags = reader.read_u8()?;
                    let limits_initial = reader.read_u8()?;
                    let limits_max = reader.read_u8()?;

                    tables.push(Table {
                        kind,
                        limits_flags,
                        limits_initial,
                        limits_max,
                    });
                }
            }
            SectionKind::Memory => {
                writeln!(env, "Found memory section");
                let num_memories = reader.read_usize()?;
                for _ in 0..num_memories {
                    let _limits_flags = reader.read_u8()?;
                    let _limits_initial = reader.read_u8()?;
                }
            }
            SectionKind::Global => {
                writeln!(env, "Found global section");
                let num_globals = reader.read_usize()?;
                for i in 0..num_globals {
                    let kind = reader.read::<TypeKind>()?;
                    let global_mut = reader.read_u8()?;
                    writeln!(env, "global #{i}: {:?} mut={}", kind, global_mut);
                    let code = parse_code(&mut reader, env)?;

                    globals.push(Global {
                        kind,
                        mutability: global_mut,
                        initializer: code,
                    });
                }
            }
            SectionKind::Export => {
                writeln!(env, "Found export section");
                let num_exports = reader.read_usize()?;
                writeln!(env, "{num_exports}");
                for _ in 0..num_exports {
                    let name = reader.read_str()?;
                    let export_kind = reader.read_u8()?;
                    let export_func_idx = reader.read_usize()?;
                    writeln!(
                        env,
                        "Found exported: {name} | index: {export_func_idx} | kind: {export_kind}"
                    );
                    if export_kind == 0 {
                        // function
                        functions[export_func_idx].name = Some(name);
                    }
                }
            }
            SectionKind::Elem => {
                writeln!(env, "Found elem section");
                let num_elem_segments = reader.read_usize()?;
                for _ in 0..num_elem_segments {
                    let _segment_flags = reader.read_u8()?;
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
                writeln!(env, "Found code section");

                let num_funcs = reader.read_usize()?;
                for func_idx in 0..num_funcs {
                    let signature = signatures[functions[imports + func_idx].signature.unwrap()].clone();

                    let _body_len = reader.read_usize()?;
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

                    let offsets = offsets_of_types(locals_types.iter().copied());
                    writeln!(env, "offsets={:?}", offsets);

                    let CodeInfo { offset, code, jump_targets } = parse_code(&mut reader, env)?;
                    let params_len_in_bytes = signature
                        .params
                        .iter()
                        .map(|t| t.len_bytes())
                        .sum();
                    let non_param_locals_len_in_bytes = locals_types[signature.params.len()..]
                        .iter()
                        .map(|ty| ty.len_bytes())
                        .sum();
                    functions[imports + func_idx].body = Some(FuncBody {
                        signature,
                        locals_offsets: offsets,
                        locals_types,
                        offset,
                        code,
                        jump_targets,
                        params_len_in_bytes,
                        non_param_locals_len_in_bytes,
                    });
                }
            }
            SectionKind::Data => {
                writeln!(env, "Found data section");

                let num_segments = reader.read_usize()?;
                for _ in 0..num_segments {
                    let segment_flags = reader.read_u8()?;
                    let code = parse_code(&mut reader, env)?;
                    let data_len = reader.read_usize()?;
                    let data = reader.read_slice(data_len)?;

                    data_segments.push(DataSegment {
                        flags: segment_flags,
                        offset: code,
                        data
                    });
                }
            }
        }
    }

    let globals_offsets = offsets_of_types(globals.iter().map(|it| it.kind));
    Ok(WasmModule { functions, globals, globals_offsets, data_segments, tables })
}

struct CodeInfo<'code> {
    offset: usize,
    code: &'code [u8],
    jump_targets: BTreeMap<usize, usize>,
}

fn parse_code<'c>(reader: &mut Reader<'c>, env: &mut impl Environment) -> Result<CodeInfo<'c>, ParserError> {
    let marker = reader.marker();
    let mut state = ParserState::default();

    loop {
        match parse_opcode::<false>(reader, marker.pos(), env, &mut state)? {
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

fn parse_opcode<const ONLY_PRINT: bool>(
    reader: &mut Reader,
    func_offset: usize,
    env: &mut impl Environment,
    state: &mut ParserState
) -> Result<ControlFlow<(), ()>, ParserError> {
    let pos = reader.pos();
    let op = reader.read_u8()?;
    match op {
        0x00 => {
            // unreachable
            writeln!(env, "unreachable");
        }
        0x01 => {
            // nop
            writeln!(env, "nop");
        }
        0x02 => {
            // block
            let block_type = reader.read_u8()?;
            writeln!(env, "block {:02x}", block_type);
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::Block, offset: pos });
            }
        }
        0x03 => {
            // loop
            writeln!(env, "loop");
            let _loop_type = reader.read_u8()?;
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::Loop, offset: pos });
            }
        }
        0x04 => {
            // if
            writeln!(env, "if");
            let _ty = reader.read::<TypeKind>()?;
            if !ONLY_PRINT {
                state.blocks.push(BlockMeta { kind: BlockType::If, offset: pos });
            }
        }
        0x05 => {
            // else
            writeln!(env, "else");
            if !ONLY_PRINT {
                let BlockMeta { kind, offset } = state.blocks.pop().unwrap();
                assert_eq!(kind, BlockType::If);
                state.jump_targets.insert(offset, pos + 1 - func_offset);
                state.blocks.push(BlockMeta { kind: BlockType::Else, offset: pos });
            }
        }
        0x0b => {
            // end
            if !ONLY_PRINT {
                write!(env, "end");
                if let Some(BlockMeta { kind, offset: start_offset }) = state.blocks.pop() {
                    writeln!(env, " // {:?} @ {:02X}", kind, pos);
                    state.jump_targets.insert(start_offset, pos + 1 - func_offset);
                } else {
                    // end of function
                    writeln!(env, " // code");
                    return Ok(ControlFlow::Break(()));
                }
            } else {
                writeln!(env, "end");
            }
        }
        0x0c => {
            // br
            let break_depth = reader.read_usize()?;
            writeln!(env, "br {}", break_depth);
        }
        0x0d => {
            // br_if
            let break_depth = reader.read_usize()?;
            writeln!(env, "br_if {}", break_depth);
        }
        0x0e => {
            // br_table
            // FIXME
            let n = reader.read_usize()?;
            write!(env, "br_table");
            for _ in 0..n {
                let n = reader.read_usize()?;
                write!(env, " {}", n);
            }
            let else_c = reader.read_usize()?;
            writeln!(env, " {} ", else_c);
        }
        0x0f => {
            // return
            writeln!(env, "return");
        }
        0x10 => {
            // call <func_idx>
            let func_idx = reader.read_usize()?;
            writeln!(env, "call {}", func_idx);
        }
        0x11 => {
            // call_indirect <func_idx>
            let sig_idx = reader.read_usize()?;
            let table_idx = reader.read_usize()?;
            writeln!(env, "call_indirect {} {}", sig_idx, table_idx);
        }
        0x1a => {
            // drop
            writeln!(env, "drop");
        }
        0x1b => {
            // select
            writeln!(env, "select");
        }
        0x20 => {
            // local.get <local>
            let local_idx = reader.read_usize()?;
            writeln!(env, "local.get {}", local_idx);
        }
        0x21 => {
            // local.set <local>
            let local_idx = reader.read_usize()?;
            writeln!(env, "local.set {}", local_idx);
        }
        0x22 => {
            // local.tee <local>
            let local_idx = reader.read_usize()?;
            writeln!(env, "local.tee {}", local_idx);
        }
        0x23 => {
            // global.get <global>
            let global_idx = reader.read_usize()?;
            writeln!(env, "global.get {}", global_idx);
        }
        0x24 => {
            // global.set <global>
            let global_idx = reader.read_usize()?;
            writeln!(env, "global.set {}", global_idx);
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
                0x28 => "i32.load",
                0x29 => "i64.load",
                0x2a => "f32.load",
                0x2b => "f64.load",
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
            writeln!(env, "{name} align={align} offset={offset}");
        }
        0x36 => {
            // i32.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "i32.store align={align} offset={offset}");
        }
        0x37 => {
            // i64.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "i64.store align={align} offset={offset}");
        }
        0x39 => {
            // f64.store
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "f64.store align={align} offset={offset}");
        }
        0x3a => {
            // i32.store8
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "i32.store8 align={align} offset={offset}");
        }
        0x3b => {
            // i32.store16
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "i32.store16 align={align} offset={offset}");
        }
        0x3d => {
            // i64.store16
            let align = reader.read_usize()?;
            let offset = reader.read_usize()?;
            writeln!(env, "i64.store16 align={align} offset={offset}");
        }
        0x40 => {
            // memory.grow
            let mem_idx = reader.read_usize()?;
            writeln!(env, "memory.grow {}", mem_idx);
        }
        0x41 => {
            // i32.const <literal>
            let val = reader.read_signed()?;
            writeln!(env, "i32.const {}", val);
        }
        0x42 => {
            // i64.const <literal>
            let val = reader.read_signed()?;
            writeln!(env, "i64.const {}", val);
        }
        0x43 => {
            // f32.const <literal>
            let val = reader.read_f32()?;
            writeln!(env, "f32.const {}", val);
        }
        0x44 => {
            // f64.const <literal>
            let val = reader.read_f64()?;
            writeln!(env, "f64.const {}", val);
        }
        0x45 => {
            // i32.eqz
            writeln!(env, "i32.eqz");
        }
        0x46 => {
            // i32.eq
            writeln!(env, "i32.eq");
        }
        0x47 => {
            // i32.ne
            writeln!(env, "i32.ne");
        }
        0x48 => {
            // i32.lt_s
            writeln!(env, "i32.lt_s");
        }
        0x49 => {
            // i32.lt_u
            writeln!(env, "i32.lt_u");
        }
        0x4a => {
            // i32.le_s
            writeln!(env, "i32.le_s");
        }
        0x4b => {
            // i32.gt_s
            writeln!(env, "i32.gt_s");
        }
        0x4c => {
            // i32.gt_u
            writeln!(env, "i32.gt_u");
        }
        0x4d => {
            // i32.le_u
            writeln!(env, "i32.le_u");
        }
        0x4e => {
            // i32.ge_s
            writeln!(env, "i32.ge_s");
        }
        0x4f => {
            // i32.ge_u
            writeln!(env, "i32.ge_u");
        }
        0x50 => {
            // i64.eqz
            writeln!(env, "i64.eqz");
        }
        0x52 => {
            // i64.ne
            writeln!(env, "i64.ne");
        }
        0x54 => {
            // i64.lt_u
            writeln!(env, "i64.lt_u");
        }
        0x56 => {
            // i64.gt_u
            writeln!(env, "i64.gt_u");
        }
        0x5a => {
            // i64.ge_u
            writeln!(env, "i64.ge_u");
        }
        0x5c => {
            // f32.ne
            writeln!(env, "f32.ne");
        }
        0x63 => {
            // f64.lt
            writeln!(env, "f64.lt");
        }
        0x65 => {
            // f64.le
            writeln!(env, "f64.le");
        }
        0x67 => {
            // i32.clz
            writeln!(env, "i32.clz");
        }
        0x68 => {
            // i32.ctz
            writeln!(env, "i32.ctz");
        }
        0x69 => {
            // i32.popcnt
            writeln!(env, "i32.popcnt");
        }
        0x6a => {
            // i32.add
            writeln!(env, "i32.add");
        }
        0x6b => {
            // i32.sub
            writeln!(env, "i32.sub");
        }
        0x6c => {
            // i32.mul
            writeln!(env, "i32.mul");
        }
        0x6d => {
            // i32.div_s
            writeln!(env, "i32.div_s");
        }
        0x6e => {
            // i32.div_u
            writeln!(env, "i32.div_u");
        }
        0x6f => {
            // i32.rem_s
            writeln!(env, "i32.rem_s");
        }
        0x70 => {
            // i32.rem_u
            writeln!(env, "i32.rem_u");
        }
        0x71 => {
            // i32.and
            writeln!(env, "i32.and");
        }
        0x72 => {
            // i32.or
            writeln!(env, "i32.or");
        }
        0x73 => {
            // i32.xor
            writeln!(env, "i32.xor");
        }
        0x74 => {
            // i32.shl
            writeln!(env, "i32.shl");
        }
        0x75 => {
            // i32.shr_s
            writeln!(env, "i32.shr_s");
        }
        0x76 => {
            // i32.shr_u
            writeln!(env, "i32.shr_u");
        }
        0x77 => {
            // i32.rotl
            writeln!(env, "i32.rotl");
        }
        0x78 => {
            // i32.rotr
            writeln!(env, "i32.rotr");
        }
        0x7a => {
            // i64.ctz
            writeln!(env, "i64.ctz");
        }
        0x7c => {
            // i64.add
            writeln!(env, "i64.add");
        }
        0x7d => {
            // i64.sub
            writeln!(env, "i64.sub");
        }
        0x7e => {
            // i64.mul
            writeln!(env, "i64.mul");
        }
        0x80 => {
            // i64.div_u
            writeln!(env, "i64.div_u");
        }
        0x82 => {
            // i64.rem_u
            writeln!(env, "i64.rem_u");
        }
        0x83 => {
            // i64.and
            writeln!(env, "i64.and");
        }
        0x84 => {
            // i64.or
            writeln!(env, "i64.or");
        }
        0x86 => {
            // i64.shl
            writeln!(env, "i64.shl");
        }
        0x88 => {
            // i64.shr_u
            writeln!(env, "i64.shr_u");
        }
        0x8c => {
            // f32.neg
            writeln!(env, "f32.neg");
        }
        0x92 => {
            // f32.add
            writeln!(env, "f32.add");
        }
        0x9a => {
            // f64.neg
            writeln!(env, "f64.neg");
        }
        0xa0 => {
            // f64.add
            writeln!(env, "f64.add");
        }
        0xa1 => {
            // f64.sub
            writeln!(env, "f64.sub");
        }
        0xa2 => {
            // f64.mul
            writeln!(env, "f64.mul");
        }
        0xa7 => {
            // i32.wrap_i64
            writeln!(env, "i32.wrap_i64");
        }
        0xad => {
            // i64.extend_i32_u
            writeln!(env, "i64.extend_i32_u");
        }
        0xbe => {
            // f32.reinterpret_i32
            writeln!(env, "f32.reinterpret_i32");
        }
        0xc0 => {
            // i32.extend8_s
            writeln!(env, "i32.extend8_s");
        }
        0xc1 => {
            // i32.extend16_s
            writeln!(env, "i32.extend16_s");
        }
        _ => {
            writeln!(env, "opcode {op:02x?} @ {pos:02x}")
        }
    }

    Ok(ControlFlow::Continue(()))
}

#[cfg(test)]
mod tests {
    use core::fmt::Arguments;

    use crate::{Environment, execute_function, parse, VmContext};

    struct MyEnv;

    impl Environment for MyEnv {
        fn write_fmt(&mut self, _args: Arguments) {}

        fn ticks(&self) -> u64 {
            0
        }
    }

    fn native_factorial(n: u32) -> u32 {
        (1..=n).product()
    }

    #[test]
    fn factorial() {
        let module =
            parse(include_bytes!("../../tests/factorial.wasm"), &mut MyEnv).expect("parse module");
        let mut ctx = VmContext::new();
        for i in 0..10 {
            let result = execute_function::<MyEnv, (f64, ), f64>(&mut ctx, &module, b"fac".into(), (i as f64, ), &mut [], &mut [], &[], &mut MyEnv).unwrap();
            assert_eq!(result, native_factorial(i) as f64);
        }
    }

    #[test]
    fn multivalue_sub() {
        let module =
            parse(include_bytes!("../../tests/multivalue.wasm"), &mut MyEnv).expect("parse module");
        let mut ctx = VmContext::new();
        for i in 0..10i32 {
            for j in 10..20i32 {
                let result = execute_function::<MyEnv, (i32, i32), i32>(&mut ctx, &module, b"reverseSub".into(), (i, j), &mut [], &mut [], &[], &mut MyEnv).unwrap();
                assert_eq!(result, j - i);
            }
        }
    }

    #[test]
    fn sum_array_of_f32() {
        let module =
            parse(include_bytes!("../../tests/sum_array.wasm"), &mut MyEnv).expect("parse module");
        let mut ctx = VmContext::new();
        let mut numbers = [1.23f32, 4.56];
        let data = unsafe { core::slice::from_raw_parts_mut(numbers.as_mut_ptr().cast(), numbers.len() * 4) };
        let result = execute_function::<MyEnv, (u32, u32), f32>(&mut ctx, &module, b"sum_slice".into(), (0u32, numbers.len() as u32), data, &mut [], &[], &mut MyEnv).unwrap();
        assert_eq!(result, 5.79);
    }

    #[test]
    fn sum_array_of_f32_recurrent() {
        let module =
            parse(include_bytes!("../../tests/sum_array_rec.wasm"), &mut MyEnv).expect("parse module");
        let mut ctx = VmContext::new();
        let mut numbers = [1.23f32, 4.56, -10.0];
        let data = unsafe { core::slice::from_raw_parts_mut(numbers.as_mut_ptr().cast(), numbers.len() * 4) };
        let result = execute_function::<MyEnv, (u32, u32), f32>(&mut ctx, &module, b"sum_slice".into(), (0u32, numbers.len() as u32), data, &mut [], &[], &mut MyEnv).unwrap();
        assert_eq!(result, -4.21);
    }
}
