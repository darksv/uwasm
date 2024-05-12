#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

extern "C" {
    fn foo() -> u32;
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry() {
    unsafe { foo() };
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort