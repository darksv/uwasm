#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::{Arguments, Write};

use esp_backtrace as _;
use esp_println::println;
use esp_hal::{
    clock::ClockControl, peripherals::Peripherals, prelude::*,
    gpio::Io, gpio::Level,
    delay::Delay,
};
use esp_hal::gpio::{AnyOutput};
use esp_hal::system::SystemControl;
use esp_hal::timer::systimer::SystemTimer;
use uwasm::{Environment, parse, VmContext, execute_function, ImportedFunc, init_globals, ByteStr, init_memory};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

struct MyEnv<'io> {
    led1: AnyOutput<'io>,
    led2: AnyOutput<'io>,
    delay: Delay,
}

impl Environment for MyEnv<'_> {
    fn write_fmt(&mut self, args: Arguments) {
        _ = esp_println::Printer.write_fmt(args);
    }

    #[inline(always)]
    fn ticks(&self) -> u64 {
        SystemTimer::now()
    }
}

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    init_heap();

    let mut env = MyEnv {
        led1: AnyOutput::new(io.pins.gpio18, Level::High),
        led2: AnyOutput::new(io.pins.gpio19, Level::High),
        delay: Delay::new(&clocks),
    };

    let module = parse(include_bytes!("../../target/wasm32-unknown-unknown/release/app-example.wasm"), &mut env).expect("parse module");
    let mut imports: Vec<ImportedFunc<MyEnv>> = Vec::new();

    for name in module.get_imports() {
        imports.push(match name.as_bytes() {
            b"halt" => |_, stack, memory| {
                println!(">>> !!!APPLICATION HALTED!!!");
            },
            b"print" => |env, stack, memory| {
                let size = stack.pop_i32().unwrap() as usize;
                let ptr = stack.pop_i32().unwrap() as usize;
                let s = ByteStr::from_bytes(&memory[ptr..][..size]);
                println!(">>> PRINT FROM VM: {:?}", s);
            },
            b"sleep_ms" => |env, stack, _memory| {
                let sleep = stack.pop_u32().unwrap();
                env.delay.delay_millis(sleep);
                println!(">>> sleeping for {sleep} ms");
            },
            b"set_output" => |env, stack, _memory| {
                let state = stack.pop_u32().unwrap();
                let pin = stack.pop_u32().unwrap();

                match (state, pin) {
                    (0, 0) => env.led1.set_low(),
                    (1, 0) => env.led1.set_high(),
                    (0, 1) => env.led2.set_low(),
                    (1, 1) => env.led2.set_high(),
                    _ => unimplemented!(),
                }
                println!(">>> setting pin {pin} to {state}")
            },
            _ => todo!("{:?}", name),
        });
    }

    let mut globals = Vec::new();
    init_globals(&mut globals, &module).unwrap();

    let mut vm_ctx = VmContext::new();
    let mut mem = [0u8; 1024];
    init_memory(&mut mem, &module).unwrap();
    loop {
        let start = SystemTimer::now();
        for _ in 0..10 {
            println!("Executing entry function...");
            let result = execute_function::<MyEnv, (u32, ), u32>(&mut vm_ctx, &module, b"entry".into(), (12u32, ), &mut mem, &mut globals, &imports, &mut env);
            println!("Result: {:?}", result);
        }
        let elapsed = SystemTimer::now() - start;
        println!("ticks: {elapsed}");
    }
}

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 2048;
    static mut HEAP: core::mem::MaybeUninit<[u8; HEAP_SIZE]> = core::mem::MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr().cast(), HEAP_SIZE);
    }
}