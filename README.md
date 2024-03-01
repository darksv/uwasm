# uWasm
This project is a runtime for [WebAssembly](https://webassembly.github.io/spec/core/index.html) modules, suitable for use as a loader for user apps in firmwares running on microcontrollers like ESP32.

## Project scope
- [ ] parser of WebAssembly binary representation;
  - basic subset of Wasm (support for SIMD, threads and other more advanced features is not planned)
- [ ] bytecode interpreter;
- [ ] native API to call from inside the virtual machine;
- [ ] optional: JIT/AOT compilation using copy-and-patch method[^1]

## Project goals
- low memory footprint
- reasonable performance

[^1]: https://dl.acm.org/doi/10.1145/3485513