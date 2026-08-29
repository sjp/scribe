#!/bin/sh
#
# Builds the WebAssembly package with wasm-pack, once for the browser and once
# for Node.
#
# The two builds differ only in the glue wasm-bindgen writes around the same
# module, so each gets its own directory under `crates/scribe-wasm/pkg`. The
# Node build is what the tests in `crates/scribe-wasm/tests/node` run against.
#
# Anything given on the command line is passed on to wasm-pack, so
# `scripts/build-wasm.sh --dev` builds without optimising.

set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
crate="$root/crates/scribe-wasm"

if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "wasm-pack is not installed. See https://rustwasm.github.io/wasm-pack/" >&2
    exit 1
fi

for target in web nodejs; do
    wasm-pack build "$crate" \
        --target "$target" \
        --out-dir "pkg/$target" \
        --out-name scribe \
        "$@"

    # wasm-pack names the npm package after the crate it built. The project,
    # the binary and the package are all called scribe; only the crate is
    # called scribe-wasm.
    manifest="$crate/pkg/$target/package.json"
    sed 's/"name": "scribe-wasm"/"name": "scribe"/' "$manifest" >"$manifest.renamed"
    mv "$manifest.renamed" "$manifest"
done

cat <<EOF

The browser build is in $crate/pkg/web and the Node build is in
$crate/pkg/nodejs. To run the tests that go through the Node build:

    node --test 'crates/scribe-wasm/tests/node/**/*.test.mjs'
EOF
