#!/bin/sh
# Downloads the openly licensed negatives the README and the docs use into corpus/ (ignored by
# git), verifying each file's SHA-256. Attribution: docs/images/ATTRIBUTION.md.
set -eu
cd "$(dirname "$0")/.."
mkdir -p corpus
ua='film-lab/0.1 (sample fetch; https://github.com/)'
fetch() {
  name=$1; url=$2; sum=$3
  if [ ! -f "corpus/$name" ]; then
    echo "fetching $name"
    curl -sSL -A "$ua" -o "corpus/$name" "$url"
  fi
  echo "$sum  corpus/$name" | shasum -a 256 -c -
}
# LoMit, CC BY 4.0: a Kodak Ektar 100 strip photographed against daylight, and the author's positive.
fetch ektar-strip-negative.jpg 'https://upload.wikimedia.org/wikipedia/commons/6/61/Color_print_film_strip_01_-_negative.jpg' 81c418e851728447c4a7ba1945c1bb6fb9335b84d47828183271f304b062a526
fetch ektar-strip-positive-reference.jpg 'https://upload.wikimedia.org/wikipedia/commons/0/06/Color_print_film_strip_03_-_positive_with_tonal_correction.jpg' 77a68da832e66bfea7c1b1820bd95f016a9ab593644838f5d8637688b8e486ee
# Laerol, CC0: a flatbed scan of a 110 frame.
fetch ericht-110-negative.jpg 'https://upload.wikimedia.org/wikipedia/commons/8/88/110_format_negative_closeup_of_frame_Ericht_at_Blairgowrie.jpg' 72576a3f146c7eaaa887d2a252da7d2b7a1855836fe53e60ded15728c275a503
# Frame 22 of the strip (the church square) with the unexposed gaps beside it, no sprocket holes.
if command -v sips >/dev/null 2>&1; then
  sips -s format png -c 1156 1950 --cropOffset 1150 1758 corpus/ektar-strip-negative.jpg --out corpus/ektar-frame22-negative.png >/dev/null
  sips -s format png -c 1156 1950 --cropOffset 1150 1758 corpus/ektar-strip-positive-reference.jpg --out corpus/ektar-frame22-positive-reference.png >/dev/null
  echo "cropped frame 22 → corpus/ektar-frame22-negative.png and corpus/ektar-frame22-positive-reference.png"
else
  echo "no sips: crop frame 22 yourself (1950×1156 at 1758,1150) or convert the whole strip with --border 0"
fi
