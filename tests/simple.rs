#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[export_name = "foo"]
pub fn factorial(n: u32, acc: u32) -> u32 {
    let f = |x: i32| -x * x + acc as i32;
    let a = -1_000;
    let b = 1_000;

    let dt =((b - a) / n as i32);

    let mut sum = 0;
    let mut xx = 0;
    for x in 0..n {
        sum += f(xx) + f(xx + dt);
        xx += dt;
    }
    (sum / 1000) as u32
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort