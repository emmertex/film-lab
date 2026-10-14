#!/bin/sh
# Starts PhotoCraft with the Film Lab plug-ins installed and the sample negative open, for trying
# them by hand. Builds the plug-ins, fetches the sample negative if needed, then runs the companion's
# `manual_test` example, which stays attached until the app quits (Ctrl-C quits both).
# PhotoCraft: --photocraft BIN or $PHOTOCRAFT (default: the sibling ../photocraft checkout's
# release build, then the installed app). Both plug-ins go in with the companion's whole-image
# measurement of the negative, so the menu entries convert the way `film-lab app` does; --plain
# installs the bare modules instead. --negative FILE opens another scan. Other options pass
# through to the example.
set -eu
cd "$(dirname "$0")/.."
crates/photocraft/build.sh
case " $* " in
  *" --negative "*) ;;
  *) [ -f corpus/ektar-frame22-negative.png ] || scripts/fetch-samples.sh ;;
esac
exec cargo run -q -p film-lab-companion --example manual_test -- "$@"
