extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{Context, evaluate, parse, ParserError, VmContext, StackFrame};

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
    for i in 0..10 {
        ctx.call_stack.push(StackFrame::new(&module, 0, (i as f64).to_le_bytes().to_vec()));
        evaluate(&mut ctx, 0, &module.functions[..], &mut MyCtx);
        dbg!(ctx.stack.pop_f64());
    }

    Ok(())
}
