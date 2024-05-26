# uWasm
This project is a runtime for [WebAssembly](https://webassembly.github.io/spec/core/index.html) modules, suitable for use as a loader for user apps in firmwares running on microcontrollers like ESP32. 

## Motivation
It is being developed within [this daily coding challenge](https://100commitow.pl/). For me, this project serves as an opportunity to learn more about WebAssembly, delve deeper into low level programming, and... use more Rust :)

Hopefully, something useful will come out of this project.

## Project scope
- [ ] parser of WebAssembly binary representation;
  - basic subset of Wasm (support for SIMD, threads and other more advanced features is not planned)
- [ ] bytecode interpreter;
- [ ] native API to call from inside the virtual machine;
- [ ] optional: JIT/AOT compilation using copy-and-patch method[^1].

## Project goals
- low memory footprint;
- reasonable performance.

## Additional ideas
- introduce continuous integration (via Github Actions) on real hardware to ensure proper performance on target platforms;
- experiment with process scheduling.

## Contributing
When writing commit messages please follow [semantic commit messages](https://gist.github.com/joshbuchea/6f47e86d2510bce28f8e7f42ae84c716) guidelines.

## Useful tools and resources
- https://webassembly.github.io/wabt/demo/wat2wasm/ - online compiler for Wasm text representation
- https://webassembly.github.io/spec/core/index.html - WebAssembly specification

[^1]: https://dl.acm.org/doi/10.1145/3485513

## Compile and run simple test program on VM
> rustup add wasm32-unknown-unknown
> rustc --target=wasm32-unknown-unknown tests/call_print.rs -C panic=abort -O
> cargo run --bin uwasm-perf -- call_print.wasm 1
