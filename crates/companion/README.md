# film-lab: the companion

What a plug-in band can't do on its own. PhotoCraft runs a filter plug-in on bands of the layer
with no whole-image pass, so the plug-in cannot measure the scan itself. This binary
analyses the whole scan up front, the way a lab-style converter's "Convert" step does, and hands
the plug-in the result as parameters that are identical on every band. It then drives
`photocraft-cli` to run the plug-in on one scan or a whole roll.

```sh
cargo run -p film-lab-companion -- app --dialog --root ~/scans --token-file ~/photocraft.token   # in the running app
cargo run -p film-lab-companion -- analyze scan.tif                 # the parameters, as JSON
cargo run -p film-lab-companion -- convert scan.tif -o positive.tif  # analyse and convert
cargo run -p film-lab-companion -- convert roll/ -o out/ --master roll/frame07.tif --preset vivid
cargo run -p film-lab-companion -- compare positive.tif reference.tif  # how close two positives are
cargo run -p film-lab-companion -- sample scan.tif 25 600 --size 15     # a scan colour, as the Info panel shows it
```

The measurement never goes through the dialog: the companion writes it (film base, black and
white points, balance) into a reserved block of the plug-in's module and installs that copy, so
the dialog shows only the preset and the sliders, and every band of the layer reads the same
numbers. `app` and `convert` both do this; `analyze` prints what would be written.

`app` is the automatic mode inside PhotoCraft. Start the app with its control channel
(`photocraft --control 7878 --control-token-file F --automation-read-root DIR
--automation-write-root DIR`) and open the scan; `film-lab app --root DIR --token-file F`
then saves a flattened copy of the document through the channel, analyses it, installs the
plug-in from bytes and runs it with the analysis as one history step. With `--dialog` it opens
the plug-in's dialog with every value filled in instead, for adjusting with the live preview
before OK. `--connect` picks another port; the token also comes from
`$PHOTOCRAFT_CONTROL_TOKEN_FILE` or `$PHOTOCRAFT_CONTROL_TOKEN`.

| Option | What |
|---|---|
| `--border N` | percent of each edge left out of the analysis (film holder, sprocket holes); default 5 |
| `--black-clip N`, `--white-clip N` | percent of pixels allowed below black / above white; default 0.3 |
| `--wb auto\|neutral\|warm\|cool` | `auto` (default): black and white neutral per channel, the scene's own colour kept; `neutral`: the channel medians balanced too (per-channel gammas, `balance_*`), which over-corrects scenes dominated by one colour; `warm` / `cool`: auto plus a temperature offset |
| `--master FILE` | for a folder: the scan whose analysis applies to the whole roll (the "match" workflow) |
| `--preset NAME` | the film preset, spelled any way (`portra400`, `Portra 400`): `standard`, `portra 160`, `portra 400`, `ektar 100`, `gold 200`, `colorplus 200`, `ultramax 400`, `superia 400`, `c200`, `pro 400h`, `cinestill 800`, `monochrome` |
| `--set KEY=VALUE` | a slider on top: `exposure=0.5`, `brightness=-10`, `contrast=10`, `temp=8`, `tint=-4`, `saturation=10` |
| `--format EXT` | output format for a folder |
| `--photocraft-cli P`, `--plugin P` | where `photocraft-cli` and the `.wasm` are; also `$PHOTOCRAFT_CLI`, `$FILM_LAB_WASM` |

The analysis samples a regular grid of at most a million pixels, builds the same histogram the
plug-in uses, and reads the film base (a high quantile of brightness), black (the quantile the
black clip leaves) and white (the white clip's quantile) per channel, on the 0..255 scale the
plug-in's `base_*`, `black_*` and `white_*` parameters take. Scans are decoded with pure-Rust
decoders: TIFF (8- and 16-bit), PNG, JPEG.

A roll converts through `photocraft-cli batch` with one action list (install, run) and one set of
parameters; `--master` picks the frame analysed, and `--set` edits apply to every frame, which is
what syncing settings across a roll does.

`sample` prints the average colour of a square at a pixel, on the 0..255 scale the plug-in's
`base_*` and `neutral_*` parameters take: the film border for the base, or something white in
the scene (a cloud, a wall) to pass as the neutral sample when a conversion keeps a cast.

`compare` resamples both images to one grid and prints per-channel mean and percentiles, a 4 × 4
grid of mean colours and the differences: the tool used to tune the defaults against reference
positives. On a lab-scanned Portra 400 frame, `convert --border 8` lands every channel's mean
within about 1 % of the lab's positive; on the openly licensed Ektar 100 frame from
`scripts/fetch-samples.sh`, whose unexposed gap is overexposed by the daylight behind the strip,
`convert --border 8 --wb neutral` lands within about 4 % of the author's positive.

The `screenshots` example (`scripts/screenshots.sh`) drives PhotoCraft through the automatic
mode and writes the repository README's screenshots and `flow.gif`; it is how the images are
kept current after a change.
