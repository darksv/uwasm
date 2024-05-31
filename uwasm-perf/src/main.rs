extern crate core;

use std::arch::x86_64::_rdtsc;
use std::fmt::Arguments;
use std::io::Write;
use uwasm::{parse, Context, ParserError, execute_function, VmContext, ImportedFunc, ByteStr};

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, #[allow(unused)] args: Arguments) {
        std::io::stdout().write_fmt(args).unwrap()
    }

    fn ticks(&self) -> u64 {
        0
        // unsafe { _rdtsc() }
    }
}

fn main() -> Result<(), ParserError> {
    let Some(path) = std::env::args_os().nth(1) else {
        panic!("missing path to .wasm file")
    };
    let runs = std::env::args()
        .nth(2)
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    let content = std::fs::read(path).expect("read file");

    let module = parse(&content, &mut MyCtx)?;
    let mut imports: Vec<ImportedFunc> = Vec::new();
    for name in module.get_imports() {
        imports.push(match name.as_bytes() {
            b"print" => |stack, memory| unsafe {
                let size = stack.pop_i32().unwrap() as usize;
                let ptr = stack.pop_i32().unwrap() as usize;
                let s = ByteStr::from_bytes(&memory[ptr..][..size]);
                println!(">>> PRINT FROM VM: {:?}", s);
                stack.push_i32(0);
            },
            _ => todo!("{:?}", name),
        });
    }

    let mut globals = Vec::new();
    // stack pointer
    globals.extend_from_slice(&0x8000u64.to_ne_bytes());

    let started = std::time::Instant::now();
    let mut ctx = VmContext::new();
    for n in 0u32..runs {
        let mut mem = [0u8; 0x8000];
        println!(">>> Executing entry function");
        let res = execute_function::<(u32,), u32>(&mut ctx, &module, b"entry".into(), (987654321,), &mut mem, &mut globals, &imports, &mut MyCtx).unwrap();
        assert_eq!(res, 0);
    }
    println!("time = {:?}/execution", started.elapsed() / runs);

    println!("{:?}", ctx.profile());

    Ok(())
}
