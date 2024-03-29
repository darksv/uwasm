#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[export_name = "factorial"]
pub fn factorial(n: u32, acc: u32) -> u32 {
    if n == 0 {
        acc
    } else {
        factorial(n - 1, n * acc)
    }
}

// rustc --target=wasm32-unknown-unknown foo.rs -O -C panic=abort