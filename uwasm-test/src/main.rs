extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{Context, parse, ParserError};

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

    parse(&content, &mut MyCtx)?;

    Ok(())
}
