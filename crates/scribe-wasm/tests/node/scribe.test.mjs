// Runs the WebAssembly bindings the way a JavaScript caller would.
//
// The build these load is the one `scripts/build-wasm.sh` writes; run that
// first, then `node --test 'crates/scribe-wasm/tests/node/**/*.test.mjs'`.
//
// What is checked here is the crossing between JavaScript and Rust — that a
// plain object goes in, that the right thing comes back out, and that a
// mistake arrives as an exception saying so. The rendering itself is the
// core's own business and is tested there.

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

/** The Node build of the bindings. */
const build = new URL('../../pkg/nodejs/scribe.js', import.meta.url);

/** The sample images and layouts the whole project shares. */
const fixtures = new URL('../../../../tests/fixtures/', import.meta.url);

/** Set this to the path of the text detection model. */
const DETECTION_VARIABLE = 'SCRIBE_DETECTION_MODEL';

/** Set this to the path of the text recognition model. */
const RECOGNITION_VARIABLE = 'SCRIBE_RECOGNITION_MODEL';

const scribe = await load();
const hello = JSON.parse(await readFile(new URL('hello.layout.json', fixtures), 'utf8'));

test('a layout renders as an SVG with a text layer and no image', () => {
  const svg = scribe.render(hello, 'svg', { image_mode: 'none' });

  assert.equal(svg.mime, 'image/svg+xml');
  assert.match(svg.text, /<tspan/);
  assert.match(svg.text, /Hello/);
  assert.equal(new TextDecoder().decode(svg.bytes), svg.text);
  assert.doesNotMatch(svg.text, /<image/);
});

test('a render embeds the image it is given', async () => {
  const bytes = await readFile(new URL('hello.png', fixtures));
  const svg = scribe.render(hello, 'svg', undefined, {
    width: hello.image.width,
    height: hello.image.height,
    mime: 'image/png',
    bytes,
  });

  assert.match(svg.text, /<image[^>]*data:image\/png;base64,/);
});

test('a layout that carries its own image renders with it', async () => {
  const bytes = await readFile(new URL('hello.png', fixtures));
  const embedded = JSON.parse(
    scribe.render(hello, 'json', { include_image: true }, {
      width: hello.image.width,
      height: hello.image.height,
      mime: 'image/png',
      bytes,
    }).text,
  );

  assert.match(embedded.image_data_uri, /^data:image\/png;base64,/);

  const svg = scribe.render(embedded, 'svg');

  assert.match(svg.text, /<image[^>]*data:image\/png;base64,/);
});

test('a template renders through the same layout', () => {
  const overlay = scribe.render(hello, 'template', { template: 'html-overlay' });

  assert.match(overlay.text, /Hello/);
  assert.ok(scribe.templates().includes('html-overlay'));
});

test('a layout out of JSON renders the same as one out of an engine', () => {
  const json = scribe.render(hello, 'json', { pretty: false });

  assert.deepEqual(JSON.parse(json.text), hello);
});

test('every format this build knows describes its own options', () => {
  assert.deepEqual(scribe.formats(), ['json', 'svg', 'template']);

  const specs = scribe.describeOptions('svg');
  const textMode = specs.find((spec) => spec.name === 'text_mode');

  assert.equal(textMode.kind, 'str');
  assert.equal(textMode.default, 'invisible');
  assert.deepEqual(textMode.choices, ['invisible', 'visible', 'debug']);
  assert.ok(textMode.help.length > 0);
});

test('the layout schema describes the layout that was just rendered', () => {
  const schema = scribe.layoutSchema();

  assert.equal(schema.title, 'Layout');
  assert.deepEqual(Object.keys(schema.properties).sort(), ['image', 'lines', 'version']);
});

test('the version is the one the package was built from', () => {
  assert.match(scribe.version(), /^\d+\.\d+\.\d+/);
});

test('a mistake arrives as an exception saying what was wrong', () => {
  assert.throws(() => scribe.render(hello, 'postscript'), /no `postscript` format/);
  assert.throws(() => scribe.render(hello, 'svg', { nonsense: 1 }), /nonsense/);
  assert.throws(() => scribe.render({ version: 1 }, 'svg'), /not a layout document/);
  assert.throws(() => scribe.render(hello, 'svg', undefined, { width: 1 }), /`image\.height`/);
  assert.throws(
    () => scribe.render({ ...hello, version: 99 }, 'svg'),
    /version 99, but this build of scribe understands version 1/,
  );
});

test('the engine reads the text in an encoded image', async (t) => {
  const models = await loadModels();
  if (!models) {
    return t.skip(`set ${DETECTION_VARIABLE} and ${RECOGNITION_VARIABLE} to run this test`);
  }

  const engine = new scribe.Engine(models.detection, models.recognition);
  try {
    const layout = engine.analyzeEncoded(await readFile(new URL('hello.png', fixtures)));

    assert.equal(layout.version, hello.version);
    assert.deepEqual(layout.image, hello.image);
    assert.match(layout.lines.map((line) => line.text).join('\n'), /Hello/);
    assert.ok(words(layout).every((word) => word.chars.length === word.text.length));
    assert.match(scribe.render(layout, 'template', { template: 'text' }).text, /Hello/);
  } finally {
    engine.free();
  }
});

test('the options an engine is built with reach the recogniser', async (t) => {
  const models = await loadModels();
  if (!models) {
    return t.skip(`set ${DETECTION_VARIABLE} and ${RECOGNITION_VARIABLE} to run this test`);
  }

  const engine = new scribe.Engine(models.detection, models.recognition, {
    include_chars: false,
    beam_width: 2,
  });
  try {
    const layout = engine.analyzeEncoded(await readFile(new URL('hello.png', fixtures)));

    assert.match(layout.lines.map((line) => line.text).join('\n'), /Hello/);
    assert.ok(words(layout).every((word) => word.chars.length === 0));
  } finally {
    engine.free();
  }
});

test('a buffer of pixels is read as the size and layout it is said to be', async (t) => {
  const models = await loadModels();
  if (!models) {
    return t.skip(`set ${DETECTION_VARIABLE} and ${RECOGNITION_VARIABLE} to run this test`);
  }

  const [width, height] = [64, 32];
  const blank = new Uint8Array(width * height * 3).fill(0xff);
  const engine = new scribe.Engine(models.detection, models.recognition);
  try {
    assert.deepEqual(engine.analyzePixels(width, height, 3, blank).image, { width, height });
    assert.throws(() => engine.analyzePixels(width, height, 2, blank), /channel count/);
    assert.throws(
      () => engine.analyzePixels(width, height, 4, blank),
      /need 8192 bytes, but 6144 were given/,
    );
  } finally {
    engine.free();
  }
});

/** Every word of a layout, whichever line it is on. */
function words(layout) {
  return layout.lines.flatMap((line) => line.words);
}

/** The bindings, or an explanation of what has not been built yet. */
async function load() {
  try {
    return (await import(build)).default;
  } catch (cause) {
    throw new Error('run scripts/build-wasm.sh before these tests', { cause });
  }
}

/** The models, or `null` if the environment does not say where they are. */
async function loadModels() {
  const paths = [process.env[DETECTION_VARIABLE], process.env[RECOGNITION_VARIABLE]];
  if (!paths.every(Boolean)) {
    return null;
  }
  const [detection, recognition] = await Promise.all(paths.map((path) => readFile(path)));
  return { detection, recognition };
}
