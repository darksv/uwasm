#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::{Arguments, Write};
use core::result;

use esp_backtrace as _;
use embassy_executor::Spawner;
use esp_println::println;
use esp_hal::{
    clock::ClockControl, gpio::IO, peripherals::Peripherals, prelude::*,
    systimer::SystemTimer,
};
use uwasm::{Context, evaluate, parse, VmContext, execute_function, ByteStr, VmStack, ImportedFunc};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, args: Arguments) {
        _ = esp_println::Printer.write_fmt(args);
    }

    fn ticks(&self) -> u64 {
        SystemTimer::now()
    }
}

static mut IDX: u32 = 0;

#[main]
async fn main(_spawner: Spawner) {
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let _clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    init_heap();

    let module = parse(include_bytes!("../../call_external.wasm"), &mut MyCtx).expect("parse module");
    let mut imports: Vec<ImportedFunc> = Vec::new();
    for name in module.get_imports() {
        imports.push(|stack| unsafe {
            stack.push_i32(IDX as i32);
            IDX += 1;
        });
    }

    let mut ctx = VmContext::new();
    loop {
        ctx.reset_profile();
        for i in 0..100 {
            let _ = execute_function::<(u32,), u32>(&mut ctx, &module, b"entry".into(), (12u32,), &[], &imports, &mut MyCtx);
        }
        println!("{:?}", ctx.profile());
    }
}

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 2048;
    static mut HEAP: core::mem::MaybeUninit<[u8; HEAP_SIZE]> = core::mem::MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr().cast(), HEAP_SIZE);
    }
}