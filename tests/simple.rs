#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

include!("simple_code.rs");

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort