#![no_std]
#![no_main]

use core::ffi::CStr;
use core::panic::PanicInfo;

#[panic_handler]
unsafe fn panic(_: &PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[repr(C)]
struct Str {
    data: *const u8,
    len: usize,
}

extern "C" {
    fn print(s: Str);
}

fn to_string_u32(num: u32, buffer: &mut [u8]) -> &str {
    let mut num = num;
    let max_digits = buffer.len();
    let mut pos = max_digits;

    if num == 0 {
        if max_digits > 0 {
            buffer[0] = b'0';
            return core::str::from_utf8(&buffer[0..1]).unwrap();
        } else {
            return "";
        }
    }

    while num > 0 && pos > 0 {
        pos -= 1;
        buffer[pos] = (num % 10) as u8 + b'0';
        num /= 10;
    }

    let start = pos;
    let result = &buffer[start..max_digits];

    unsafe { core::str::from_utf8_unchecked(result) }
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry(n: u32) {
    let mut buffer = [0u8; 10];
    let number_string = to_string_u32(n, &mut buffer);
    unsafe { print(Str {
        data: number_string.as_ptr(),
        len: number_string.len(),
    }) }
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort