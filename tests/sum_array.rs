#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[export_name = "sum_slice"]
pub fn sum_slice(arr: &[f32]) -> f32 {
    arr.iter().sum()
}

// rustc --target=wasm32-unknown-unknown foo.rs -O -C panic=abort