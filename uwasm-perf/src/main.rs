extern crate core;

use std::arch::x86_64::_rdtsc;
use std::fmt::Arguments;
use std::io::Write;
use uwasm::{parse, Context, ParserError, execute_function, VmContext, ImportedFunc};

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, #[allow(unused)] args: Arguments) {
        std::io::stdout().write_fmt(args).unwrap()
    }

    fn ticks(&self) -> u64 {
        unsafe { _rdtsc() }
    }
}

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let content = std::fs::read(path).expect("read file");

    let module = parse(&content, &mut MyCtx)?;
    let mut imports: Vec<ImportedFunc> = Vec::new();
    for name in module.get_imports() {
        imports.push(match name.as_bytes() {
            b"print" => |stack| unsafe {
                stack.push_i32(0);
            },
            _ => todo!("{:?}", name),
        });
    }

    let n = 1_000_000;

    let started = std::time::Instant::now();
    let mut ctx = VmContext::new();
    for _ in 0u32..n {
        let res = execute_function::<(u32,), u32>(&mut ctx, &module, b"entry".into(),(123u32,), &[], &[],  &mut MyCtx).unwrap();
        assert_eq!(res, 0);
    }
    println!("time = {:?}/execution", started.elapsed() / n);

    println!("{:?}", ctx.profile());

    Ok(())
}
