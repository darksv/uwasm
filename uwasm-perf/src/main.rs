extern crate core;

use std::fmt::Arguments;
use uwasm::{parse, Context, ParserError, execute_function, VmContext};

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, #[allow(unused)] args: Arguments) {
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
    let n = 1_000_000;

    let started = std::time::Instant::now();
    let mut ctx = VmContext::new();
    for _ in 0u32..n {
        let res: f64 = execute_function(&mut ctx, &module, b"fac".into(),(15.0f64,), &[], &[],  &mut MyCtx).unwrap();
        assert_eq!(res, native_factorial(15) as f64);
    }
    println!("time = {:?}/execution", started.elapsed() / n);

    println!("{:?}", ctx.profile());

    Ok(())
}
