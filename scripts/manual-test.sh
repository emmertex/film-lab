#!/bin/sh
# Starts PhotoCraft with the Film Lab plug-in installed and the sample negative open, for trying
# it by hand. Builds the plug-in, fetches the sample negative if needed, then runs the companion's
# `manual_test` example, which stays attached until the app quits (Ctrl-C quits both).
# PhotoCraft: --photocraft BIN or $PHOTOCRAFT (default: the sibling ../photocraft checkout's
# release build, then the installed app). The plug-in goes in with the companion's whole-image
# measurement of the negative, so the menu entry converts the way `film-lab app` does; --plain
# installs the bare module instead. --negative FILE opens another scan. Other options pass
# through to the example.
set -eu
cd "$(dirname "$0")/.."
crates/photocraft/build.sh
case " $* " in
  *" --negative "*) ;;
  *) [ -f corpus/ektar-frame22-negative.png ] || scripts/fetch-samples.sh ;;
esac
exec cargo run -q -p film-lab-companion --example manual_test -- "$@"
