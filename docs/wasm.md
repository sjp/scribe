# The WebAssembly package

The whole pipeline — recognition and every renderer — compiles to
WebAssembly, so a browser can read the text in an image and lay a selectable
text layer over it without a server. The core touches no filesystem and fetches
nothing; the page supplies the model bytes and the pixels, and gets a
[layout](layout.md) and rendered documents back.

## Building

```sh
scripts/build-wasm.sh          # add --dev to build without optimising
```

That runs [wasm-pack](https://rustwasm.github.io/wasm-pack/) twice over
`crates/scribe-wasm`, once for the browser and once for Node, since the two
builds differ in the glue wasm-bindgen writes around the same module:

- `crates/scribe-wasm/pkg/web` — an ES module with a default `init` export,
  for a `<script type="module">` or a bundler.
- `crates/scribe-wasm/pkg/nodejs` — the module read straight from disk and
  instantiated on load, so there is no `init` to await. This is what the Node
  tests run against:

  ```js
  const scribe = (await import('./crates/scribe-wasm/pkg/nodejs/scribe.js')).default;
  ```

Both are npm packages called `scribe`, and both carry `scribe.d.ts`, which
declares the whole layout model, the option shapes and the rendered document,
so a TypeScript caller needs nothing generated.

The `decode` feature is on by default and is what `analyzeEncoded` needs.
Building without it drops the image decoders and leaves `analyzePixels`, which
is enough for a caller working from a `<canvas>`.

## The API

```ts
new Engine(detectionModel: Uint8Array, recognitionModel: Uint8Array, options?: OcrOptions)
Engine.analyzePixels(width, height, channels: 1 | 3 | 4, data: Uint8Array): Layout
Engine.analyzeEncoded(bytes: Uint8Array): Layout
Engine.free(): void

render(layout: Layout, format: string, options?: RenderOptions, image?: SourceImage): Rendered
formats(): string[]
templates(): string[]
describeOptions(format: string): OptionSpec[]
layoutSchema(): object
version(): string
```

An `Engine` holds the loaded models. Building one is expensive and reading an
image is not, so build it once and analyse as many images with it as you like —
and call `free()` when you are done, because the models are tens of megabytes
and nothing else will let go of them.

A `Layout` is a plain object. It survives `JSON.stringify`, can be kept in
`IndexedDB` or sent to a server, and can be handed back to `render` later
without the models being present at all — `render` loads nothing.

`render` returns `{ bytes, mime, text }`, where `text` is absent only if a
renderer produced bytes that are not UTF-8. `options` is a plain object of
whatever the format takes; a value of the wrong kind is converted where it can
be, so `'false'` and `false` mean the same thing. `describeOptions` returns
those options as data — name, kind, default, help and any choices — which is
enough to build a form or a menu for a format the page knows nothing about.

`image` describes the raster the layout was read from: `width` and `height`
always, and `mime`, `bytes` and `href` as far as they are known. Leave it out
for a format that emits text alone; the dimensions then come from the layout.
A layout that carries its own picture — one the `json` format wrote with
`include_image` set, kept as a single object — supplies the image when none is
passed, and an image passed in wins over the one it carries.

Errors arrive as thrown exceptions carrying the message the core would have
printed.

## Loading the models

scribe never downloads a model. The page fetches the two `.rten` files itself,
from wherever it keeps them, and hands over the bytes:

```js
async function models() {
  const [detection, recognition] = await Promise.all([
    fetch('/models/text-detection.rten').then((r) => r.arrayBuffer()),
    fetch('/models/text-recognition.rten').then((r) => r.arrayBuffer()),
  ]);
  return [new Uint8Array(detection), new Uint8Array(recognition)];
}
```

They are large, so serve them with a long cache lifetime, or keep them in the
[Cache API](https://developer.mozilla.org/en-US/docs/Web/API/Cache) and read
them from there on later visits. Analysis is synchronous and will block the
thread it runs on: for anything but a small image, run the engine in a worker.

## Overlaying an image in a page

The `none` image mode writes the text layer and nothing else, which is exactly
what to stack over an `<img>` that is already on the page: the picture is left
untouched, and the text over it can be selected, searched and read aloud.

```html
<div class="scribe-frame" style="position: relative; display: inline-block">
  <img id="page" src="page.png" alt="" style="display: block" />
</div>

<script type="module">
  import init, { Engine, render } from './pkg/web/scribe.js';

  await init();

  const image = document.getElementById('page');
  await image.decode();

  // The engine wants pixels, and a canvas is where a page has them.
  const canvas = document.createElement('canvas');
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const context = canvas.getContext('2d');
  context.drawImage(image, 0, 0);
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height);

  const [detection, recognition] = await models();
  const engine = new Engine(detection, recognition);
  let layout;
  try {
    layout = engine.analyzePixels(pixels.width, pixels.height, 4, pixels.data);
  } finally {
    engine.free();
  }

  // No image in the document: the picture underneath is the picture.
  const svg = render(layout, 'svg', { image_mode: 'none' });

  const overlay = document.createElement('div');
  overlay.innerHTML = svg.text;
  Object.assign(overlay.style, {
    position: 'absolute',
    inset: '0',
  });
  overlay.firstElementChild.setAttribute('width', '100%');
  overlay.firstElementChild.setAttribute('height', '100%');
  image.parentElement.append(overlay);
</script>
```

The SVG must be inline in the document for any of this to work. One loaded
through `<img src="page.svg">` is treated as a picture, and its text can be
neither selected nor found.

Rendering `template` with `template: 'html-overlay'` gives the same thing as
positioned HTML spans instead of SVG, which some browsers handle more
predictably, and `template: 'svg-overlay'` writes this same layer with the
`<img>` and the positioning around it already in place; [the
templates](templates.md#html-overlay) describe them.

## From a layout alone

Recognition is the expensive half, and the layout is the durable half. A page
that has read an image once can keep the layout and render it again — into a
different format, with different options — with no engine and no models:

```js
import init, { render, formats, describeOptions } from './pkg/web/scribe.js';

await init();

const layout = JSON.parse(localStorage.getItem('page.layout'));

formats();                                   // ["json", "svg", "template"]
describeOptions('svg')[0].name;              // "text_mode"

render(layout, 'template', { template: 'text' }).text;
render(layout, 'svg', { text_mode: 'debug', image_mode: 'none' }).text;

// Two copies of one image in a page need a name each, since the token the
// renderer works out for itself is the same for both.
render(layout, 'svg', { scope_mode: 'fixed', scope: 'left' }).text;
```
