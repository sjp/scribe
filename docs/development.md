# Development

## The workspace

```text
crates/scribe-core/     the library: layout model, OCR pipeline, renderers
crates/scribe-cli/      the `scribe` binary: files, arguments, terminal
crates/scribe-wasm/     the WebAssembly bindings
templates/              the built-in Jinja templates, embedded at build time
schema/                 the generated JSON Schema of the layout model
tests/fixtures/         the sample images and the layouts read from them
scripts/                fetching models and building the WebAssembly package
docs/                   this documentation
```

The split is the point. `scribe-core` touches no filesystem, no network and no
terminal, so it compiles for `wasm32-unknown-unknown`; every path, every
environment variable and every byte written to a stream lives in `scribe-cli`.
Keeping that line is what lets the same code run in a browser.

## Building

```sh
cargo build --workspace
cargo run -p scribe-cli -- --help
```

The OCR engine and its tensor library are unusably slow when built without
optimisation, so dependencies are compiled at `opt-level = 3` even in a debug
profile. The workspace's own crates stay debuggable.

Rust stable, edition 2024. For the WebAssembly build you also want the target
and wasm-pack:

```sh
rustup target add wasm32-unknown-unknown
cargo check -p scribe-core -p scribe-wasm --target wasm32-unknown-unknown
scripts/build-wasm.sh
node --test 'crates/scribe-wasm/tests/node/**/*.test.mjs'
```

## The models

The recognition models are not in this repository and are never downloaded by
the library. `scripts/fetch-models.sh` puts a copy in `models/`, which git
ignores:

```sh
scripts/fetch-models.sh
export SCRIBE_DETECTION_MODEL="$PWD/models/text-detection.rten"
export SCRIBE_RECOGNITION_MODEL="$PWD/models/text-recognition.rten"
```

## Tests

```sh
cargo test --workspace
```

Everything runs without a model. The handful of tests that need one — reading a
real image in `scribe-core`, and the end-to-end runs in `scribe-cli` — look for
the two environment variables above, and print `skipped: …` and pass when they
are not set. Set them and the same command runs the lot. CI has no models, so
those tests report themselves skipped there and are run locally.

Everything else works from the committed fixtures: the layouts in
`tests/fixtures` are what the pipeline read from the images beside them, so a
change in any renderer shows up as a change in a snapshot without a model being
loaded at all.

### Snapshots

Renderer output is snapshotted with [insta](https://insta.rs). A changed
snapshot is written beside the old one as `.snap.new`; review and accept them
with:

```sh
cargo install cargo-insta       # once
cargo insta review
```

Read the diff rather than accepting it blindly. A snapshot is the only place a
change in how a text layer is placed becomes visible.

### Generated files

Three things in the repository are generated from the code and checked in, each
with a test that fails when it has drifted:

| File | Regenerate with |
| --- | --- |
| `schema/layout.schema.json` | `SCRIBE_UPDATE_SCHEMA=1 cargo test -p scribe-core` |
| The option tables in `docs/formats.md` | `SCRIBE_UPDATE_DOCS=1 cargo test -p scribe-core` |
| `tests/fixtures/*` | `UPDATE_FIXTURES=1 cargo test -p scribe-core` |

Redrawing the fixtures needs both a font and the models: the images are drawn
from the descriptions in `crates/scribe-core/tests/support/mod.rs`, and then
read with the engine to produce the layouts beside them. DejaVu Sans is what
the committed images were drawn with, looked for in the usual system font
directories; `SCRIBE_FIXTURE_FONT` names another. Another font draws readable
fixtures too, but every layout and every snapshot then has to be taken again.

## Documentation

```sh
cargo doc --workspace --no-deps --open
```

`missing_docs` is a warning across the workspace and CI denies warnings, so
every public item needs a doc comment. Examples in doc comments are run as
tests by `cargo test`.

Nothing in the repository — code, comments, documentation or commit messages —
should need anything outside the repository to be understood.

## Style

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Both are part of CI and both must be clean. rustfmt runs with its defaults.

## CI

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests:

- **Format, lint and test** — `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -D warnings`, `cargo test --workspace`, and the library's
  tests again with every image decoder turned on.
- **WebAssembly build and bindings** — `cargo check` for
  `wasm32-unknown-unknown` without `decode`, with it, and with every image
  decoder turned on, then `scripts/build-wasm.sh` and the Node tests against
  the package it writes.

Both jobs cache with `Swatinem/rust-cache`.
