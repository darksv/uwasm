rustc +nightly --target=wasm32-unknown-unknown tests/simple.rs -C panic=abort -C opt-level=3
cargo +nightly run --bin uwasm-test simple.wasm
wasm2wat simple.wasm