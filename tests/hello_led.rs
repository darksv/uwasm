#![no_std]
#![no_main]

use core::time::Duration;

#[panic_handler]
unsafe fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
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
        extern "C" {
            pub(crate) fn set_output(pin: u8, high: bool);
            pub(crate) fn sleep_ms(ms: u32);
        }
    }

    pub fn set_output(pin: u8, state: State) {
        unsafe { raw::set_output(pin, state == State::High) }
    }

    pub fn sleep(duration: Duration) {
        unsafe { raw::sleep_ms(duration.as_millis().try_into().unwrap()) }
    }
}

#[no_mangle]
#[export_name = "entry"]
pub fn entry(n: u32) -> u32 {
    use api::State;

    let mut state = State::Low;
    loop {
        api::set_output(0, state);
        api::sleep(Duration::from_secs(1));
        state = state.toggle()
    }
}

// rustc --target=wasm32-unknown-unknown foo.rs -C panic=abort