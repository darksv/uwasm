use crate::parser::{Item, ParserError, Reader, SectionKind, TypeKind};

mod parser;

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

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let content = std::fs::read(path).expect("read file");

    let mut reader = Reader::new(&content);

    const WASM_MAGIC: &'static [u8; 4] = b"\x00asm";

    if reader.read_bytes::<4>() != Ok(WASM_MAGIC) {
        panic!("missing magic");
    }

    println!("Version: {:?}", reader.read_u32()?);
    while let Ok(section_type) = reader.read::<SectionKind>() {
        let section_size = reader.read_usize()?;
        match section_type {
            SectionKind::Type => {
                let num_types = reader.read_usize()?;
                for _ in 0..num_types {
                    let kind = reader.read::<TypeKind>()?;
                    match kind {
                        TypeKind::Func => {
                            println!("Signature: {:?}", reader.read::<FuncSignature>()?);
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
                    let name_len = reader.read_usize()?;
                    let name = reader.read_slice(name_len as _)?;
                    let name = std::str::from_utf8(name).expect("valid utf8"); // TODO
                    let export_kind = reader.read_u8()?;
                    let export_func_idx = reader.read_usize()?;
                    println!("Found exported: {name} | index: {export_func_idx} | kind: {export_kind}");
                }
            }
            other => unimplemented!("{:?}", other),
        }
        if section_size == 0 {
            let _fixup = reader.read_usize()?;
        }
    }

    Ok(())
}
