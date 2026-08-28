//! Recognition of text in a raster image, producing a layout.
//!
//! This module wraps the OCR engine behind an interface that suits every
//! target the crate supports: models arrive as bytes rather than paths, and
//! images arrive as an RGB(A) pixel buffer rather than a file. Loading those
//! bytes is the caller's problem, which keeps the filesystem out of the
//! library.
