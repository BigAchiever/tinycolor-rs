//! WebAssembly transport for the RPC protocol.
//!
//! This exists so the *unmodified* upstream test suite can drive the Rust
//! port in-process from Node. The boundary is deliberately one function
//! taking and returning a JSON string: keeping it narrow means the shim,
//! not the bindings, carries the API shape, and the same protocol is reused
//! by the native CLI for fuzzing and benchmarks.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn dispatch(request: &str) -> String {
    tinycolor_core::rpc::dispatch(request)
}
