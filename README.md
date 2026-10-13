<h1 align="center">Film Lab</h1>

<p align="center"><b>Scan the negative, pick the film, get the positive.</b></p>

<p align="center">Film-negative conversion as a plug-in for the <a href="https://getartcraft.com/apps">Crafting Apps</a>: <a href="https://github.com/storytold/photocraft">PhotoCraft</a> today, in pure Rust.</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="PhotoCraft WebAssembly plug-in" src="https://img.shields.io/badge/PhotoCraft-WebAssembly%20plug--in-2f7bf5?style=flat-square">
  <img alt="12 film presets" src="https://img.shields.io/badge/film%20presets-12-6b8e23?style=flat-square">
  <img alt="Status: alpha" src="https://img.shields.io/badge/status-alpha-d69e2e?style=flat-square">
</p>

> [!NOTE]
> **Under development.** The conversion, the companion and the film presets work end to end in
> PhotoCraft. The presets are starting points that may need further calibration (see
> [Presets](#presets)), and LightCraft support waits on a plug-in API.

## Screenshots

<table>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/images/01-negative.jpg" alt="A colour negative open in PhotoCraft: a frame of Kodak Ektar 100 with its orange mask, the unexposed gap beside it, the Info and History panels on the right" width="100%">
      <p><sub>The scan open in PhotoCraft: a frame of Kodak Ektar 100.</sub></p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/images/02-search.jpg" alt="PhotoCraft's command search with 'film lab' typed and Film Lab listed first under Filter › Plug-ins, the negative behind it" width="100%">
      <p><sub>Once installed, Film Lab sits under Filter › Plug-ins; ⌘F finds it by name.</sub></p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/images/03-dialog.jpg" alt="The Film Lab dialog: a Preset menu on Standard, sliders for exposure, brightness, contrast, temp, tint and saturation, a Preview checkbox, Cancel and OK; the converted frame previewed behind it" width="100%">
      <p><sub>The dialog: a film preset and six sliders, with a live preview. The measurement is never shown.</sub></p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/images/04-converted.jpg" alt="The converted positive filling the canvas, blue sky and cream church, with Film Lab… as the last step in the History panel" width="100%">
      <p><sub>The positive, as one history step.</sub></p>
    </td>
  </tr>
</table>

Every screenshot is the real application, driven over its control channel by
`scripts/screenshots.sh`, converting an openly licensed Ektar 100 negative
([attribution](docs/images/ATTRIBUTION.md)). The GIF under
[Use in PhotoCraft](#use-in-photocraft) shows the same flow in motion.

## What it does

Film Lab converts a scanned colour or black-and-white negative on the active layer into a
positive. The dialog offers a **film preset** and six sliders: exposure, brightness, contrast,
temperature, tint and saturation. What belongs to the scan rather than the film, the film base
and each channel's black and white point, is measured on the whole image by the companion,
because a plug-in only ever sees one band of the layer. The companion also converts whole rolls
without the app. The conversion itself is built from the physics of colour negatives and the
film makers' published data.

| Part | Role | Code |
|---|---|---|
| **Plug-in** | Converts the layer inside PhotoCraft: a film preset, six sliders, a live preview, one history step | [`crates/photocraft`](crates/photocraft) |
| **Companion** (`film-lab`) | Measures the whole scan, installs the plug-in with the measurement inside and applies it; converts single scans and rolls through `photocraft-cli` | [`crates/companion`](crates/companion) |
| **Presets** | Ten film stocks, from Kodak Portra to CineStill 800T, plus a neutral default and monochrome | [`crates/core/src/preset.rs`](crates/core/src/preset.rs) |
| **Core** | The conversion: levels in density space, colour, tone and toning; `no_std`, no allocator, host-agnostic | [`crates/core`](crates/core) |

## Hosts

| App | Plug-in API | Status |
|---|---|---|
| **PhotoCraft** | Sandboxed WebAssembly filter plug-ins (ABI v1): a `.wasm` module with no imports, run by wasmi on bands of the layer, installed with `plugin.install` or from the plug-ins folder, listed under Filter › Plug-ins. | Supported: `crates/photocraft`, with `crates/photocraft-harness` driving the ABI in tests. |
| **LightCraft** | None yet; the app offers a control protocol, an MCP server and a CLI. | Planned: a `crates/lightcraft` adapter over the same core once a plug-in API ships. |

## Layout

```text
Cargo.toml                   workspace: crates/* minus crates/photocraft
crates/
  core/                      film-lab-core: the conversion (no_std, no allocator, host-agnostic)
  photocraft/                film-lab-photocraft: the wasm plug-in (its own workspace, like PhotoCraft's example plug-in)
  photocraft-harness/        host-side checks for the plug-in (wasmi, fuel-metered like PhotoCraft)
  companion/                 film-lab: whole-image analysis and roll conversion through photocraft-cli
scripts/                     fetch-samples.sh (the openly licensed negatives), manual-test.sh (PhotoCraft with the plug-in and a scan), screenshots.sh (the README's images and GIF)
.github/workflows/ci.yml     fmt, clippy, tests, plug-in build
.github/workflows/release.yml  on a tag vX.Y.Z: the CI checks, then a GitHub release with the plug-in
```

`core` knows nothing about a host, its pixel layout or how parameters arrive; each host crate
adapts its app's ABI to the core. A LightCraft crate would be another such adapter.

## Build and test

```sh
rustup target add wasm32-unknown-unknown   # once

cargo test --workspace                     # core, plug-in harness (builds the plug-in), companion
cargo clippy --workspace --all-targets -- -D warnings
crates/photocraft/build.sh                 # → crates/photocraft/target/wasm32-unknown-unknown/release/film_lab_photocraft.wasm
```

`scripts/manual-test.sh` builds the plug-in and starts PhotoCraft on its control channel with
the plug-in installed and the sample negative open, then stays attached until the app quits:

```sh
scripts/manual-test.sh                       # Filter › Plug-ins › Film Lab… on the sample frame
scripts/manual-test.sh --plain               # the bare module, with no measurement inside
scripts/manual-test.sh --negative my-scan.tif --photocraft /path/to/photocraft
```

The plug-in is installed with the companion's measurement of the negative, so the menu entry
converts as the companion does; `--plain` installs the bare module, which falls back to the
preset's base. While the script runs, the companion reaches the same instance with
`film-lab app --dialog --root target/manual --token-file target/manual/token`.

## Release

The plug-in's version lives in `crates/photocraft/Cargo.toml`; the manifest PhotoCraft shows
takes it from there. To release, bump it, commit, and push a matching tag:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

`.github/workflows/release.yml` runs the CI checks, refuses a tag that differs from the crate's
version, builds the module with `--locked`, and publishes a GitHub release with
`film_lab_photocraft.wasm` and its SHA-256. A version with a suffix (`0.2.0-rc.1`) is a
pre-release. Run the workflow by hand (Actions › release › Run workflow) to get the module as a
build artifact without releasing anything.

## Use in PhotoCraft

![The flow: the negative, the command search finding Film Lab, its dialog with a preset and six sliders, the converted frame](docs/images/flow.gif)

Once installed, Film Lab appears under **Filter › Plug-ins › Film Lab…** with a dialog of a
preset and six sliders. The conversion needs the scan's film base and each channel's black and
white point, and PhotoCraft hands a plug-in the layer in strips with no whole-image pass, so the
measuring is done by the companion: it reads the open document, measures it as a whole,
installs the plug-in with the measurement written inside the module, and applies it. Opened
from the menu on a scan that has not been measured, the plug-in falls back to the preset's
typical film base (see [Without the companion](#without-the-companion)).

`film-lab` is the companion binary: `cargo build --release -p film-lab-companion` puts it in
`target/release/film-lab`. It installs the plug-in from the repository's build
(`crates/photocraft/build.sh`), or from `--plugin PATH` or `$FILM_LAB_WASM`.

**1. Start PhotoCraft with its control channel and open the scan.** The companion talks to the
app through that channel; the two folders are where the app may read and write files on its
behalf. Any RGB or greyscale document works, at 8, 16 or 32 bits.

```sh
photocraft --control 7878 --control-token-file ~/photocraft.token \
  --automation-read-root ~/scans --automation-write-root ~/scans
```

**2. Apply the plug-in with the companion.** One command, nothing to type into the app: it
saves a flattened copy of the document through the channel, measures it, installs the plug-in
with the measurement inside and runs it.

```sh
film-lab app --root ~/scans --token-file ~/photocraft.token --preset portra400
```

The document converts in place as one history step. With `--dialog` the plug-in's dialog opens
instead, with the measurement already in place, to choose the preset and adjust the sliders
against the live preview before OK:

```sh
film-lab app --dialog --root ~/scans --token-file ~/photocraft.token
```

The options of `film-lab analyze` apply here too: `--border 8` leaves the outer 8 % of the scan
out of the measurement, and `--wb neutral` balances the channel medians.

**3. Or open the dialog from the menu.** Once the companion has installed the plug-in,
Filter › Plug-ins › Film Lab… (or ⌘F and its name) opens the same dialog, carrying the
measurement of the scan last measured. This is the way to try another preset or slider setting
on that scan; for a new scan, run the companion again. The measurement is never shown in the
dialog: it travels inside the module. Changing the preset re-renders the preview.

**4. OK.** The positive replaces the layer's pixels as one history step ("Film Lab…" in the
History panel), so it undoes in one go; on a Smart Object it is a smart filter. Everything is
computed from the scan each time, so re-running with another preset is the way to edit.

The GIF and the screenshots convert one frame of a Kodak Ektar 100 strip that
[LoMit photographed against daylight](https://commons.wikimedia.org/wiki/File:Color_print_film_strip_01_-_negative.jpg)
(CC BY 4.0, cropped; see [docs/images/ATTRIBUTION.md](docs/images/ATTRIBUTION.md)).
`scripts/fetch-samples.sh` downloads it, the photographer's own positive and a CC0 scan of a
110 frame into `corpus/`; `scripts/screenshots.sh` regenerates every image and the GIF.

## Presets

Pick the stock the roll was, or the one it should have been.

| Preset | Film | Character |
|---|---|---|
| `standard` | a neutral lab scan | even layer contrast, neutral colour, the lab tone profile |
| `portra 160` | Kodak Portra 160 | the gentlest Portra: low contrast, pastel saturation, warm natural skin, soft warm highlights |
| `portra 400` | Kodak Portra 400 | the portrait standard: moderate contrast, warm skin, muted but full colour, warm highlights, cool shadows |
| `ektar 100` | Kodak Ektar 100 | the saturated one: steep layers, vivid reds and blues, clean neutrals, contrast to match |
| `gold 200` | Kodak Gold 200 | golden: warm yellow highlights, warm neutrals, saturated reds and yellows |
| `colorplus 200` | Kodak ColorPlus 200 | the everyday stock: warm like Gold but softer, warm shadows, slightly faded |
| `ultramax 400` | Kodak UltraMax 400 | punchy: saturated, contrasty, warm, strong blues |
| `superia 400` | Fujifilm Superia X-TRA 400 | Fuji's greens: cooler than Kodak, vivid, teal shadows, a touch of magenta in skin |
| `c200` | Fujifilm C200 | clean and cool: soft contrast, restrained saturation, a green-cyan lean in the shadows |
| `pro 400h` | Fujifilm Pro 400H | pastel: flat layers, airy highlights, mint greens, cyan shadows, soft skin |
| `cinestill 800` | CineStill 800T | tungsten cine stock in daylight: low contrast, muted colour, teal shadows against warm highlights |
| `monochrome` | black-and-white stocks | a paper-like straight line with some contrast, no colour |

The film names are trademarks of their makers (Eastman Kodak, FUJIFILM, CineStill) and identify
the stock a recipe takes after. The recipes are this project's own; nothing here is endorsed by
or affiliated with those companies. On the command line any spelling works (`portra400`,
`Portra 400`); the menu shows the names capitalised.

> [!NOTE]
> The presets may need further calibration. They are built from the stocks' published
> characteristics and the way they are known to render, not measured from the films, and only
> Portra 400 has been checked against a lab scan so far. How a stock renders also depends on
> the scanner, the exposure and the lab, so treat each preset as a starting point and tune it
> against your own reference scans. Corrections, with a scan and a trusted positive of the
> same frame, are welcome.

A preset is a small table in `crates/core/src/preset.rs`: the density per log-exposure of each
layer, the typical mask colour, a tone profile, a colour model, and saturation, warmth and
toning offsets.

## Without the companion

- **No measurement.** With no companion measurement in the module, the preset's typical mask
  colour stands in for the film base, with a fixed range. The result is identical on every
  strip, so seam-free at any size, and right when the scanning setup matches the preset. The
  plug-in never measures the pixels it is given: PhotoCraft hands it the layer in strips of 256
  rows, each in a fresh sandbox, the preview included, so a per-strip measurement would give
  every strip its own levels and show as blocks. A host can pass `base_r/g/b` (the Info panel's
  values on the unexposed border) in the JSON to replace the preset's base.
- **Installing by hand.** `crates/photocraft/build.sh` builds the module; `plugin.install
  {"path": …}` over the control channel, or a copy in the folder set in Edit › Preferences ›
  Plug-ins, installs it without a measurement. Filter › Plug-ins › Film Lab… then runs with
  the preset's base.

## Headless

The companion runs the same measurement and the plug-in through `photocraft-cli`, for one scan
or for a roll with one shared measurement:

```sh
cargo run -p film-lab-companion -- convert scan.tif -o positive.tif --preset portra400
cargo run -p film-lab-companion -- convert roll/ -o out/ --master roll/frame07.tif
```

`film-lab analyze` prints the measurement, `sample` reads a scan colour as the Info panel shows
it, and `compare` measures a result against a reference positive. The plug-in alone, with no
measurement:

```sh
crates/photocraft/build.sh
photocraft-cli run scan.tif \
  --cmd plugin.install --params '{"path": "/abs/path/film_lab_photocraft.wasm"}' \
  --cmd plugin.run --params '{"id": "io.github.smyrgeorge.film-lab", "params": {"preset": "standard"}}' \
  --out positive.tif
```

Measured with `film-lab compare`: on a lab-scanned Portra 400 frame, the companion's analysis
with an 8 % border buffer lands every channel's mean within about 1 % of the lab's positive; on
the Ektar 100 frame above, against the photographer's own positive, `--wb neutral` lands within
about 4 %.

Parameters, the measurement block and the conversion are described in
[`crates/photocraft/README.md`](crates/photocraft/README.md), and the companion in
[`crates/companion/README.md`](crates/companion/README.md). To try the plug-in without a scan,
`cargo run -p film-lab-photocraft-harness --example synthetic_negative -- DIR` writes a
synthetic negative together with the scene it came from.

## Design notes

- **PhotoCraft runs each band in a fresh instance with no whole-image pass**, so the companion
  measures the whole scan and the measurement reaches every band the same way: inside the
  module. The host drops parameters a manifest does not declare, so the numbers cannot travel
  as hidden parameters; the companion writes them into a reserved block of the plug-in before
  installing it.
- **PhotoCraft draws one flat dialog from the manifest**, with no sections or hiding, so the
  dialog is a preset and six sliders; the film's character lives in the preset tables.
- **The host meters instructions per sample and interprets the module**, so the per-channel
  transfer functions are sampled into lookup tables once per band and interpolated per pixel,
  with no transcendental call per sample. The harness test pins the budget, the core's tests
  the accuracy.
- **Plug-ins see the document's own colour model** (RGB, grey, Lab, CMYK…) as normalised `f32`,
  with no colour management; the plug-in refuses CMYK and multichannel documents.
- **Conventions** follow the Crafting Apps: pure Rust, no panics in non-test code
  (`unwrap`, `expect` and `panic` are denied in every production crate), and no `unsafe` outside
  the plug-in's ABI surface.

## References

- PhotoCraft: `docs/plugins.md` (ABI v1), `examples/plugins/invert-rs`, `crates/plugins` (limits,
  manifest validation, band splitting)
- LightCraft: `docs/gaps.md`, `docs/target-app-parity.md` (plug-in ecosystem status)
