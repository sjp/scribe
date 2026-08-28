//! WebAssembly bindings for scribe.
//!
//! A thin translation layer: JavaScript values in, scribe-core calls,
//! JavaScript values out. Any behaviour worth testing belongs in the core.

use wasm_bindgen::prelude::wasm_bindgen;

/// The version of scribe these bindings were built from.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
