# Writing templates

The `template` format renders a [layout](layout.md) through a
[Jinja](https://docs.rs/minijinja/) template, so any text format is reachable
without writing Rust. Five templates ship with scribe, and each one is also a
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
| `rotate_transform(box, precision=2)` | An oriented box's rotation as `rotate(angle cx cy)`, which is what both SVG and CSS take. |
| `points(box, precision=2)` | An oriented box's four corners as the `x,y` pairs an SVG `<polygon>` takes, clockwise from the corner that is the top left before rotation. |
| `data_uri()` | The source image as a `data:` URI, or none. |

Everything minijinja itself provides — `default`, `join`, `map`, `length`,
`loop.index`, macros, `{% set %}` — is available too.

## The built-in templates

### `html-overlay`

The image with its text laid over it: the picture exactly as it was, and one
absolutely positioned transparent `<span>` per word on the pixels it was read
from. This is the shape a browser extension injects into a page, and the same
approach a PDF viewer's text layer takes. Find-in-page, selection and screen
readers all work; the `alt` attribute carries the whole text as well.

Takes `var.text_fill` and `var.selection_background`, the colours selected text
is drawn in; `var.char_positions`, to give every character a span on its own
pixels rather than stretching a word across its box; and `var.font_size_mode`
(`box_height` or `cap_height`) with `var.cap_height_ratio`, to read a box as
the height of a capital letter.

```html
<div class="scribe-overlay">
  <style>
    .scribe-overlay { position: relative; display: inline-block; line-height: 0 }
    .scribe-overlay img { display: block; max-width: 100% }
    .scribe-overlay span { position: absolute; color: transparent; white-space: pre; line-height: 1; transform-origin: center }
    .scribe-overlay span::selection { color: #000; background: rgba(0, 90, 255, 0.35) }
  </style>
  <img src="hello.png" width="284" height="96" alt="Hello World" />
  <span style="left: 26px; top: 29px; width: 105px; height: 33px; font-size: 33px; transform: rotate(0deg)">Hello</span>
  <span style="left: 146px; top: 28px; width: 111px; height: 35px; font-size: 35px; transform: rotate(0deg)">World</span>
</div>
```

The `<img>` is left out when neither the image's bytes nor an href were given,
which leaves a bare text layer to position over an image already in the page.

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
