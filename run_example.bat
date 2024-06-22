cargo build -r --target=wasm32-unknown-unknown --bin app-example
cargo run --bin uwasm-perf -- target/wasm32-unknown-unknown/release/app-example.wasm