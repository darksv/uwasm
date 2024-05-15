#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

extern "C" {
    fn foo(_: u64) -> u32;
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry(n: u32) -> u32 {
    n + unsafe { foo() }
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort