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

include!("../../tests/simple_code.rs");

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let content = std::fs::read(path).expect("read file");

    let module = parse(&content, &mut MyCtx)?;
    dbg!(&module);

    let mut ctx = VmContext::new();
    for i in 0..10 {
        evaluate(&mut ctx, &module, 1, &[100u32.to_le_bytes(), (i as u32).to_le_bytes()].concat(), &mut MyCtx);
        assert_eq!(ctx.stack.pop_u32(), Some(factorial(100, i)));
    }

    Ok(())
}
