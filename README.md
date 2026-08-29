# scribe

Text in a raster image is invisible to everything that matters. A screenshot, a
scanned page, a photograph of a sign: the words are there for a person looking
at it and gone for everyone else. Ctrl+F finds nothing. Selecting a phrase and
copying it is impossible. A screen reader has only whatever `alt` text somebody
remembered to write. A search engine indexes the file name.

scribe runs optical character recognition over an image and emits an output in
which that text is machine-readable, searchable and selectable — while the
result still looks exactly like the image it came from. The default output is
an SVG holding the original picture with a transparent text layer over it: the
same image to look at, and real text to a browser, a screen reader and a
crawler.

The recognised text is described once, as a renderer-agnostic
[layout](docs/layout.md), and that layout drives every output: the SVG, plain
JSON, or any format you can write as a template. It ships as a command line
tool, a Rust library and a WebAssembly module.

**Status: early development.** Nothing here is stable yet.

## Install

```sh
cargo install --path crates/scribe-cli
```

The binary is called `scribe`.

## Models

Recognition runs the models published by the
[ocrs](https://github.com/robertknight/ocrs) project, whose engine scribe
builds on. **scribe never downloads them.** The library takes model *bytes*,
never paths or URLs, and the command line takes explicit paths — so nothing
here ever reaches the network on its own.

Fetch them once from
`https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten` and
`https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten`, or run
the script that does it for you:

```sh
scripts/fetch-models.sh
export SCRIBE_DETECTION_MODEL="$PWD/models/text-detection.rten"
export SCRIBE_RECOGNITION_MODEL="$PWD/models/text-recognition.rten"
```

Either environment variables or `--detection-model` and `--recognition-model`
will do; the flags win.

## Quick start

Read an image and write an SVG that looks just like it and is searchable:

```sh
scribe ocr page.png --out page.svg
```

Keep the layout beside it, so the image never has to be read again:

```sh
scribe ocr page.png --out page.svg --layout-json page.layout.json
```

Render that layout again into anything else, with no models loaded and no
recognition run:

```sh
scribe render page.layout.json --format template --opt template=hocr --out page.hocr
scribe render page.layout.json --format template --opt template=text
```

A whole directory at once, and a look at how well the text layer fits:

```sh
scribe ocr scans/*.png --out-dir out/
scribe ocr page.png --debug --out check.svg
```

`scribe formats` lists the output formats and every option each one takes;
`scribe templates` lists the built-in templates; `scribe schema` prints the
JSON Schema of the layout model.

## What comes out

```sh
scribe render page.layout.json --no-image --opt precision=1
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" class="scribe-root" width="284" height="96" viewBox="0 0 284 96" role="img" aria-label="Hello World">
  <style>
    .scribe-root { color-scheme: light dark; }
    .scribe-text { user-select: text; -webkit-user-select: text; white-space: pre; }
    .scribe-text::selection, .scribe-text ::selection { fill: HighlightText; background: color-mix(in srgb, Highlight 35%, transparent); }
  </style>
  <g class="scribe-text" font-family="sans-serif" fill="transparent">
    <text class="scribe-line"><tspan class="scribe-word" x="26" y="55.4" font-size="33" textLength="105" lengthAdjust="spacingAndGlyphs">Hello</tspan> <tspan class="scribe-word" x="146" y="56" font-size="35" textLength="111" lengthAdjust="spacingAndGlyphs">World</tspan></text>
  </g>
</svg>
```

That is the text layer alone, with `--no-image`; by default the picture is
carried in the document beneath it and the output is one self-contained file.

Three formats ship with scribe: `svg`, `json`, and `template` — the last
rendering a Jinja template, of which twelve come built in and any number can be
written. Seven of them are ways of giving an image in a web page text that can
be found, selected, read aloud and indexed: a text layer over it in HTML or
SVG, a transcript only a screen reader reaches, a `<figure>` with the whole
text beneath it, everything at once, the layout itself in a `<script>`, and
JSON-LD for a crawler. The rest are `hocr`, `alto`, `markdown`, `text` and
`alt-text`. Nothing is hard-coded that could be an option: embedding or
linking the image, visible or invisible text, the font, the class names, the
ids, the number of decimals. See [output formats](docs/formats.md).

## In a browser

The whole pipeline compiles to WebAssembly, so a page can read an image and lay
a text layer over it with no server at all:

```js
import init, { Engine, render } from './pkg/web/scribe.js';

await init();
const engine = new Engine(detectionModel, recognitionModel);
const layout = engine.analyzePixels(width, height, 4, pixels);
engine.free();

const overlay = render(layout, 'svg', { image_mode: 'none' }).text;
```

`scripts/build-wasm.sh` builds the package with
[wasm-pack](https://rustwasm.github.io/wasm-pack/), for the browser and for
Node. TypeScript declarations for the layout model and every option ship with
it. See [the WebAssembly package](docs/wasm.md).

## Documentation

- [Output formats](docs/formats.md) — every renderer and every option.
- [The layout model](docs/layout.md) — the data everything else is written
  from, and its coordinate conventions.
- [Writing templates](docs/templates.md) — the context, the filters and the
  built-in templates.
- [The WebAssembly package](docs/wasm.md) — building it and calling it.
- [Development](docs/development.md) — the workspace, the tests and CI.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you explicitly state
otherwise, any contribution intentionally submitted for inclusion in this
project by you, as defined in the Apache-2.0 licence, shall be dual licensed as
above, without any additional terms or conditions.
