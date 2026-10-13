# Film Lab for PhotoCraft

A WebAssembly filter plug-in for PhotoCraft's plug-in ABI v1 (`photocraft/docs/plugins.md`),
built like the upstream example `examples/plugins/invert-rs`: `no_std`, no allocator, no imports.
It converts a scanned film negative on the active layer into a positive: a film preset and six
sliders.

This crate is **its own cargo workspace** (not a member of the repository workspace): it only
builds for `wasm32-unknown-unknown`, and `.cargo/config.toml` makes that the default target here.

## Build

```sh
rustup target add wasm32-unknown-unknown
./build.sh        # cargo build --release --target wasm32-unknown-unknown
```

Output: `target/wasm32-unknown-unknown/release/film_lab_photocraft.wasm`.

## Install

```text
plugin.install {"path": "/abs/path/film_lab_photocraft.wasm"}
```

Or copy the `.wasm` into the folder set in Edit › Preferences › Plug-ins (with *Use Additional
Plug-ins Folder* on). It then appears as **Filter › Plug-ins › Film Lab…** with a generated
dialog and live preview.

## Workflow

Screenshots of these steps in the app are in the [repository README](../../README.md#use-in-photocraft).

**Automatic** (the companion, `crates/companion`): with PhotoCraft started on its control channel
and the scan open, `film-lab app` measures the whole document, writes the measurement into the
module it installs, and runs the plug-in; with `--dialog` it opens the dialog instead, for
choosing the preset and adjusting the sliders with the live preview before OK.

**In the dialog alone**: Filter › Plug-ins › Film Lab…, pick a preset, adjust. The dialog
carries whatever measurement the module holds (the companion's last one); with none, the
preset's typical mask colour stands in for the film base.

A conversion is one undo step; on a smart object it is recorded as a smart filter and re-run on
re-render.

## Parameters

| Parameter | Range | What |
|---|---|---|
| `preset` | `standard` (default), `portra 160`, `portra 400`, `ektar 100`, `gold 200`, `colorplus 200`, `ultramax 400`, `superia 400`, `c200`, `pro 400h`, `cinestill 800`, `monochrome` | The film's character: layer contrast, mask colour, tone profile, colour model, saturation, warmth, toning (`crates/core/src/preset.rs`) |
| `exposure` | −5..5 stops | Overall brightness |
| `brightness` | −50..50 | Mid-tone gamma; black and white stay put |
| `contrast` | −50..50 | Added to the preset's contrast |
| `temp`, `tint` | −50..50 | Warmer / cooler, magenta / green, on top of the preset's |
| `saturation` | −50..50 | On top of the preset's |

Not in the dialog, but read from the JSON when a host passes them: `base_r/g/b` (0..255, the Info
panel's values on the unexposed border, replacing the preset's base when the module holds no
measurement), `neutral_r/g/b` (a scan
colour that should come out grey), `range` (stops rendered between black and white; 3 by
default, 6 is scene-linear), and `encoding` (`srgb` or `linear`; otherwise 8- and 16-bit
documents are sRGB, 32-bit linear).

### The measurement, or the preset's base

ABI v1 runs every band of the layer (a strip 256 rows tall, up to 1280 px wide for RGB) in a
fresh instance with no whole-image pass, the dialog's preview included, so anything measured
from the pixels a band holds differs from band to band and shows as blocks. The plug-in
therefore measures nothing itself.

- **With a measurement** in the module (the companion's: film base, black and white points,
  balance, all measured on the whole scan) nothing depends on the band, so large layers
  convert without seams.
- **Without one**, the preset's typical mask colour is the base and a fixed range of stops
  leads to white: identical on every band, seam-free, right when the scanning setup matches.

### The measurement block

The host drops parameters a manifest doesn't declare, so the measurement can't be a hidden
parameter. The module has a 512-byte block starting with `FILMLAB-MEASURE-1:`; the companion
writes the measurement there as JSON (`base_*`, `black_*`, `white_*` as scan values 0..255,
`balance_*`) before installing the module, and the plug-in reads it. The harness tests patch the
block the same way.

## The conversion

Clean-room, from the physics of negatives (`crates/core`): density grows by the layer's gamma
(about 0.6 for colour negative film, per layer in the presets) per stop of scene exposure, so
relative exposure is `(base / scan)^(1/γ)` per channel, with the border as base. Dividing by the base per channel also neutralises the orange mask. Levels work in
density space: each channel's range between its black and white points is mapped onto the same
`range` of stops, so a scan whose blue runs far denser than its red (every 8-bit scan of an
orange-masked negative) still comes out balanced, with black and white at 0 and 1. White
balance, the colour model, pre-saturation, mid-tone balance, exposure and the white / black
points act in linear light; brightness and the per-channel balance are gammas; the tone
profile, contrast, lights / darks, fade / glow and the soft ends act in sRGB-encoded space, then
the toning and saturation, then sharpening.

Measured with `film-lab compare`: on a lab-scanned Portra 400 frame with its lab positive,
the companion's analysis with a border buffer of 8 % and the defaults lands every channel's
mean within about 1 %; on an openly licensed Ektar 100 frame photographed against daylight (the
README's example), `--wb neutral` or a neutral sample on a cloud lands within about 4 % of the
author's own positive.

The host meters about 4,000 instructions per sample and runs the module in an interpreter, so
the conversion samples each channel's transfer functions into lookup tables once per band
(`film_lab_core::Tables`, 48 KiB in static memory) and interpolates per sample: no `powf` per
pixel, and a positive within a thousandth of the exact one. The harness tests check the budget
and the core's tests the accuracy.

## Return codes

| Code | Meaning |
|---|---|
| 0 | converted |
| 1 | no colour channel (alpha only) |
| 2 | unsupported colour mode (CMYK / multichannel) |

## Test

`cargo test -p film-lab-photocraft-harness` (from the repository root) builds this crate and
drives its exports with wasmi, the interpreter PhotoCraft embeds: ABI version, the manifest and
its seven controls, round trips of negatives synthesised from the core's own film model (a typed
base, the embedded measurement and the fallback without one, greyscale, linear documents),
every preset and slider, the neutral sample, hostile parameters, and the
instruction budget. The `synthetic_negative` example writes a negative to run through
`photocraft-cli` by hand.
