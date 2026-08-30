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
<svg xmlns="http://www.w3.org/2000/svg" id="scribe-g923no" class="scribe-g923no-root" width="284" height="96" viewBox="0 0 284 96">
  <style>
    #scribe-g923no { color-scheme: light dark; }
    #scribe-g923no .scribe-g923no-text { all: revert; fill: transparent; font-family: sans-serif; white-space: pre; … }
    #scribe-g923no .scribe-g923no-text text, #scribe-g923no .scribe-g923no-text tspan { fill: inherit; font-family: inherit; white-space: inherit; … }
    #scribe-g923no .scribe-g923no-text::selection, #scribe-g923no .scribe-g923no-text ::selection { fill: HighlightText; background: color-mix(in srgb, Highlight 35%, transparent); }
  </style>
  <g class="scribe-g923no-text">
    <text class="scribe-g923no-line"><tspan class="scribe-g923no-word" x="26" y="55.4" font-size="33" textLength="105" lengthAdjust="spacingAndGlyphs">Hello</tspan> <tspan class="scribe-g923no-word" x="146" y="56" font-size="35" textLength="111" lengthAdjust="spacingAndGlyphs">World</tspan></text>
  </g>
</svg>
```

An SVG loaded through `<img src="page.svg">` is treated as a picture and none
of this works. The document has to be inline in the page, or in an `<object>`
or `<iframe>`.

### Naming what the document writes: `scope_mode`, `scope`, `class_prefix`

A standalone `.svg` file, and one loaded through `<object>` or `<iframe>`, is
its own document: nothing it declares can reach anything else. Inline in
somebody else's page it is not, and three things stop being local to it — the
class names, the ids, and every rule in the `<style>`, which a browser applies
to the whole of the page rather than to the `<svg>` it sits in.

So every name the document writes is built from three parts: `class_prefix`,
a token that sets this document apart from the others in the page, and the
name itself.

```
class="scribe-g923no-text"    id="scribe-g923no-line-3"
```

- `scope_mode=content`, the default, works the token out from the whole of the
  layout: what it says, how large the image is, and where every box sits on it,
  so that two crops of one sign are told apart. It is derived rather than
  random, so the same layout rendered twice gives the same document, byte for
  byte — which is what makes the output worth caching and worth keeping under
  version control.
  The consequence to know about is the other side of the same coin: the *same
  image twice in one page collides with itself*.
- `scope_mode=fixed` takes the token from `scope`. This is what the page with
  two copies of one image wants, since it is the page, and not the renderer,
  that knows there are two.
- `scope_mode=none` writes nothing but the prefix, which suits a document that
  is going to stand on its own with nothing around it to collide with.

The root element carries the prefix and the token as its id — `scribe-g923no`
above — both because the stylesheet needs something to hang off and because a
script given one image needs a way to find the layer over it.

An empty `class_prefix` is fine when a token follows it, since a token begins
with a letter; with no token at all — `scope_mode=none` — it is refused,
because the prefix is then the whole of every name and a bare `.root` is as
likely to collide with a page as a name can be.

`class_prefix` and `scope` are checked rather than escaped, and a value that
is not a valid CSS identifier is refused by name: escaping is what a document
does to text, and these land in a selector, where an escaped form would name
something else. The colours and `font_family` are checked the same way, so
that none of them can carry a `}` into a stylesheet that reaches the whole
page.

### Holding the layer against the page: `include_style`, `style_nonce`

Every rule the document writes hangs off the root element:

```css
#scribe-g923no .scribe-g923no-text { all: revert; fill: transparent; … }
```

That keeps the rules from reaching anything else in the page, and the
specificity it costs is the point rather than the price: it is what stops the
page's own rules from winning against the layer. A page saying no more than
`svg text { fill: currentColor }` would otherwise turn an invisible layer into
a visible one stacked over the picture, and one setting `font-family` on
`text` would shift the fitting out from under the pixels it was fitted to.

`all: revert` takes back everything the page said about the group itself, down
to the `display` and the `opacity` nothing else names. It cannot take back
what the page said further up, since reverting an inherited property leaves
the value inherited, so the properties the layer is read and copied by —
`font-style`, `letter-spacing`, `text-transform` and the rest — are named as
well; each line and word then takes that same list from the group rather than
from the page. Where a word sits, the size it is set at, the length it is
stretched to and the turn of its line are left alone, since those are the
fitting itself.

`include_style=false` leaves the `<style>` out. The layer stays selectable and
keeps its spaces — the same declarations go onto the group as a `style`
attribute instead — and what is given up is the selection colours and nothing
else, `::selection` being a pseudo-element with nowhere but a stylesheet to
live. Without a stylesheet the page's own rules can still reach the lines and
words inside the group, so `include_style=false` is for a page you know.

`style_nonce` is written as `nonce="…"` on the `<style>` element. A page whose
Content Security Policy is not `style-src 'unsafe-inline'` drops the
stylesheet otherwise, and nothing outside the renderer can put it back.

### What the text layer looks like: `text_mode`

- `invisible` — the text is transparent and the output looks exactly like the
  image. Selecting it shows the selection colour, so a reader can see what
  they have.
- `visible` — the text is drawn in `text_fill` over the image. Useful for
  seeing how well the layer fits, and for a "text view" of a picture.
- `debug` — the text is drawn and the boxes it came from are outlined, lines in
  `debug_line_stroke` and words in `debug_word_stroke`. This is the mode to
  look at when a layer sits wrongly.

### What a selection looks like: `selection_fill`, `selection_background`

Selected text is drawn in the reader's own colours rather than colours of the
document's choosing. `selection_fill` defaults to `HighlightText` and
`selection_background` to `Highlight` mixed down to 35% — the system colours a
browser selects with — so a selection here looks like a selection anywhere
else, in light mode and in dark. The mix is what lets the pixels of the word
beneath a selection still show through, the way a PDF viewer's highlight does.

The root element carries `color-scheme: light dark`, which is what lets those
colours resolve to the dark ones when the reader is in dark mode. It also
settles what a document opened on its own is drawn on: without it the canvas
around the picture stays white however dark the browser is. The rule hangs off
the root's class rather than the element itself, so an SVG placed inline in a
page cannot change the colour scheme that page chose for itself.

Note that the image underneath is unchanged either way: a scan of a white page
stays white in dark mode. It is the selection and the canvas that follow the
reader, not the picture.

### Where the image comes from: `image_mode`

- `embed` — the image is carried in the document as a `data:` URI, so the SVG
  is one self-contained file. Needs the image's bytes and media type.
- `link` — the document points at the image with `<image href="…">`, which
  keeps it small but ties it to where the image is. Needs an href; the command
  line writes the image's path as it reads from the directory the output goes
  into, since that is what a browser resolves it against, or whatever
  `--link-href` says.
- `none` — no image at all, leaving a text layer to be laid over an `<img>`
  elsewhere. This is the mode a browser overlay wants.

Asking to embed an image whose bytes were not supplied, or to link to one with
no href, is an error rather than a silently empty document, and the command
line answers it by naming `--image` and `--no-image`.

The library keeps `embed` as its default, since a caller that reached for a
renderer usually has the picture in hand. `scribe render` is given a layout
and not always an image, so when none is named, none is carried in the
document being read and no flag or `--opt` said what to do with one, it
renders as though `--no-image` had been given and says so at `-v`. An
`--image` whose kind cannot be told from its bytes is refused by name before
anything is rendered.

### What a screen reader hears: `role`, `aria_label`, `title`

An SVG inline in a page is part of that page's accessibility tree, and the
role on its root element decides how much of the text layer that tree holds.

- `role=none`, the default, leaves the attribute off the element rather than
  writing the ARIA role of that name, which would mean the opposite. The
  `<text>` elements are exposed as the text they are, so a screen reader reads
  the whole of the layer in the order the lines were written, which is what
  the layer is for. No `aria-label` is derived: the text already says what the
  text says.
- `role=img` announces the document as a picture. WAI-ARIA makes the children
  of an image presentational, so what is heard is the label and nothing
  within, however much text the layer holds. Given no `aria_label`, the
  document takes one from the recognised text, cut to 200 characters at a word
  boundary and ended with an ellipsis. This is the mode for a picture whose
  words are an aside — a logo, a sign, a caption — or for a page that carries
  the text somewhere else already.
- `role=group` exposes the text as `none` does and draws a boundary around it
  that a reader can move to and past in one step, and that `aria_label` names.
  This is the middle ground for a page with several of these in it: the whole
  transcript is there, and it can be skipped.

`aria_label` is written under any role when you set it, and derived from the
text only under `img`. `title` becomes a `<title>` element whatever the role
is: a tooltip in a browser, and the name of a document opened on its own.

None of it reaches a reader through `<img src="page.svg">`, which is a picture
whatever its root says. How much of it is heard otherwise differs between
screen readers, and between a document inline in a page and one in an
`<object>` or an `<iframe>`.

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
| `font_scale` | a number | `1` | What to multiply a box's height by to get its font size; above zero. |
| `font_size_mode` | `box_height`, `cap_height` | `box_height` | Whether a box's height is the whole font size or the height of a capital letter set in it, which suits text written in capitals. |
| `cap_height_ratio` | a number | `0.7` | How much of the font size a capital letter stands, as a fraction above zero, when the size is worked out from cap height. |
| `baseline_mode` | `ratio`, `estimate` | `ratio` | Whether the baseline is always the fixed fraction above a box's bottom, or the bottom itself for a line whose characters do not fall below it. |
| `baseline_ratio` | a number | `0.2` | How far above the bottom of a box its baseline sits, as a fraction of the height, from 0 to 1. |
| `length_adjust` | `spacingAndGlyphs`, `spacing` | `spacingAndGlyphs` | Whether fitting a word to its box stretches the gaps alone or the glyphs as well. |
| `char_positions` | `true` or `false` | `false` | Set each character at the pixels it was read from, where the recogniser said where they are, rather than stretching a whole word to fill its box. |
| `space_mode` | `none`, `tspan` | `none` | Whether the gap between two words holds a `<tspan>` of its own carrying the space, or the words are parted by a plain space character. |
| `line_break_mode` | `none`, `tspan` | `none` | Whether one line is parted from the next by a `<tspan>` carrying a newline, so that copying several lines out keeps them on separate lines. |
| `axis_align_tolerance` | a number | `0.5` | How many degrees off level a line may be before it is given a rotation; not negative. |
| `min_confidence` | a number | `0` | Leave out words the recogniser is less sure of than this, from 0 to 1. The recogniser scribe reads images with reports no confidence, so this filters only a layout that came from somewhere else. |
| `unscored_words` | `keep`, `drop` | `keep` | Whether a word carrying no confidence at all is kept or left out once `min_confidence` is above zero. |
| `text_fill` | text | `#000` | The colour drawn text is filled with. |
| `selection_fill` | text | `HighlightText` | The colour selected text shows in; a system colour follows the reader's own. |
| `selection_background` | text | `color-mix(in srgb, Highlight 35%, transparent)` | The colour behind selected text, so that selecting invisible text shows. |
| `debug_line_stroke` | text | `#06c` | The colour line boxes are outlined in. |
| `debug_word_stroke` | text | `#c00` | The colour word boxes are outlined in. |
| `class_prefix` | text | `scribe-` | What every class name in the document starts with; a valid CSS identifier prefix. May be empty when a token follows it, since a token begins with a letter of its own. |
| `scope_mode` | `content`, `fixed`, `none` | `content` | Whether the class names, the ids and the stylesheet carry a token setting this document apart from anything around it: one worked out from the whole of the layout, one of your own, or none at all. |
| `scope` | text | empty | The token to set this document apart, when `scope_mode` is `fixed`; a valid CSS identifier part. |
| `ids` | `true` or `false` | `false` | Give every line and word an id, such as `line-3` and `word-3-1`, under the same prefix and token as the classes. |
| `role` | `none`, `img`, `group` | `none` | What the document is announced as: `none` writes no role and leaves the text layer to be read word by word, `img` announces `aria_label` and nothing within, and `group` reads the text with a boundary around it. |
| `title` | text | empty | A title for the document; left out when empty. |
| `aria_label` | text | empty | What assistive technology announces the document as; under `role=img`, the recognised text when empty. |
| `precision` | a whole number | `2` | How many decimals coordinates are written to, from 0 to 10. |
| `include_style` | `true` or `false` | `true` | Carry a stylesheet making the text selectable and its selection visible. |
| `style_nonce` | text | empty | The nonce the stylesheet is written with, for a page whose content security policy does not allow inline styles outright. |
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
`image_data_uri` field beside the layout's own: one file that describes a
picture and holds it. `scribe render` reads that field back and renders from
the picture it carries, so a self-contained document needs no `--image`:

