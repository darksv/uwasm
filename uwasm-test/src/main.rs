extern crate core;

use std::fmt::Arguments;
use std::io::Write;
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

#[derive(Debug)]
struct Case<'s> {
    #[allow(unused)]
    name: &'s ByteStr,
    args: Vec<&'s ByteStr>,
    expected: &'s ByteStr,
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

    fn has_data(&self) -> bool {
        self.pos < self.data.len()
    }

    #[must_use]
    fn take_while(&mut self, f: impl Fn(u8) -> bool) -> &'data ByteStr {
        let start = self.pos;
        let mut end = self.pos;
        while self.has_data() {
            if f(self.data[self.pos]) {
                self.pos += 1;
                end = self.pos;
            } else {
                break;
            }
        }
        ByteStr::from_bytes(&self.data[start..end])
    }

    fn consume<const N: usize>(&mut self, expected: &[u8; N]) -> bool {
        if self.pos + N > self.data.len() {
            return false;
        }

        if &self.data[self.pos..][..N] == expected {
            self.pos += N;
            return true;
        }
        false
    }

    fn consume_whitespace(&mut self) {
        _ = self.take_while(|c| c.is_ascii_whitespace());
    }
}

fn parse_input(input: &[u8]) -> anyhow::Result<(Signature<'_>, Vec<Case<'_>>)> {
    let mut parser = InputParser::new(input);
    let signature;
    if parser.has_data() {
        let name = parser.take_while(|c| c != b'(');
        parser.consume(b"(");
        let mut args = Vec::new();
        while !parser.consume(b")") {
            parser.consume_whitespace();
            let type_name = parser.take_while(|c| c.is_ascii_alphanumeric());
            if type_name.is_empty() {
                break;
            }
            parser.consume_whitespace();
            parser.consume(b",");
            args.push(type_name);
        }
        parser.consume_whitespace();
        parser.consume(b"->");
        parser.consume_whitespace();
        let type_name = parser.take_while(|c| c.is_ascii_alphanumeric());

        signature = Signature {
            name,
            args,
            returns: type_name,
        };
    } else {
        bail!("no data");
    }

    let mut cases = Vec::new();
    while parser.has_data() {
        let name = parser.take_while(|c| c != b'(');
        parser.consume(b"(");
        let mut args = Vec::new();
        while !parser.consume(b")") {
            parser.consume_whitespace();
            let type_name = parser.take_while(|c| matches!(c, b'0'..=b'9' | b'-'));
            if type_name.is_empty() {
                break;
            }
            parser.consume_whitespace();
            parser.consume(b",");
            args.push(type_name);
        }
        parser.consume_whitespace();
        parser.consume(b"=");
        parser.consume_whitespace();
        let expected = parser.take_while(|c| matches!(c, b'0'..=b'9' | b'-'));

        cases.push(Case {
            name,
            args,
            expected
        });
    }

    Ok((signature, cases))
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
            let (signature, inputs) = parse_input(&input_text)?;

            println!("{:?} {:?}", signature, inputs);

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

            let idx = module.get_function_index_by_name(signature.name)
                .context("selecting entry function")?;

            for case in inputs {
                let mut mem = Vec::new();
                for (idx, param) in signature.args.iter().enumerate() {
                    match param.as_bytes() {
                        b"u32" => {
                            let val: u32 = std::str::from_utf8(&case.args[idx]).unwrap().parse().unwrap();
                            mem.extend_from_slice(&val.to_ne_bytes())
                        },
                        _ => unimplemented!("other: {:?}", param),
                    }
                }

                let mut ctx = VmContext::new();
                evaluate(&mut ctx, &module, idx, &mem, &[], &mut MyCtx);
                match signature.returns.as_bytes() {
                    b"u32" => {
                        let res = ctx.stack.pop_u32().unwrap();
                        let expected: u32 = std::str::from_utf8(&case.expected).unwrap().parse().unwrap();
                        assert_eq!(res, expected);
                    },
                    _ => unimplemented!("other: {:?}", signature.returns),
                };

            }
        }
    }
    Ok(())
}
