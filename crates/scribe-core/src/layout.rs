//! The renderer-agnostic description of the text found in an image.
//!
//! A layout is the contract between the OCR pipeline and every renderer, and
//! between this crate and its callers: it is versioned, serialisable, and the
//! only thing a renderer needs in order to do its job. Text is described at
//! three granularities — line, word and character — and each item carries both
//! an axis-aligned bounding box and an oriented box, in image pixel
//! coordinates with the origin at the top left and y increasing downwards.