```sh
scribe ocr page.png --format json --opt include_image=true -o page.json
scribe render page.json -o page.svg
```

An `--image` given as well is the one that is used; the carried picture is for
when there is none.

<!-- options: json -->
| Option | Takes | Default | What it does |
| --- | --- | --- | --- |
| `pretty` | `true` or `false` | `true` | Indent the document over several lines instead of writing it on one. |
| `include_chars` | `true` or `false` | `true` | Keep the per-character boxes, which are most of the document's size. |
| `include_image` | `true` or `false` | `false` | Add the source image as an `image_data_uri` field, if its bytes are known, so that the document can be rendered again without it. |
<!-- end options -->

## `template`

The layout through a Jinja template: one of the twelve that ship with scribe,
or one of your own. This is how to write a format scribe has never heard of —
hOCR for a PDF builder, ALTO for an archive, HTML for a page, CSV for a
spreadsheet — without writing any Rust.

```sh
scribe ocr page.png --format template --opt template=hocr
scribe ocr page.png --template-file my-format.jinja --opt var.title="Page 1"
```

The built-in templates are `html-overlay`, `svg-overlay`, `html-figure`,
`sr-only-transcript`, `figure-transcript`, `json-ld`, `layout-json`, `hocr`,
`alto`, `markdown`, `text` and `alt-text`; `scribe templates` lists them. The
first seven are the ways of giving an image in a web page text that can be
found, selected, read aloud and indexed. Options named `var.<name>` are not
this format's own — they reach the template as `vars.<name>`, so a template
can take parameters that scribe knows nothing about. [Writing
templates](templates.md) describes the context a template is given, the
filters it can call and what each built-in one does.

An output's media type and file extension come from the template that was
chosen, and can be set outright with `mime` and `extension`. They matter:
`autoescape` follows the media type, escaping values as they are written when
the output is HTML or XML and leaving them alone when it is not.

<!-- options: template -->
| Option | Takes | Default | What it does |
| --- | --- | --- | --- |
| `template` | `html-overlay`, `svg-overlay`, `html-figure`, `sr-only-transcript`, `figure-transcript`, `json-ld`, `layout-json`, `hocr`, `alto`, `markdown`, `text`, `alt-text` | `html-overlay` | Which of the built-in templates to render, of the ones listed beside this; ignored when `template_source` is set. |
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
