rustc --target=wasm32-unknown-unknown -C "link-args=-z stack-size=256" tests/hello_led.rs
cargo +nightly run -r --bin uwasm-uc --target riscv32imc-unknown-none-elf