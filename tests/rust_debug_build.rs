#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry(n: u32) {
    let mut i = 0;
    while i < 10 {
        i += 1;
    }
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort -C opt-level=0