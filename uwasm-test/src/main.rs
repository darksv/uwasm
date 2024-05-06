extern crate core;

use std::fmt::Arguments;
use std::io::{BufRead, Write};
use std::process::Command;
use anyhow::{bail, Context};
use uwasm::{ByteStr, evaluate, parse, VmContext};

struct MyCtx;

impl uwasm::Context for MyCtx {
    fn write_fmt(&mut self, args: Arguments) {
        std::io::stdout().write_fmt(args).unwrap()
    }
}

#[derive(Debug)]
struct Signature<'s> {
    name: &'s ByteStr,
    args: Vec<&'s ByteStr>,
    returns: &'s ByteStr,
}

struct InputParser<'data> {
    data: &'data [u8],
    pos: usize,
}

impl<'data> InputParser<'data> {
    fn new(data: &'data [u8]) -> Self {
        Self {
            data,
            pos: 0,
        }
    }

    fn take_while(&mut self, f: impl Fn(u8) -> bool) -> &'data ByteStr {
        let start = self.pos;
        let mut end = self.pos;
        while self.pos < self.data.len() {
            if f(self.data[self.pos]) {
                self.pos += 1;
                end = self.pos;
            } else {
                break;
            }
        }
        ByteStr::from_bytes(&self.data[start..end])
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.data[self.pos] == expected {
            self.pos += 1;
            return true;
        }
        false
    }

    fn skip_whitespace(&mut self) {
        self.take_while(|c| c.is_ascii_whitespace());
    }
}

enum Type {
    U32,
    I32,
}

// type_name.as_bytes() {
// b"u32" => Type::U32,
// b"i32" => Type::I32,
// }

fn parse_input(input: &[u8]) -> anyhow::Result<Signature<'_>> {
    for line in input.split(|c| *c == b'\n') {
        let mut parser = InputParser::new(line);
        let name = parser.take_while(|c| c != b'(');
        parser.consume(b'(');
        let mut args = Vec::new();
        while !parser.consume(b')') {
            parser.skip_whitespace();
            let type_name = parser.take_while(|c| c.is_ascii_alphanumeric());
            if type_name.is_empty() {
                break;
            }
            parser.skip_whitespace();
            parser.consume(b',');
            args.push(type_name);
        }
        parser.skip_whitespace();
        parser.consume(b'-');
        parser.consume(b'>');
        parser.skip_whitespace();
        let type_name = parser.take_while(|c| c.is_ascii_alphanumeric());

        return Ok(Signature {
            name,
            args,
            returns: type_name
        });
    }

    unimplemented!()
}

fn main() -> anyhow::Result<()> {
    let Some(path) = std::env::args_os().nth(1) else {
        bail!("missing path to test files")
    };

    let dir = std::fs::read_dir(path)
        .context("reading test directory")?;

    for entry in dir {
        let entry = entry.context("reading entry")?;

        if entry.path().extension().is_some_and(|ext| ext.to_str() == Some("input")) {
            let input_text = std::fs::read(entry.path())?;
            let input = parse_input(&input_text)?;

            println!("{:?}", input);

            let rs_path = entry.path().with_extension("rs");
            let wasm_path = entry.path().with_extension("wasm");
            if rs_path.exists() {
                println!("compiling {}", rs_path.display());
                let status = Command::new("rustc")
                    .arg("--target=wasm32-unknown-unknown")
                    .arg(rs_path)
                    .arg("-O")
                    .arg("-o")
                    .arg(&wasm_path)
                    .arg("-C")
                    .arg("panic=abort")
                    .spawn()
                    .expect("compile rust code")
                    .wait();
                println!("result {:?}", status);
            }

            let content = std::fs::read(wasm_path).expect("read file");
            let module = parse(&content, &mut MyCtx)?;
            dbg!(&module);

            let idx = module.functions
                .iter()
                .position(|f| f.name.is_some_and(|b| b.as_bytes() == input.name.as_bytes()))
                .context("selecting entry function")?;

            let mut mem = Vec::new();
            for param in &input.args {
                match param.as_bytes() {
                    b"u32" => mem.extend_from_slice(&0u32.to_ne_bytes()),
                    _ => unimplemented!("other: {:?}", param),
                }
            }

            let data = b"123456";
            let mut ctx = VmContext::new();
            evaluate(&mut ctx, &module, idx, &mem, data, &mut MyCtx);
            match input.returns.as_bytes() {
                b"u32" => println!("{:?}", ctx.stack.pop_u32()),
                _ => unimplemented!("other: {:?}", input.returns),
            }
        }
    }
    Ok(())
}
