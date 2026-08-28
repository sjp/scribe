//! Turning a layout into an output format.
//!
//! A renderer is pure: it takes a layout, its own options and optional access
//! to the source image, and returns a string or bytes. Nothing is written and
//! nothing is fetched. Built-in renderers cover JSON, SVG and user-supplied
//! templates; the SVG default produces an image that looks exactly like the
//! original with a transparent but selectable text layer over it.
