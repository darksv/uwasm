#![feature(panic_info_message)]
#![no_std]
#![no_main]

use core::fmt::Write;
use core::time::Duration;
use heapless::String;

#[panic_handler]
unsafe fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut buf: String<100> = String::new();
    if let Some(msg) = info.message() {
        _ = writeln!(&mut buf, "panic: {}", msg);
        api::print(buf.as_str());
    } else {
        api::print("<no info>");
    }
    api::halt();
}

mod api {
    use core::convert::TryInto;
    use core::time::Duration;

    #[derive(Copy, Clone, PartialEq)]
    #[repr(u8)]
    pub enum State {
        Low = 0,
        High = 1,
    }

    impl State {
        pub fn toggle(self) -> Self {
            match self {
                State::High => State::Low,
                State::Low => State::High,
            }
        }
    }

    mod raw {
        #[repr(C)]
        pub(super) struct Str {
            pub(super) data: *const u8,
            pub(super) len: usize,
        }

        extern "C" {
            pub(super) fn set_output(pin: u8, high: bool);
            pub(super) fn sleep_ms(ms: u32);
            pub(super) fn print(s: Str);
            pub(super) fn halt() -> !;
        }
    }

    pub fn set_output(pin: u8, state: State) {
        unsafe { raw::set_output(pin, state == State::High) }
    }

    pub fn sleep(duration: Duration) {
        unsafe { raw::sleep_ms(duration.as_millis().try_into().unwrap()) }
    }

    pub fn print(s: &str) {
        unsafe {
            raw::print(raw::Str {
                len: s.len(),
                data: s.as_ptr(),
            })
        }
    }

    pub fn halt() -> ! {
        unsafe {
            raw::halt()
        }
    }
}

#[inline(never)]
fn foo() {
    let mut s: String<100> = String::new();
    write!(&mut s, "{:?}", 123);
    api::print(core::hint::black_box(s.as_str()));
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry(n: u32) -> u32 {
    use api::State;

    let mut state = State::Low;
    for i in 0..10 {
        api::set_output(0, state);
        api::sleep(Duration::from_millis(10 * i * n as u64));

        api::set_output(1, state);
        api::sleep(Duration::from_millis(20 * i * n as u64));

        state = state.toggle()
    }

    api::print("ok");
    foo();

    0
}

// rustc --target=wasm32-unknown-unknown -C link-args=-z stack-size=512 tests/hello_led.rs