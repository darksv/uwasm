extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{Context, evaluate, parse, ParserError, VmContext};

struct MyCtx;
impl Context for MyCtx {
    fn write_fmt(&mut self, args: Arguments) {
        std::io::stdout().write_fmt(args).unwrap()
    }
}

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let content = std::fs::read(path).expect("read file");

    let module = parse(&content, &mut MyCtx)?;
    let mut ctx = VmContext::new();
    evaluate(&mut ctx, &module.functions[0], &[1.0], &module.functions[..]);
    dbg!(module);

    Ok(())
}
