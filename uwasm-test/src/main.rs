extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{evaluate, parse, Context, ParserError, VmContext};

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
    dbg!(&module);

    let mut ctx = VmContext::new();
    for i in 0..10 {
        evaluate(&mut ctx, &module, 0, &[100u32.to_le_bytes(), (i as u32).to_le_bytes()].concat(), &mut MyCtx);
        dbg!(ctx.stack.pop_i32());
    }

    Ok(())
}
