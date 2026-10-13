//! Film Lab for PhotoCraft: a filter plug-in for the PhotoCraft WebAssembly plug-in ABI v1
//! (`photocraft/docs/plugins.md`; built like `photocraft/examples/plugins/invert-rs`).
//!
//! `no_std`, no allocator, no imports. The host hands over one band of the layer as interleaved
//! `f32` samples in the document's colour model; this module converts the negative in place
//! with [`film_lab_core`], leaving alpha and fully transparent pixels alone.
//!
//! # The dialog
//!
//! A preset (the character of a film stock: layer contrast, mask colour, tone profile, colour
//! model, saturation, warmth, toning) and six sliders on top of it: exposure, brightness,
//! contrast, temperature, tint, saturation. Everything else is internal.
//!
//! # The measurement
//!
//! What belongs to the scan rather than the film (the film base, the black and white points,
//! a balance) is measured, not preset, and it has to be measured on the whole image: PhotoCraft
//! hands a plug-in the layer in bands of 256 rows in fresh instances with no whole-image pass,
//! the dialog's preview included, so nothing measured from one band agrees with the next. The
//! companion measures the whole scan and writes the result into this module before installing
//! it ([`MEASUREMENT_MARKER`]), and the conversion uses it whenever the module holds one.
//! Without one, the preset's typical mask colour stands in for the film base, with a fixed
//! range and no measurement at all: identical on every band, right when the scanning setup
//! matches the preset.
//!
//! A host may also pass `base_r/g/b` (0..255, the Info panel's values) in the JSON: they
//! replace the preset's base when the module holds no measurement. Other parameters of the
//! core are honoured from the JSON too (`range`, `neutral_*`, `encoding`) but have no dialog
//! control.
#![no_std]

use film_lab_core::{Analysis, Converter, Encoding, Preset, Sliders, Tables, params};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

/// The manifest PhotoCraft reads at install (`pc_manifest`); parameters in dialog order. No
/// parameter is named like a key of the host's `_image` object (see `film_lab_core::params`). The
/// version is this crate's, so a release tag only has to match `Cargo.toml`.
const MANIFEST: &str = concat!(
    r#"{
  "id": "io.github.smyrgeorge.film-lab",
  "name": "Film Lab…",
  "version": ""#,
    env!("CARGO_PKG_VERSION"),
    r#"",
  "kind": "filter",
  "author": "film-lab",
  "description": "Converts a scanned film negative into a positive: pick a film preset, adjust exposure, brightness, contrast, temperature, tint and saturation. The companion measures the whole image for it.",
  "params": {
    "preset":     {"type": "choice", "options": ["standard", "portra 160", "portra 400", "ektar 100", "gold 200", "colorplus 200", "ultramax 400", "superia 400", "c200", "pro 400h", "cinestill 800", "monochrome"], "default": "standard"},
    "exposure":   {"type": "number", "min": -5, "max": 5, "default": 0},
    "brightness": {"type": "number", "min": -50, "max": 50, "default": 0},
    "contrast":   {"type": "number", "min": -50, "max": 50, "default": 0},
    "temp":       {"type": "number", "min": -50, "max": 50, "default": 0},
    "tint":       {"type": "number", "min": -50, "max": 50, "default": 0},
    "saturation": {"type": "number", "min": -50, "max": 50, "default": 0}
  },
  "overlap": 0,
  "area": "content"
}"#
);

/// Starts the measurement block the companion fills in: the marker, then JSON padded with
/// spaces to [`MEASUREMENT_LEN`] bytes in all, with the keys `base_r/g/b`, `black_r/g/b`,
/// `white_r/g/b` (scan values, 0..255) and `balance_r/g/b`.
pub const MEASUREMENT_MARKER: &[u8] = b"FILMLAB-MEASURE-1:";
pub const MEASUREMENT_LEN: usize = 512;

const fn blank_block() -> [u8; MEASUREMENT_LEN] {
    let mut b = [b' '; MEASUREMENT_LEN];
    let mut i = 0;
    while i < MEASUREMENT_MARKER.len() {
        b[i] = MEASUREMENT_MARKER[i];
        i += 1;
    }
    b
}

/// The block lives in the module's data segment; `static mut` keeps the optimiser from folding
/// its initial (blank) contents into the code that reads it.
#[used]
static mut MEASUREMENT: [u8; MEASUREMENT_LEN] = blank_block();

/// The conversion's lookup tables, refilled on every call: 48 KiB, too much for the stack, and
/// what keeps a sample to a few dozen instructions under the host's interpreter.
static mut TABLES: Tables = Tables::blank();

fn measurement() -> &'static [u8] {
    // SAFETY: the block is only ever read, and only through this shared reference.
    let b = unsafe { &*core::ptr::addr_of!(MEASUREMENT) };
    b.get(MEASUREMENT_MARKER.len()..).unwrap_or(&[])
}

/// Bit 8 of the `format` argument: the last channel is alpha.
const HAS_ALPHA: u32 = 1 << 8;

/// Return codes of `pc_filter` (anything but 0 is reported by the host as an error).
const OK: i32 = 0;
const ERR_NO_CHANNELS: i32 = 1;
const ERR_UNSUPPORTED_MODE: i32 = 2;

#[unsafe(no_mangle)]
pub extern "C" fn pc_abi_version() -> u32 {
    1
}

/// `len << 32 | ptr` of the UTF-8 manifest JSON.
#[unsafe(no_mangle)]
pub extern "C" fn pc_manifest() -> u64 {
    ((MANIFEST.len() as u64) << 32) | MANIFEST.as_ptr() as u64
}

