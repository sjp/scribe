# The layout model

A **layout** is everything scribe knows about the text in one image. It is
what recognition produces and the only thing any renderer reads, so it is also
the contract between the two: keep a layout and you can render it again later,
into any format, without the models and without the image.

```text
pixels ──► ocr ──► Layout ──► render ──► document
```

It is versioned, it round-trips through JSON, and a [JSON
Schema](#json-schema) describes it for consumers in other languages.

## Shape

```jsonc
{
  "version": 1,
  "image": { "width": 284, "height": 96 },
  "lines": [
    {
      "text": "Hello World",
      "bbox": { "x": 26.0, "y": 28.0, "width": 231.0, "height": 35.0 },
      "rotated_box": { "cx": 141.5, "cy": 45.5, "width": 231.0, "height": 35.0, "angle_deg": -0.0 },
      "words": [
        {
          "text": "Hello",
          "bbox": { "x": 26.0, "y": 29.0, "width": 105.0, "height": 33.0 },
          "rotated_box": { "cx": 78.5, "cy": 45.5, "width": 105.0, "height": 33.0, "angle_deg": -0.0 },
          "chars": [
            { "text": "H", "bbox": { "x": 26.0, "y": 29.0, "width": 32.0, "height": 33.0 }, "confidence": null }
          ],
          "confidence": null
        }
      ],
      "confidence": null
    }
  ]
}
```

- `version` is the version of the model the document was written against. It
  changes only when the meaning or the shape of the model changes, so a
  consumer can decide whether it understands a document before reading it.
  This build writes version 1. It is required: a document that leaves it out
  is refused. A document that names a higher version was written by a newer
  scribe and is refused too, naming both versions, rather than being read as
  something it may not be. Anything at or below the current version is read,
  and stands or falls on whether its fields match the model.
- `image` is the raster the coordinates refer to.
- `lines` are the lines of text, in reading order.

Text is described at three granularities. A `Line` has the text of the whole
line, its two boxes, and its `words`; a `Word` has its text, its two boxes, and
its `chars`; a `Char` has one grapheme and an axis-aligned box. Each level
carries its text spelled out, so a renderer or a template needs no logic to
join anything back together.

`words` may be empty for a line the recogniser gave no words for, and `chars`
may be empty for a word it gave no character positions for — recognition run
with `include_chars` off drops them deliberately, since they are the bulk of a
serialised layout. A renderer that wants them has to cope with their absence.

`confidence` is how sure the recogniser is, from 0 to 1, and is `null` when it
does not say. The engine scribe reads images with never says, so every layout
scribe produced carries `null` at every level; a layout written by some other
tool, or edited by hand, may carry real scores, and the
[`min_confidence`](formats.md#svg) option is there for those.

## Coordinates

Coordinates are **image pixels**, with the origin at the **top left** and **y
increasing downwards** — the same frame as a `<canvas>`, an `<img>` and an SVG
`viewBox` of `0 0 width height`. Nothing is normalised and nothing is scaled:
a layout read from a 284 by 96 image describes that image and no other.

Every text item carries both kinds of box, because renderers want different
ones:

- `bbox` is axis-aligned: `x`, `y` of its top-left corner, and its `width` and
  `height`. This is what an HTML `left`/`top`/`width`/`height` wants.
- `rotated_box` is oriented: `width` by `height` centred on `(cx, cy)` and
  turned by `angle_deg`. This is what an SVG `rotate()` or a CSS `transform`
  wants, and it is the box that actually follows text set at an angle.

For level text the two describe the same rectangle. For turned text `bbox` is
the smallest axis-aligned rectangle containing `rotated_box`, so it is larger
than the text and, on its own, a poor fit.

## Rotation

`angle_deg` is the rotation of the box's width axis away from the positive x
axis, measured in degrees and **positive clockwise as seen on screen**. That is
what SVG's `rotate()` and CSS's `rotate()` both do in this y-down frame, so an
angle can be passed to either without a sign change. It is normalised to
`(-180, 180]`.

```text
                    angle_deg = -90
                          ▲
                          │
                          │
  angle_deg = 180   ◄─────┼─────►   angle_deg = 0
                          │
                          │
                          ▼
                    angle_deg = +90
```

Each arrow points along the box's width axis — the direction its text reads.
Because y increases downwards, `+90` points down the image: text reading from
top to bottom, as a label down the side of a chart does. `-90` points up it. A
line a few degrees off level, as a photographed page is, has an `angle_deg` of
a few degrees either way.

`RotatedBox::corners()` gives the four corners in image pixels, starting at the
corner that is the top left before rotation and going clockwise on screen — the
order an SVG `<polygon points="…">` wants. `to_rect()` is the axis-aligned
bounds. `is_axis_aligned(tolerance_deg)` holds at every multiple of 90 degrees,
so a box that is axis-aligned may still be a quarter turn from upright, in
which case `to_rect()` swaps its width and height.

## JSON

Field names are `snake_case` and the same in every language. **Unknown fields
are rejected**, so a document written against a different shape of the model
fails loudly rather than losing data quietly. Missing `words`, `chars` and
`confidence` are the only omissions accepted on input; output always spells
them out.

One field beyond the model is accepted: `image_data_uri`, the image the layout
was read from, as a base64 `data:` URI. The [`json`](formats.md#json) format
writes it when it is asked to embed the image, which makes a document that
describes a picture and holds it. It is not part of a layout — nothing writes
it for a layout that has none — so a reader takes the two apart.

`Layout::from_json` and `LayoutDocument::from_json` are the checked path:
they are where the version is looked at, and they fail with a `LayoutError`
that tells a version it cannot read apart from JSON it cannot parse.
Deserialising a `Layout` straight through serde reads whatever version it
finds; a caller doing that calls `Layout::check_version()` itself. The
JavaScript `render` checks whatever layout it is handed.

In Rust:

```rust
use scribe_core::layout::{Layout, LayoutDocument};

let layout = Layout::empty(800, 600);
let json = layout.to_json()?;               // to_json_pretty() for a readable one
assert_eq!(Layout::from_json(&json)?, layout);
assert_eq!(layout.text(), "");              // every line, joined with newlines

// The same document, with the picture it may carry kept rather than dropped.
let document = LayoutDocument::from_json(&json)?;
assert_eq!(document.layout, layout);
assert_eq!(document.image, None);           // the media type and bytes, when carried
```

At the command line, `--layout-json` writes the layout beside the output of an
`ocr` run, `--format json` writes it as the output, and `scribe render` reads
one back:

```sh
scribe ocr page.png --out page.svg --layout-json page.layout.json
scribe render page.layout.json --format template --opt template=hocr
```

A document that carries its picture renders on its own, with no image named
beside it:

```sh
scribe ocr page.png --format json --opt include_image=true -o page.json
scribe render page.json -o page.svg
```

In JavaScript the same holds: a layout with an `image_data_uri` supplies the
image when `render` is passed none.

## JSON Schema

The schema is generated from the Rust types and checked in at
[`schema/layout.schema.json`](../schema/layout.schema.json). A test regenerates
it and fails if it has drifted, so it always describes the build beside it.

Use it to generate types in another language, or to validate a layout that has
been somewhere else and back before rendering it. The same schema is available
from the tools themselves:

```sh
scribe schema
```

```js
scribe.layoutSchema();
```

TypeScript declarations for the whole model ship with the WebAssembly package,
so a JavaScript caller needs nothing generated at all. See
[the WebAssembly package](wasm.md).
