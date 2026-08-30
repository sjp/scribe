# Writing templates

The `template` format renders a [layout](layout.md) through a
[Jinja](https://docs.rs/minijinja/) template, so any text format is reachable
without writing Rust. Twelve templates ship with scribe, and each one is also a
worked example of the context a template of your own is given.

```sh
scribe ocr page.png --format template --opt template=hocr
scribe ocr page.png --template-file my-format.jinja --opt var.title="Page 1"
scribe templates                      # the built-in names
```

```js
scribe.render(layout, 'template', { template: 'html-overlay' });
scribe.render(layout, 'template', { template_source: '{{ text }}' });
```

`--template-file` chooses the `template` format on its own, and sets
`template_source` to the contents of the file. `template` and
`template_source` are the format's own options; see
[the format's options](formats.md#template) for the rest.

## The context

Four names are in scope.

### `layout`

The layout exactly as [its JSON](layout.md) spells it: `layout.version`,
`layout.image.width`, `layout.image.height` and `layout.lines`, each line with
`text`, `bbox`, `rotated_box`, `words` and `confidence`; each word the same
with `chars`; each character with `text`, `bbox` and `confidence`.

```jinja
{% for line in layout.lines %}
  {% for word in line.words %}
    {{ word.text }} at {{ word.bbox.x }},{{ word.bbox.y }}
  {% endfor %}
{% endfor %}
```

`words` is empty for a line the recogniser gave no words for, and `chars` is
empty when character positions were not kept. `line.words or [line]` is the
idiom the built-in templates use to fall back to the line itself, since a line
and a word carry the same fields.

### `image`

The raster the layout was read from, as much of it as the caller offered:

| Name | Is |
| --- | --- |
| `image.width` | The width in pixels. Always known. |
| `image.height` | The height in pixels. Always known. |
| `image.mime` | The media type, such as `image/png`, or none. |
| `image.href` | A path or URL the output can point at, or none. |
| `image.data_uri` | The image as a `data:` URI, or none when its bytes or its media type are not known. |

`data_uri` is encoded the first time a template asks for it, so a template that
never mentions it never pays for it — base64 of a photograph is megabytes of
string that a plain-text output has no use for. `data_uri()` is also a
function, which reads the same.

Anything that may be absent should be guarded, since a value that is not there
prints as `None`. That is true of `confidence` in the layout as well as of the
image's fields, so guard with `is not none`, with `if`, or with `default` in
its boolean form:

```jinja
{% if image.href %}<img src="{{ image.href }}" />{% endif %}
{{ image.mime | default("application/octet-stream", true) }}
{% if word.confidence is not none %} conf="{{ word.confidence | round(2) }}"{% endif %}
```

### `text`

Every recognised line joined with newlines: the whole of what the image says,
ready for an `alt` attribute or a transcript.

### `vars`

Whatever the caller passed under `var.`, so a template can take parameters that
scribe knows nothing about:

```sh
scribe render page.layout.json --template-file my.jinja --opt var.title="Page 1" --opt var.compact=on
```

```jinja
<h1>{{ vars.title }}</h1>
{% if vars.compact | default(false) | flag %}…{% endif %}
```

A value typed at a command line arrives as text, so `flag` and `number` are
there to read one as a boolean or a number. Always give a `default`: a name
that was never set is an error, not an empty string.

## Strictness and escaping

Templates are read strictly. Naming something that is not there fails, and the
error says which template and which line and column:

```text
error: the template renderer could not use the template it was given, line 3 column 8: undefined value
```

Values are escaped as they are written when the output is marked up, which is
worked out from its media type: `text/html`, `application/xml` and anything
else whose type mentions HTML or XML are escaped, and everything else is left
alone. `autoescape` overrides that with `auto`, `html` or `none`, and `mime`
changes the media type the guess is made from. The apostrophe is written as
`&#39;`, which is valid in both HTML and XML.

## Filters and functions

Each of these is both a filter and a function, so `{{ x | round(2) }}` and
`{{ round(x, 2) }}` are the same.

| Name | Does |
| --- | --- |
| `round(value, precision=0)` | Writes a number with at most that many decimals and no trailing zeros, so a whole pixel reads as `28` rather than `28.0`. |
| `number(value)` | Reads a value as a number, including one that arrived as text. Fails if it is not one. |
| `flag(value)` | Reads a value as true or false the way an option is read, so `off`, `no`, `0` and `false` all mean false. |
| `json(value)` | Writes a value as JSON, with `<`, `>` and `&` escaped so the result is safe inside a `<script>` as well as parseable. |
| `xml_escape(value)` | Escapes the five markup characters, spelling the apostrophe as `&apos;`. |
| `html_escape(value)` | The same, spelling the apostrophe as `&#39;`. |
| `base64(value)` | The value's text as standard base64 with padding. |
| `css_value(value, name=None)` | The value if a stylesheet can carry it as it stands, and a failure if it could close a declaration or open a rule of its own. |
| `css_ident(value, name=None)` | The value if a CSS identifier can hold it, which is what a class name, an id or a prefix for either is built from. |
| `css_ident_start(value, name=None)` | A whole composed name, if an identifier may begin the way it does. |
| `rotate_transform(box, precision=2)` | An oriented box's rotation as `rotate(angle cx cy)`, which is what both SVG and CSS take. |
| `points(box, precision=2)` | An oriented box's four corners as the `x,y` pairs an SVG `<polygon>` takes, clockwise from the corner that is the top left before rotation. |
| `data_uri()` | The source image as a `data:` URI, or none. |
| `svg(options, **overrides)` | The layout as an SVG document, written by the [`svg` renderer](formats.md#svg) with its own options, ready to be put inside the page the template is writing. |
| `scope()` | The token the `svg` renderer works out for this layout, so a template can name what it puts around a layer what the layer itself is named. |

The three `css_` checks are what the [`svg` renderer](formats.md#svg) applies
to the options of the same shape, so a template writing a `<style>`, a class
or an id from a caller's value refuses what the renderer would refuse. They
return the value rather than escaping it, and mark it as needing no escaping:
a stylesheet is not text, an escape inside a `<style>` does not mean the same
thing to the parser reading a page as to the one reading a standalone
document, and an escaped name selects something other than what stands beside
it in the rule. `name` is what the error calls the value, so that whoever
passed it is told which one was refused:

```text
error: the template renderer could not use the `html-overlay` template, line 16 column 75: invalid operation: cannot use "red} body{display:none} x{" as `selection_fill`: a value written into a stylesheet cannot hold '}'
```

```jinja
<style>.{{ ns }}text::selection { background: {{ vars.highlight | default("Highlight") | css_value("highlight") }} }</style>
```

A value that has been through `css_value` belongs in a `<style>`; anywhere
else — an attribute, say — escape it as usual.

`svg` takes the renderer's options as a mapping, as keywords, or as both with
the keywords winning, so a template can pass the caller's own values straight
through and still settle what it must. The XML declaration is left out unless
it is asked for, since the document is going inside another one, and the
result is markup rather than text to be escaped into the page.

```jinja
{{ svg(vars, image_mode="none") }}
{{ svg({"char_positions": true}, image_mode="none", text_mode="visible") }}
```

`scope` is the same token the renderer would work out for itself under
[`scope_mode=content`](formats.md#naming-what-the-document-writes-scope_mode-scope-class_prefix),
so a template can name its wrapper, its transcript and the layer inside it all
alike, and two of them in one page collide over nothing:

```jinja
{%- set prefix = vars.class_prefix | default("scribe-") | css_ident("class_prefix") -%}
{%- set ns = (prefix ~ scope() ~ "-") | css_ident_start("class_prefix") -%}
<div class="{{ ns }}overlay">{{ svg(vars, image_mode="none", scope_mode="fixed", scope=scope()) }}</div>
```

Everything minijinja itself provides — `default`, `join`, `map`, `length`,
`loop.index`, macros, `{% set %}` — is available too.

## The built-in templates

The first seven are ways of giving an image in a web page text that can be
found, selected, read aloud and indexed. No single one of them reaches every
reader, which is what `html-figure` is for; the rest are the formats other
tools read.

### `html-overlay`

The image with its text laid over it: the picture exactly as it was, and one
absolutely positioned transparent `<span>` per word on the pixels it was read
from. This is the shape a browser extension injects into a page, and the same
approach a PDF viewer's text layer takes. Find-in-page, selection and screen
readers all work; the `alt` attribute carries the whole text as well.

Takes `var.selection_fill` and `var.selection_background`, the colours selected
text is drawn in, both of them a system colour by default so that a selection
looks like every other selection the reader makes, in light mode and in dark;
`var.char_positions`, to give every character a span on its own pixels rather
than stretching a word across its box; and `var.font_size_mode` (`box_height`
or `cap_height`) with `var.cap_height_ratio`, to read a box as the height of a
capital letter.

```html
<div class="scribe-overlay">
  <style>
    .scribe-overlay { position: relative; display: inline-block; line-height: 0; color-scheme: light dark }
    .scribe-overlay img { display: block; max-width: 100% }
    .scribe-overlay span { position: absolute; color: transparent; white-space: pre; line-height: 1; transform-origin: center }
    .scribe-overlay span::selection { color: HighlightText; background: color-mix(in srgb, Highlight 35%, transparent) }
  </style>
  <img src="hello.png" width="284" height="96" alt="Hello World" />
  <span style="left: 26px; top: 29px; width: 105px; height: 33px; font-size: 33px; transform: rotate(0deg)">Hello</span>
  <span style="left: 146px; top: 28px; width: 111px; height: 35px; font-size: 35px; transform: rotate(0deg)">World</span>
</div>
```

The `<img>` is left out when neither the image's bytes nor an href were given,
which leaves a bare text layer to position over an image already in the page.

### `svg-overlay`

The same idea as `html-overlay` with the layer written by the [`svg`
renderer](formats.md#svg) instead of by positioned spans, so that a turned
line turns with its box and a word is fitted to the pixels it was read from.
Its `var.` options are the SVG renderer's own — every one of them, by the same
name — save that the image is never carried in the layer, since the page's own
`<img>` is underneath it. `var.class_prefix`, `var.scope_mode` and `var.scope`
name the wrapper and the layer inside it alike, so that two of these in one
page share nothing.

```html
<div class="scribe-g923no-overlay">
  <style>
    .scribe-g923no-overlay { position: relative; display: inline-block; line-height: 0 }
    .scribe-g923no-overlay img { display: block; max-width: 100% }
    .scribe-g923no-overlay svg { position: absolute; left: 0; top: 0; width: 100%; height: 100% }
  </style>
  <img src="hello.png" width="284" height="96" alt="" />
  <svg xmlns="http://www.w3.org/2000/svg" id="scribe-g923no" class="scribe-g923no-root" width="284" height="96" viewBox="0 0 284 96">
    …
  </svg>
</div>
```

The image carries no `alt`: the layer over it is read as the text it is, and
reading the same words twice helps nobody. A `var.role` of `img` turns the
layer back into a picture with a label, so a page asking for that wants to
write the `alt` itself.

### `html-figure`

Every mechanism at once, since no single one reaches every reader: a
`<figure>` holding the image with the whole text in its `alt`, a transparent
text layer over it that can be found and selected, and a transcript after it
that only a screen reader reaches. This is the one to reach for when the page
is not yours to know.

Takes `var.overlay`, `spans` (the default) or `svg`, for whether the layer is
positioned HTML or the SVG renderer's own; `var.id`, the id of the transcript;
and the options of the templates it says the same as — `var.class_prefix`,
`var.scope_mode`, `var.scope`, `var.selection_fill`,
`var.selection_background`, `var.char_positions`, `var.font_size_mode` and
`var.cap_height_ratio`.

An SVG layer here is written with `role=img`, unlike the one `svg-overlay`
writes: the `alt` and the transcript already say the text, so the layer is
there to be found and selected rather than read out a third time.

```html
<figure class="scribe-g923no-figure">
  <style>…</style>
  <div class="scribe-g923no-overlay">
    <img src="hello.png" width="284" height="96" alt="Hello World" aria-describedby="scribe-g923no-transcript" />
    <span style="left: 26px; top: 29px; width: 105px; height: 33px; font-size: 33px; transform: rotate(0deg)">Hello</span>
    <span style="left: 146px; top: 28px; width: 111px; height: 35px; font-size: 35px; transform: rotate(0deg)">World</span>
  </div>
  <div class="scribe-g923no-sr-only" id="scribe-g923no-transcript">
    <p>Hello World</p>
  </div>
</figure>
```

### `sr-only-transcript`

The image, and after it everything it says in an element that only a screen
reader reaches, tied to the image by `aria-describedby`. The page looks
exactly as it did, and a reader of a long document hears its lines in order
instead of a single breathless `alt`.

Takes `var.id`, the id the transcript carries and the image points at;
`var.class_prefix`, `var.scope_mode` and `var.scope`, which name it when
`var.id` is left alone, so that `aria-describedby` on two images in one page
addresses two transcripts rather than one; and `var.alt`, the short label the
image itself carries, the transcript being the long one.

```html
<div class="scribe-g923no-transcript">
  <style>
    .scribe-g923no-sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; border: 0; overflow: hidden; clip: rect(0 0 0 0); clip-path: inset(50%); white-space: normal }
  </style>
  <img src="hello.png" width="284" height="96" alt="An image of text, transcribed after it." aria-describedby="scribe-g923no-transcript" />
  <div class="scribe-g923no-sr-only" id="scribe-g923no-transcript">
    <p>Hello World</p>
  </div>
</div>
```

As with `html-overlay`, the `<img>` is left out when neither the image's bytes
nor an href were given, which leaves a bare transcript to place beside an
image already in the page.

### `figure-transcript`

A `<figure>` holding the image and, beneath it, everything it says. Nothing is
positioned over anything, so nothing can drift out of place; the text is
simply there, to be found, selected, read aloud and indexed.

Takes `var.mode`, `caption` (the default) or `details`, for whether the
transcript is a caption everyone sees or a `<details>` the reader opens;
`var.summary`, what a `<details>` is labelled with; `var.class_prefix`; and
`var.alt`.

```html
<figure class="scribe-figure">
  <img src="hello.png" width="284" height="96" alt="An image of text, transcribed after it." />
  <figcaption class="scribe-transcript">
    <p>Hello World</p>
  </figcaption>
</figure>
```

### `json-ld`

A schema.org `ImageObject` carrying what the image says, for the crawlers and
indexes that read JSON-LD. Nothing on the page changes and nothing is shown:
this is the one mechanism written for readers that never look at pixels.

Takes `var.caption`, a caption for the image, written only when it is given;
and `var.wrap`, true by default, for whether the document is written inside
the `<script>` element a page carries it in. `var.wrap=false` writes the bare
JSON, and wants `mime` and `extension` set with it.

```html
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "ImageObject",
  "contentUrl": "hello.png",
  "width": 284,
  "height": 96,
  "text": "Hello World"
}
</script>
```

### `layout-json`

The whole layout in the page, as the `json` renderer writes it, for the page's
own code to read. A script or an extension can build any of these mechanisms
from it lazily, or feed the site's own search, without asking a server for
anything.

Takes `var.id`, the id the script carries and the image points at;
`var.image`, true by default, for whether the image is written before it;
`var.class_prefix`, what the class name and the `data-` attribute start with;
and `var.scope_mode` and `var.scope`, the token in the id when `var.id` is
left alone. The `data-` attribute keeps the plain prefix and none of the
token, since a script looking for a layout has to know the name it is looking
for; the id it holds is what tells one layout from another.

```html
<div class="scribe-g923no-layout">
  <img src="hello.png" width="284" height="96" alt="Hello World" data-scribe-layout="scribe-g923no-layout" />
  <script type="application/json" id="scribe-g923no-layout">{"version":1,"image":{"width":284,"height":96},"lines":[…]}</script>
</div>
```

The layout goes through `json`, whose escaping of `<`, `>` and `&` is what
keeps text read from an image from ending the element early.

### `hocr`

hOCR 1.2: HTML whose class names and `title` attributes carry the layout, which
is what PDF builders and text-layer viewers read. Takes `var.title`, the title
of the document.

```html
  <body>
    <div class="ocr_page" id="page_1" title="bbox 0 0 284 96; image &quot;hello.png&quot;">
      <span class="ocr_line" id="line_1" title="bbox 26 28 257 63">
        <span class="ocrx_word" id="word_1_1" title="bbox 26 29 131 62">Hello</span>
        <span class="ocrx_word" id="word_1_2" title="bbox 146 28 257 63">World</span>
      </span>
    </div>
  </body>
```

### `alto`

ALTO 4: the XML that digitisation pipelines and archives exchange page layouts
in. One text block holds every line, since the recogniser reports lines rather
than the regions they were set in.

```xml
    <Page ID="page_1" PHYSICAL_IMG_NR="1" WIDTH="284" HEIGHT="96">
      <PrintSpace HPOS="0" VPOS="0" WIDTH="284" HEIGHT="96">
        <TextBlock ID="block_1" HPOS="0" VPOS="0" WIDTH="284" HEIGHT="96">
          <TextLine ID="line_1" HPOS="26" VPOS="28" WIDTH="231" HEIGHT="35">
            <String ID="word_1_1" HPOS="26" VPOS="29" WIDTH="105" HEIGHT="33" CONTENT="Hello" />
            <String ID="word_1_2" HPOS="146" VPOS="28" WIDTH="111" HEIGHT="35" CONTENT="World" />
          </TextLine>
        </TextBlock>
      </PrintSpace>
    </Page>
```

### `markdown`

Each recognised line as its own paragraph.

```markdown
Reading Machines

The quick brown fox jumps

over the lazy dog, and does
```

### `text`

Every recognised line, one per line, and nothing else.

```text
Reading Machines
The quick brown fox jumps
over the lazy dog, and does
```

### `alt-text`

Everything the image says as one line, ready for an `alt` attribute: newlines
and runs of spaces become single spaces, since an attribute has no lines to
break. Takes `var.max_chars`, the most characters to write, `0` by default for
all of them; a longer text is cut at a word boundary and ends in an ellipsis.

```text
Reading Machines The quick brown fox jumps over the lazy dog, and does
```

The text is written as it was read, not escaped, since where it is going is
not the template's to know: a caller placing it in markup asks for
`autoescape=html`.

## A template of your own

A template writes plain text with the extension `txt` unless it is told
otherwise, so say what you are writing:

```sh
scribe render page.layout.json \
  --template-file words.csv.jinja \
  --opt mime=text/csv --opt extension=csv \
  --out words.csv
```

```jinja
{#- One row per word: the text and the box it was read from. -#}
text,x,y,width,height,confidence
{% for line in layout.lines -%}
{% for word in line.words -%}
"{{ word.text | replace('"', '""') }}",{{ word.bbox.x | round(1) }},{{ word.bbox.y | round(1) }},{{ word.bbox.width | round(1) }},{{ word.bbox.height | round(1) }},{{ word.confidence | default("", true) }}
{% endfor -%}
{% endfor -%}
```

The templates that ship with scribe are in [`templates/`](../templates); they
are the shortest way to see the context in use. Copy one and change it.