/// A block of `size` bytes: fresh pages from `memory.grow` (each run gets a fresh instance, so
/// nothing is ever freed). Returns 0 when the host's memory cap is reached.
#[unsafe(no_mangle)]
pub extern "C" fn pc_alloc(size: u32) -> u32 {
    let pages = (size as usize).div_ceil(65536);
    let old = core::arch::wasm32::memory_grow(0, pages);
    if old == usize::MAX { 0 } else { (old * 65536) as u32 }
}

/// A colour given as the Info panel shows it (0..255), as a host-encoded triple, or `None`
/// when unset (all 0). A grey document has one channel: the first key stands for it.
fn colour(json: &[u8], keys: [&str; 3], grey: bool) -> Option<[f32; 3]> {
    let v = |k: &str| params::number(json, k).map_or(0.0, |x| x / 255.0);
    let mut c = [v(keys[0]), v(keys[1]), v(keys[2])];
    if grey {
        c = [c[0]; 3];
    }
    if c == [0.0; 3] { None } else { Some(c) }
}

/// The companion's measurement, when the block holds one.
fn embedded(enc: Encoding, grey: bool) -> Option<(Analysis, [f32; 3])> {
    let b = measurement();
    let base = colour(b, ["base_r", "base_g", "base_b"], grey)?;
    let black = colour(b, ["black_r", "black_g", "black_b"], grey).unwrap_or(base);
    let white = colour(b, ["white_r", "white_g", "white_b"], grey)?;
    let bal = |k: &str| params::number(b, k).unwrap_or(1.0);
    let balance = if grey { [bal("balance_r"); 3] } else { [bal("balance_r"), bal("balance_g"), bal("balance_b")] };
    Some((Analysis::from_scan_values(base, black, white, enc)?, balance))
}

/// Converts `width × height` pixels of `channels` interleaved `f32` samples at `buf` in place.
///
/// # Safety
/// The host guarantees `buf..buf + len` is a block it got from `pc_alloc` holding `len / 4`
/// samples, and `params..params + params_len` the UTF-8 parameter JSON.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pc_filter(
    buf: *mut f32,
    len: u32,
    _width: u32,
    _height: u32,
    channels: u32,
    format: u32,
    params: *const u8,
    params_len: u32,
) -> i32 {
    let n = channels as usize;
    let alpha = format & HAS_ALPHA != 0;
    let colour_channels = if alpha { n.saturating_sub(1) } else { n };
    if colour_channels == 0 {
        return ERR_NO_CHANNELS;
    }
    if colour_channels != 1 && colour_channels != 3 {
        // CMYK / multichannel documents: not a negative we know how to read.
        return ERR_UNSUPPORTED_MODE;
    }
    // SAFETY: see the function's contract.
    let px = unsafe { core::slice::from_raw_parts_mut(buf, len as usize / 4) };
    // SAFETY: see the function's contract.
    let json = unsafe { core::slice::from_raw_parts(params, params_len as usize) };
    let grey = colour_channels == 1;
    let enc = params::string(json, "encoding").and_then(Encoding::parse).unwrap_or_else(|| Encoding::for_depth(format & 0xff));

    let preset = params::string(json, "preset").and_then(Preset::parse).unwrap_or_else(Preset::default_preset);
    let num = |key: &str| params::number(json, key).unwrap_or(0.0);
    let sliders = Sliders {
        exposure: num("exposure"),
        brightness: num("brightness"),
        contrast: num("contrast"),
        temp: num("temp"),
        tint: num("tint"),
        saturation: num("saturation"),
    };
    let mut p = preset.params(&sliders);
    if grey {
        p.monochrome = true;
    }
    if let Some(range) = params::number(json, "range") {
        p.range = range;
    }

    let opaque = |pix: &[f32]| !alpha || pix.get(n - 1).is_some_and(|a| *a > 0.0);
    // Grayscale (one colour channel) is read as a neutral triple.
    let rgb = |pix: &[f32]| {
        let c0 = pix.first().copied().unwrap_or(0.0);
        [c0, pix.get(1).copied().unwrap_or(c0), pix.get(2).copied().unwrap_or(c0)]
    };

    // The base and levels: the companion's measurement in the module, or the preset's mask.
    let from_base = |base_encoded: [f32; 3]| {
        Analysis::from_base([enc.decode(base_encoded[0]), enc.decode(base_encoded[1]), enc.decode(base_encoded[2])], p.film_gamma)
    };
    let typed_base = colour(json, ["base_r", "base_g", "base_b"], grey);
    let preset_base = if grey { [preset.base[1]; 3] } else { preset.base };
    let analysis = match embedded(enc, grey) {
        Some((a, balance)) => {
            p.balance = balance;
            a
        }
        // No measurement: the preset's base (or a typed one) with the preset's range, the same
        // on every band. Measuring the band itself would give every band its own levels.
        None => from_base(typed_base.unwrap_or(preset_base)),
    };
    let neutral = colour(json, ["neutral_r", "neutral_g", "neutral_b"], grey);
    let converter = Converter::new(&analysis, &p, enc, neutral);
    // SAFETY: a wasm instance runs one call at a time, and nothing else reaches the tables.
    let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
    converter.tables(tables);

    for pix in px.chunks_exact_mut(n) {
        if !opaque(pix) {
            continue;
        }
        let out = converter.convert_with(tables, rgb(pix));
        for (v, o) in pix.iter_mut().take(colour_channels).zip(out) {
            *v = o;
        }
    }
    OK
}
