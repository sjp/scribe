# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic
Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The `Layout` model: a versioned, JSON-serialisable description of the text
  found in an image, at line, word and character granularity, with an
  axis-aligned and an oriented box for each. A JSON Schema is generated from it
  and checked in at `schema/layout.schema.json`. Every document says which
  version of the model it was written against, and one written by a newer
  scribe than the build reading it is refused, naming both versions, rather
  than being read as something it may not be.
- Recognition over [ocrs](https://github.com/robertknight/ocrs), taking model
  bytes and a pixel buffer rather than paths and files, so it runs anywhere the
  library does. Models are never downloaded.
- A renderer abstraction and a registry, with every renderer publishing the
  options it takes so that a command line or a JavaScript caller can offer them
  without knowing what they mean.
- The `json` renderer, writing the layout itself, or — asked to embed the
  image — a self-contained document that describes a picture and holds it.
  Such a document reads back like any other: the layout comes out as it went
  in and the picture comes with it, so `scribe render` and the WebAssembly
  `render` draw it again with no image named alongside. An image that is
  named wins over the carried one.
- The `svg` renderer, writing the image with a selectable text layer over it:
  transparent, visible or drawn with the boxes it came from; the image
  embedded, linked or left out; and the fit of the layer to the glyphs beneath
  it settled by font size mode, baseline mode, per-character placement, length
  adjustment and separator elements. Words a layout scores below a threshold
  are left out, and a word with no score at all is kept unless it is asked to
  go the same way; the recogniser scribe reads images with hands back no
  score, so a layout it wrote loses nothing either way. The document opts into
  both colour schemes and draws selected text in the reader's own system
  colours, so that a selection, and the canvas a document opened on its own
  sits on, follow light and dark mode without the document naming a colour for
  either. It is written to be placed inside another document: every class name
  and every id carries a token setting it apart from anything else in the
  page, worked out from the layout, given by the caller or left out; every
  rule in its stylesheet hangs off the root element rather than reaching the
  page around it; the text layer holds its own against whatever that page
  styles its text with; the names and colours it is given are checked rather
  than escaped; and the stylesheet can carry a nonce for a page whose content
  security policy asks for one.
- The `template` renderer, writing whatever a Jinja template describes, with
  callers' own templates and values accepted and twelve built in: text layers
  to lay over an image, written as positioned HTML or as SVG; transcripts an
  image points at for a screen reader; a `<figure>` carrying the whole text
  beneath the picture; all of those at once; the layout itself in a
  `<script>`; JSON-LD for a crawler; and `hocr`, `alto`, `markdown`, `text`
  and `alt-text`. A template can ask the SVG renderer for its text layer
  rather than writing a second one.
- The `scribe` command line tool: `ocr`, `render`, `formats`, `templates` and
  `schema`, over one image or many, with the layout kept beside the output.
- WebAssembly bindings packaged for the browser and for Node, with TypeScript
  declarations for the layout model, the renderer options and the rendered
  document.
- Fixture images and golden tests covering every built-in renderer.
- User and developer documentation under `docs/`.

[Unreleased]: https://github.com/sjp/scribe
