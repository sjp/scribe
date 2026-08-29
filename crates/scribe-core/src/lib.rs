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
//!
//! A [`Layout`](layout::Layout) is the contract between the two halves. It is
//! versioned and serialisable, so recognition can happen once and rendering as
//! often as you like, in another format, in another process, on another day,
//! with no model loaded at all.
//!
//! ```
//! use scribe_core::image_source::ImageSource;
//! use scribe_core::layout::{Layout, Line, Rect, RotatedBox};
//! use scribe_core::render::{Options, RenderError, Renderer, registry};
//!
//! # fn main() -> Result<(), RenderError> {
//! // A layout usually comes from `ocr`; one built by hand renders just the same.
//! let bbox = Rect::new(26.0, 28.0, 231.0, 35.0);
//! let layout = Layout::new(
//!     284,
//!     96,
//!     vec![Line {
//!         text: "Hello World".to_string(),
//!         bbox,
//!         rotated_box: RotatedBox::from_rect(bbox),
//!         words: Vec::new(),
//!         confidence: None,
//!     }],
//! );
//!
//! let registry = registry();
//! let renderer = registry.get("svg").expect("svg is built in");
//! let output = renderer.render(
//!     &layout,
//!     &ImageSource::new(284, 96),
//!     &Options::new().with("image_mode", "none"),
//! )?;
//!
//! assert_eq!(output.mime, "image/svg+xml");
//! assert!(output.as_str().expect("an SVG is text").contains("Hello World"));
//! # Ok(())
//! # }
//! ```
//!
//! Reading an image needs the two trained models, which this crate never
//! fetches: see [`ocr`] for the shape of that call, and [`render`] for the
//! renderers and the options each one takes.

pub mod image_source;
pub mod layout;
pub mod ocr;
pub mod render;
