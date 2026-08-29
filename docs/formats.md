# Output formats

Everything scribe writes comes from one place: a [layout](layout.md), the
description of the text found in an image. A *format* — or renderer — turns
that layout into a document. Three ship with scribe:

| Format | Writes | Media type |
| --- | --- | --- |
| `svg` | The image with a selectable text layer over it | `image/svg+xml` |
| `json` | The layout itself | `application/json` |
| `template` | Whatever a [Jinja template](templates.md) describes | the template's own |

A format is chosen with `--format` at the command line, with the second
argument to `render()` in JavaScript, and by name from the registry in Rust.

## Setting options

No format's options are known to anything but the format itself, so they are
set by name:

```sh
scribe ocr page.png --opt text_mode=visible --opt precision=3
```

```js
scribe.render(layout, 'svg', { text_mode: 'visible', precision: 3 });
```

```rust
use scribe_core::render::{Options, registry};

let registry = registry();
let renderer = registry.get("svg").expect("svg is built in");
let options = Options::new().with("text_mode", "visible").with("precision", 3_i64);
```

A value of the wrong kind is read as the kind the option takes, so `"3"` typed
at a terminal and `3` passed from a program mean the same thing, and `true`,
`yes`, `on` and `1` all mean true. An option a format does not take, or a
value it cannot read, is an error naming the option rather than something
quietly ignored.

A few flags stand for the options people reach for most — `--visible`,
`--debug`, `--embed`, `--link`, `--no-image`, `--min-confidence` — and
`--opt` wins over them. A flag whose option the chosen format does not take is
refused in the words it was written in.

`scribe formats` prints the same tables as below from the build in front of
you, and `describeOptions(format)` returns them to JavaScript as data.

## `svg`

The default. The document holds the original raster and, over it, one `<text>`
element per line carrying one `<tspan>` per word, each placed on the pixels it
was recognised from. The text is transparent by default, so the result looks
exactly like the image while a browser can select it, search it and read it
aloud.

```sh
scribe render page.layout.json --no-image --opt precision=1
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="284" height="96" viewBox="0 0 284 96" role="img" aria-label="Hello World">
  <style>
    .scribe-text { user-select: text; -webkit-user-select: text; white-space: pre; }
    .scribe-text::selection, .scribe-text ::selection { fill: #000; background: rgba(0, 90, 255, 0.35); }
  </style>
  <g class="scribe-text" font-family="sans-serif" fill="transparent">
    <text class="scribe-line"><tspan class="scribe-word" x="26" y="55.4" font-size="33" textLength="105" lengthAdjust="spacingAndGlyphs">Hello</tspan> <tspan class="scribe-word" x="146" y="56" font-size="35" textLength="111" lengthAdjust="spacingAndGlyphs">World</tspan></text>
  </g>
</svg>
```

An SVG loaded through `<img src="page.svg">` is treated as a picture and none
of this works. The document has to be inline in the page, or in an `<object>`
or `<iframe>`.

### What the text layer looks like: `text_mode`

- `invisible` — the text is transparent and the output looks exactly like the
  image. Selecting it shows the selection colour, so a reader can see what
  they have.
- `visible` — the text is drawn in `text_fill` over the image. Useful for
  seeing how well the layer fits, and for a "text view" of a picture.
- `debug` — the text is drawn and the boxes it came from are outlined, lines in
  `debug_line_stroke` and words in `debug_word_stroke`. This is the mode to
  look at when a layer sits wrongly.

### Where the image comes from: `image_mode`

- `embed` — the image is carried in the document as a `data:` URI, so the SVG
  is one self-contained file. Needs the image's bytes and media type.
- `link` — the document points at the image with `<image href="…">`, which
  keeps it small but ties it to where the image is. Needs an href; the command
  line uses the path as it was written, or whatever `--link-href` says.
- `none` — no image at all, leaving a text layer to be laid over an `<img>`
  elsewhere. This is the mode a browser overlay wants.

Asking to embed an image whose bytes were not supplied, or to link to one with
no href, is an error rather than a silently empty document.

### Fitting the layer to the glyphs

