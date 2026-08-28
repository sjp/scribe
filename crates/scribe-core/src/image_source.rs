//! What a renderer knows about the raster image a layout came from.
//!
//! Renderers differ in what they need: one embeds the original image, another
//! links to it, another ignores it entirely and emits text alone. This module
//! describes the source — its pixel dimensions, its media type and, when the
//! caller can provide them, its encoded bytes — so that each renderer can take
//! only what it uses.
