#!/bin/sh
# Regenerates docs/images (the README's screenshots and flow.gif) by driving PhotoCraft.
# Builds the plug-in, fetches the sample negative if needed, then runs the companion's
# `screenshots` example. PhotoCraft: --photocraft BIN or $PHOTOCRAFT (default: the sibling
# ../photocraft checkout's release build, then the installed app). Other options pass through.
set -eu
cd "$(dirname "$0")/.."
crates/photocraft/build.sh
[ -f corpus/ektar-frame22-negative.png ] || scripts/fetch-samples.sh
cargo run -q -p film-lab-companion --example screenshots -- "$@"