The recogniser gives a box per word and, when it can, a box per character.
How closely the text layer follows the glyphs under it is a choice:

- `font_size_mode` reads a box's height either as the whole font size
  (`box_height`) or as the height of a capital letter set in it
  (`cap_height`, scaled by `cap_height_ratio`). Text written in capitals has
  no ascenders or descenders, so its boxes are cap height and the default
  makes it too large.
- `baseline_mode` places the baseline either at a fixed fraction of the height
  above the bottom of the box (`ratio`, using `baseline_ratio`) or at the
  bottom itself for a line no character of which descends (`estimate`).
- `char_positions` sets every character at the pixels it was read from instead
  of stretching a whole word across its box. Truer, and larger.
- `length_adjust` decides whether fitting a word to its box stretches the gaps
  alone (`spacing`) or the glyphs as well (`spacingAndGlyphs`).
- `space_mode` and `line_break_mode` give the gap between two words, and the
  break between two lines, a `<tspan>` of their own carrying the character, so
  that copying several lines out of the document yields what the image says
  rather than one run-on line.

<!-- options: svg -->
| Option | Takes | Default | What it does |
| --- | --- | --- | --- |
| `text_mode` | `invisible`, `visible`, `debug` | `invisible` | Whether the text layer is transparent, drawn over the image, or drawn with the boxes it came from. |
| `image_mode` | `embed`, `link`, `none` | `embed` | Whether the image is carried in the document, referenced by its path or URL, or left out. |
| `font_family` | text | `sans-serif` | The CSS font family the text layer is set in. |
| `font_size_scope` | `word`, `line` | `word` | Whether each word takes its font size from its own box or from the line's. |
| `font_scale` | a number | `1` | What to multiply a box's height by to get its font size. |
| `font_size_mode` | `box_height`, `cap_height` | `box_height` | Whether a box's height is the whole font size or the height of a capital letter set in it, which suits text written in capitals. |
| `cap_height_ratio` | a number | `0.7` | How much of the font size a capital letter stands, as a fraction, when the size is worked out from cap height. |
| `baseline_mode` | `ratio`, `estimate` | `ratio` | Whether the baseline is always the fixed fraction above a box's bottom, or the bottom itself for a line whose characters do not fall below it. |
| `baseline_ratio` | a number | `0.2` | How far above the bottom of a box its baseline sits, as a fraction of the height. |
| `length_adjust` | `spacingAndGlyphs`, `spacing` | `spacingAndGlyphs` | Whether fitting a word to its box stretches the gaps alone or the glyphs as well. |
| `char_positions` | `true` or `false` | `false` | Set each character at the pixels it was read from, where the recogniser said where they are, rather than stretching a whole word to fill its box. |
| `space_mode` | `none`, `tspan` | `none` | Whether the gap between two words holds a `<tspan>` of its own carrying the space, or the words are parted by a plain space character. |
| `line_break_mode` | `none`, `tspan` | `none` | Whether one line is parted from the next by a `<tspan>` carrying a newline, so that copying several lines out keeps them on separate lines. |
| `axis_align_tolerance` | a number | `0.5` | How many degrees off level a line may be before it is given a rotation. |
| `min_confidence` | a number | `0` | Leave out words the recogniser is less sure of than this, from 0 to 1. |
| `text_fill` | text | `#000` | The colour drawn text is filled with, and that selected text shows in. |
| `selection_background` | text | `rgba(0, 90, 255, 0.35)` | The colour behind selected text, so that selecting invisible text shows. |
| `debug_line_stroke` | text | `#06c` | The colour line boxes are outlined in. |
| `debug_word_stroke` | text | `#c00` | The colour word boxes are outlined in. |
| `class_prefix` | text | `scribe-` | What every class name in the document starts with; a valid CSS identifier prefix. |
| `ids` | `true` or `false` | `false` | Give every line and word an id, such as `line-3` and `word-3-1`. |
| `title` | text | empty | A title for the document; left out when empty. |
| `aria_label` | text | empty | What assistive technology announces; the recognised text when empty. |
| `precision` | a whole number | `2` | How many decimals coordinates are written to. |
| `include_style` | `true` or `false` | `true` | Carry a stylesheet making the text selectable and its selection visible. |
| `xml_declaration` | `true` or `false` | `true` | Begin the document with an XML declaration. |
<!-- end options -->

