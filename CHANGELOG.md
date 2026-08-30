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
  adjustment and separator elements. Nothing on the root element stands between
  a screen reader and that layer: no ARIA role is written unless one is asked
  for, so the `<text>` elements are reached and read in the order they were
  written, `role=img` announces the document as a picture carrying a label
  instead — the caller's own, or the recognised text cut at a word boundary to
  a length that can be read out — and `role=group` reads the text with a
  boundary around it that can be stepped past. Words a layout scores below a
  threshold are left out, and a word with no score at all is kept unless it is
  asked to go the same way; the recogniser scribe reads images with hands back
  no score, so a layout it wrote loses nothing either way. The document opts
  into both colour schemes and draws selected text in the reader's own system
  colours, so that a selection, and the canvas a document opened on its own
  sits on, follow light and dark mode without the document naming a colour for
  either. It is written to be placed inside another document: every class name
  and every id carries a token setting it apart from anything else in the page,
  worked out from the layout, given by the caller or left out; every rule in
  its stylesheet hangs off the root element rather than reaching the page
  around it; the text layer holds its own against whatever that page styles its
  text with; the names and colours it is given are checked rather than escaped;
  a number it cannot be written with — a font scale at zero, a ratio past one,
  more decimals than a coordinate has to give — is refused naming the range it
  fell outside of, rather than quietly clamped or written into a document no
  browser will honour; and the stylesheet can carry a nonce for a page whose
  content security policy asks for one.
- The `template` renderer, writing whatever a Jinja template describes, with
  callers' own templates and values accepted and twelve built in: text layers
  to lay over an image, written as positioned HTML or as SVG; transcripts an
  image points at for a screen reader; a `<figure>` carrying the whole text
  beneath the picture; all of those at once; the layout itself in a
  `<script>`; JSON-LD for a crawler; and `hocr`, `alto`, `markdown`, `text`
  and `alt-text`. A template can ask the SVG renderer for its text layer
  rather than writing a second one, and checks a caller's own values the way
  that renderer checks its options: `css_value`, `css_ident` and
  `css_ident_start` refuse a colour that would close the declaration it is
  written into and open a rule over the whole page, or a name that a selector
  could not carry, saying which value was refused and why. The built-in
  templates put every value they write into a stylesheet, a class or an id
  through them. The ones that go into somebody else's page — the two text
  layers, the figure, the transcript and the embedded layout — build every
  name from `var.class_prefix` and a token settled by `var.scope_mode` and
  `var.scope`, so that two of them in one page share no class name, no id and
  no rule.
- The `scribe` command line tool: `ocr`, `render`, `formats`, `templates` and
  `schema`, over one image or many, with the layout kept beside the output. An
  output that links to its image rather than carrying it points at the image
  from the directory it is written into, so it still finds the picture when it
  is opened from somewhere other than where the run was started; `--link-href`
  says outright where to point instead. `scribe render` with no image to hand
  and nothing said about one writes the text layer on its own rather than
  stopping; a format asked outright for an image it was never given says which
  flags name one, an `--image` whose kind cannot be told from its bytes is
  refused before anything is rendered, and both leave with the status of a
  mistake in the request rather than that of a failed run. One bad file among
  many does not stop the rest: it is named on standard error as it happens,
  every other input is still read, and the run ends with a count of what did
  not come out and a failing status. `--fail-fast` stops at the first failure
  instead, and something wrong with the request itself ends the run where it
  is noticed rather than being repeated for every input.
- WebAssembly bindings packaged for the browser and for Node, with TypeScript
  declarations for the layout model, the renderer options and the rendered
  document.
- Fixture images and golden tests covering every built-in renderer.
- User and developer documentation under `docs/`.

[Unreleased]: https://github.com/sjp/scribe
