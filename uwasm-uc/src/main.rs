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
use esp_hal::gpio::{AnyOutput, GpioPin, Output};
use esp_hal::system::SystemControl;
use esp_hal::timer::systimer::SystemTimer;
use uwasm::{Context, parse, VmContext, execute_function, ImportedFunc};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

struct MyCtx<'io> {
    led: AnyOutput<'io>,
    delay: Delay,
}

impl Context for MyCtx<'_> {
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

    let mut ctx = MyCtx {
        led: AnyOutput::new(io.pins.gpio18, Level::High),
        delay: Delay::new(&clocks),
    };

    let module = parse(include_bytes!("../../hello_led.wasm"), &mut ctx).expect("parse module");
    let mut imports: Vec<ImportedFunc<MyCtx>> = Vec::new();

    let delay = Delay::new(&clocks);

    for name in module.get_imports() {
        imports.push(match name.as_bytes() {
            b"sleep_ms" => |ctx, stack, memory| {
                let sleep = stack.pop_u32().unwrap();
                ctx.delay.delay_millis(sleep);
                println!(">>> sleeping for {sleep} ms");
            },
            b"set_output" => |ctx, stack, memory| {
                let state = stack.pop_u32().unwrap();
                let pin = stack.pop_u32().unwrap();

                match state {
                    0 => ctx.led.set_low(),
                    1 => ctx.led.set_high(),
                    _ => unimplemented!(),
                }
                println!(">>> setting pin {pin} to {state}")
            },
            _ => todo!("{:?}", name),
        });
    }

    let mut globals = Vec::new();
    // stack pointer
    globals.extend_from_slice(&32u64.to_ne_bytes());
    let mut mem = [0u8; 32];

    let mut vm_ctx = VmContext::new();
    loop {
        let start = SystemTimer::now();
        for i in 0..100 {
            let _ = execute_function::<MyCtx, (u32, ), u32>(&mut vm_ctx, &module, b"entry".into(), (12u32, ), &mut mem, &mut globals, &imports, &mut ctx);
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