## `json`

The layout as it stands, which is the same document `--layout-json` writes and
the same one `scribe render` reads back. Nothing is lost: a layout kept from
one run renders as well tomorrow as it did today, and without the models.

```sh
scribe ocr page.png --format json --opt include_chars=false
```

With `include_image` set, and the image's bytes to hand, the document gains an
`image_data_uri` field beside the layout's own. That field is not part of the
layout model, so a document written with it is not one `scribe render` will
read back.

<!-- options: json -->
| Option | Takes | Default | What it does |
| --- | --- | --- | --- |
| `pretty` | `true` or `false` | `true` | Indent the document over several lines instead of writing it on one. |
| `include_chars` | `true` or `false` | `true` | Keep the per-character boxes, which are most of the document's size. |
| `include_image` | `true` or `false` | `false` | Add the source image as an `image_data_uri` field, if its bytes are known. |
<!-- end options -->

## `template`

The layout through a Jinja template: one of the five that ship with scribe, or
one of your own. This is how to write a format scribe has never heard of —
hOCR for a PDF builder, ALTO for an archive, HTML for a page, CSV for a
spreadsheet — without writing any Rust.

```sh
scribe ocr page.png --format template --opt template=hocr
scribe ocr page.png --template-file my-format.jinja --opt var.title="Page 1"
```

The built-in templates are `html-overlay`, `hocr`, `alto`, `markdown` and
`text`; `scribe templates` lists them. Options named `var.<name>` are not this
format's own — they reach the template as `vars.<name>`, so a template can
take parameters that scribe knows nothing about. [Writing
templates](templates.md) describes the context a template is given, the
filters it can call and what each built-in one does.

An output's media type and file extension come from the template that was
chosen, and can be set outright with `mime` and `extension`. They matter:
`autoescape` follows the media type, escaping values as they are written when
the output is HTML or XML and leaving them alone when it is not.

<!-- options: template -->
| Option | Takes | Default | What it does |
| --- | --- | --- | --- |
| `template` | `html-overlay`, `hocr`, `alto`, `markdown`, `text` | `html-overlay` | Which template to render: `html-overlay`, `hocr`, `alto`, `markdown` or `text`; ignored when `template_source` is set. |
| `template_source` | text | empty | A template of your own, in Jinja syntax, rendered instead of a built-in one. |
| `mime` | text | empty | The media type of the output; the chosen template's own when empty. |
| `extension` | text | empty | The file extension of the output, without a dot; the chosen template's own when empty. |
| `autoescape` | `auto`, `html`, `none` | `auto` | Whether values are escaped as they are written; `auto` escapes when the media type is HTML or XML. |
<!-- end options -->

## Adding a format

A format is a `Renderer`: it says what it is called, describes its options, and
turns a layout and an image into bytes. Register one and the command line and
the JavaScript bindings offer it and its options beside the built-in ones,
without either of them knowing anything about it.

```rust
use scribe_core::image_source::ImageSource;
use scribe_core::layout::Layout;
use scribe_core::render::{
    OptionSpec, Options, RenderError, RenderOutput, Renderer, registry,
};

struct WordCount;

impl Renderer for WordCount {
    fn name(&self) -> &str {
        "word-count"
    }

    fn describe_options(&self) -> Vec<OptionSpec> {
        Vec::new()
    }

    fn render(
        &self,
        layout: &Layout,
        _image: &ImageSource<'_>,
        options: &Options,
    ) -> Result<RenderOutput, RenderError> {
        self.resolve_options(options)?;
        let words: usize = layout.lines.iter().map(|line| line.words.len()).sum();
        Ok(RenderOutput::text(words.to_string(), "text/plain", "txt"))
    }
}

let mut registry = registry();
registry.register(Box::new(WordCount));
assert!(registry.names().contains(&"word-count"));
```
