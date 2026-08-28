# scribe

scribe runs optical character recognition over a raster image and emits an
output in which the recognised text is machine-readable, searchable and
selectable while the result still looks exactly like the original image. The
recognised text is described once, as a renderer-agnostic layout, and that
layout drives every output: an SVG with a transparent text layer over the
image, plain JSON, or any format you can express as a template. It ships as a
command line tool, a Rust library and a WebAssembly module.

**Status: early development.** Nothing here is stable yet.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you explicitly state
otherwise, any contribution intentionally submitted for inclusion in this
project by you, as defined in the Apache-2.0 licence, shall be dual licensed as
above, without any additional terms or conditions.
