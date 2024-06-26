extern crate core;

use std::fmt::Arguments;
use std::io::Write;
use uwasm::{parse, Environment, ParserError, execute_function, VmContext, ImportedFunc, ByteStr, init_globals, init_memory};

struct MyEnv;

impl Environment for MyEnv {
    fn write_fmt(&mut self, args: Arguments) {
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

    let module = parse(&content, &mut MyEnv)?;
    let mut imports: Vec<ImportedFunc<MyEnv>> = Vec::new();
    for name in module.get_imports() {
        imports.push(match name.as_bytes() {
            b"halt" => |_, stack, memory| {
                println!(">>> !!!APPLICATION HALTED!!!");
            },
            b"print" => |_, stack, memory| {
                let size = stack.pop_i32().unwrap() as usize;
                let ptr = stack.pop_i32().unwrap() as usize;
                let s = ByteStr::from_bytes(&memory[ptr..][..size]);
                println!(">>> PRINT FROM VM {size} {ptr}: {:?}", s);
            },
            b"sleep_ms" => |_, stack, _memory| {
                let sleep = stack.pop_u32().unwrap();
                println!(">>> sleeping for {sleep} ms")
            },
            b"set_output" => |_, stack, _memory| {
                let state = stack.pop_u32().unwrap();
                let pin = stack.pop_u32().unwrap();
                println!(">>> setting pin {pin} to {state}")
            },
            _ => todo!("{:?}", name),
        });
    }

    let mut globals = Vec::new();
    init_globals(&mut globals, &module).unwrap();

    let started = std::time::Instant::now();
    let mut ctx = VmContext::new();
    let mut mem = [0u8; 1024*8];
    init_memory(&mut mem, &module).unwrap();
    for _n in 0u32..runs {
        println!(">>> Executing entry function");
        let res = execute_function::<MyEnv, (u32, ), u32>(&mut ctx, &module, b"entry".into(), (1, ), &mut mem, &mut globals, &imports, &mut MyEnv);
        println!(">>> Result: {:?}", res);
    }
    println!("time = {:?}/execution", started.elapsed() / runs);

    println!("{:?}", ctx.profile());

    Ok(())
}
