extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{evaluate, parse, Context, ParserError, VmContext};

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, args: Arguments) {
        // std::io::stdout().write_fmt(args).unwrap()
    }
}

fn native_factorial(n: u64) -> u64 {
    (1..=n).product()
}

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let content = std::fs::read(path).expect("read file");

    let module = parse(&content, &mut MyCtx)?;
    let mut ctx = VmContext::new();

    let n = 1_000_000;

    let started = std::time::Instant::now();
    for i in 0u32..n {
        evaluate(&mut ctx, &module, 0, &15.0f64.to_le_bytes(), &mut MyCtx);
        assert_eq!(ctx.stack.pop_f64(), native_factorial(15) as f64);
    }
    println!("time = {:?}/execution", started.elapsed() / n);

    Ok(())
}
