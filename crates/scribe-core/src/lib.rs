//! The engine behind scribe: turn a raster image into a description of the
//! text it contains, then render that description into any output format.
//!
//! The pipeline is
//!
//! ```text
//! pixels ──► ocr ──► Layout ──► render ──► String / bytes
//! ```
//!
//! Every stage is free of filesystem, network and terminal access so that the
//! whole crate also builds for `wasm32-unknown-unknown`. Callers supply model
//! data and pixels as bytes and receive rendered output as bytes.

pub mod image_source;
pub mod layout;
pub mod ocr;
pub mod render;
