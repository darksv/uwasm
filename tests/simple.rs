#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[export_name = "foo"]
pub fn factorial(n: u32, acc: u32) -> u32 {
    sum(n, acc) / 2
}

#[inline(never)]
fn sum(a: u32, b: u32) -> u32 {
    a + b + 1
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort