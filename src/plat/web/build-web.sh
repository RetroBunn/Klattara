#!/bin/sh
# Builds KlattPad for the web: the engine as WebAssembly, into www/klattara.wasm.
# www/ is then the whole site: serve it from any web server, or GitHub Pages.
#
#   rustup target add wasm32-unknown-unknown
#   ./build-web.sh
#   python3 -m http.server -d www 8000     # then open http://localhost:8000
set -eu
cd "$(dirname "$0")"
cargo build --release -p klattara-web --target wasm32-unknown-unknown --manifest-path ../../../Cargo.toml
cp ../../../target/wasm32-unknown-unknown/release/klattara_web.wasm www/klattara.wasm
# Smaller, where Binaryen's wasm-opt is installed; it works as well without.
if command -v wasm-opt >/dev/null 2>&1; then
	wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int www/klattara.wasm -o www/klattara.wasm
fi
ls -l www/klattara.wasm
