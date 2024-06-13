rustc --target=wasm32-unknown-unknown -C "link-args=-z stack-size=256" tests/hello_led.rs -O
cargo +nightly-2024-06-01 run -r --bin uwasm-uc --target riscv32imc-unknown-none-elf