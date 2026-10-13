#!/bin/sh
# Builds the Film Lab PhotoCraft plug-in.
# Needs the wasm32-unknown-unknown target: `rustup target add wasm32-unknown-unknown`.
# Extra arguments go to cargo (the release workflow passes `--locked`).
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown "$@"
out=target/wasm32-unknown-unknown/release/film_lab_photocraft.wasm
echo "built $out ($(wc -c < "$out" | tr -d ' ') bytes)"
