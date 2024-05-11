#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;

use core::fmt::Arguments;
use core::result;

use esp_backtrace as _;
use embassy_executor::Spawner;
use esp_println::println;
use esp_hal::{
    clock::ClockControl, gpio::IO, peripherals::Peripherals, prelude::*,
    systimer::SystemTimer,
};
use uwasm::{Context, evaluate, parse, VmContext, execute_function, ByteStr};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

struct MyCtx;

impl Context for MyCtx {
    fn write_fmt(&mut self, _args: Arguments) {}
}

#[main]
async fn main(_spawner: Spawner) {
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let _clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    init_heap();

    let module = parse(include_bytes!("../../tests/factorial.wasm"), &mut MyCtx).expect("parse module");

    loop {
        let start = SystemTimer::now();
        let result = execute_function::<(f64,), f64>(&module, b"fac".into(), (10.0f64,), &[], &mut MyCtx);
        let elapsed = SystemTimer::now() - start;
        println!("calculated: {result:?} | ticks: {elapsed}");
    }
}

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 2048;
    static mut HEAP: core::mem::MaybeUninit<[u8; HEAP_SIZE]> = core::mem::MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr().cast(), HEAP_SIZE);
    }
}