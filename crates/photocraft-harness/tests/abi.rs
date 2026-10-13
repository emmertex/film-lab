//! The PhotoCraft plug-in honours ABI v1 (`photocraft/docs/plugins.md`), converts negatives
//! synthesised from the core's own film model back into the scene, and its presets, sliders and
//! measurement sources behave as documented.

use film_lab_core::{PRESETS, RANGE_STOPS, color, film};
use film_lab_photocraft_harness::{FUEL_PER_SAMPLE, Host, MODE_CMYK, MODE_GRAY, MODE_RGB, build_plugin, embed_measurement, format_code};
use serde_json::Value;

/// A film base in linear light (orange mask).
const BASE: [f32; 3] = [0.6, 0.35, 0.15];

fn host() -> Host {
    let wasm = std::fs::read(build_plugin().unwrap()).unwrap();
    Host::load(&wasm).unwrap()
}

/// The linear scan value of scene brightness `s` (0..1 of the positive) on channel `ch`.
fn negative_linear(s: f32, ch: usize) -> f32 {
    film::transmittance(1.0 + s * (libm::exp2f(RANGE_STOPS) - 1.0), BASE[ch])
}

/// The sRGB-encoded scan value of scene brightness `s` on channel `ch`.
fn negative(s: f32, ch: usize) -> f32 {
    color::srgb_encode(negative_linear(s, ch))
}

/// The `base_*` values for [`BASE`] as the Info panel would show them.
fn base_params() -> String {
    let v = |ch: usize| color::srgb_encode(BASE[ch]) * 255.0;
    format!(r#""base_r": {}, "base_g": {}, "base_b": {}"#, v(0), v(1), v(2))
}

/// An RGBA band of `scene` values (grey, one per pixel), all opaque.
fn rgba_band(scene: &[f32]) -> Vec<f32> {
    scene.iter().flat_map(|s| [negative(*s, 0), negative(*s, 1), negative(*s, 2), 1.0]).collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// The scene-linear reference run: the standard preset with a linear tone is not in the dialog,
/// so these pass the hidden `range` and a typed base (the module holds no measurement).
fn reference(extra: &str) -> String {
    format!(r#"{{"preset": "standard", {}, "range": 6{extra}}}"#, base_params())
}

/// Runs one grey scene value through the plug-in.
fn one(h: &mut Host, s: f32, params: &str) -> [f32; 3] {
    let mut px = rgba_band(&[s]);
    assert_eq!(h.filter(&mut px, 1, 1, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0);
    [px[0], px[1], px[2]]
}

#[test]
fn exports_abi_v1_and_a_short_manifest() {
    let mut h = host();
    assert_eq!(h.abi_version().unwrap(), 1);
    let text = h.manifest().unwrap();
    assert!(text.len() <= 64 * 1024);
    let m: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(m["id"], "io.github.smyrgeorge.film-lab");
    assert_eq!(m["name"], "Film Lab…");
    assert_eq!(m["kind"], "filter");
    let params = m["params"].as_object().unwrap();
    // Keys in manifest order (a key's name may recur later as an option value: first occurrence).
    let mut order: Vec<&str> = Vec::new();
    for k in text.split('"').filter(|k| params.contains_key(*k)) {
        if !order.contains(&k) {
            order.push(k);
        }
    }
    assert_eq!(order, ["preset", "exposure", "brightness", "contrast", "temp", "tint", "saturation"], "seven controls, the preset first");
    let presets: Vec<&str> = m["params"]["preset"]["options"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(presets, PRESETS.iter().map(|p| p.name).collect::<Vec<_>>(), "the dialog lists the core's presets");
    assert_eq!(m["params"]["preset"]["default"], "standard");
    let reserved = ["x", "y", "width", "height", "overlap", "mode", "depth", "alpha", "channels", "canvasWidth", "canvasHeight", "id", "layer"];
    for key in params.keys() {
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !key.starts_with('_'), "bad name {key}");
        assert!(!reserved.contains(&key.as_str()), "{key} clashes with the host's keys");
    }
    let wasm = std::fs::read(build_plugin().unwrap()).unwrap();
    assert_eq!(wasm.windows(b"FILMLAB-MEASURE-1:".len()).filter(|w| *w == b"FILMLAB-MEASURE-1:").count(), 1, "one measurement block");
    assert!(h.alloc(1 << 20).unwrap() > 0);
}

#[test]
fn a_typed_base_recovers_the_scene_from_a_synthetic_negative() {
    let mut h = host();
    let scene = [0.0, 0.02, 0.05, 0.1, 0.18, 0.3, 0.5, 0.7, 0.85, 1.0, 0.4, 0.6];
    let mut px = rgba_band(&scene);
    // A transparent pixel with scan values that must come back untouched.
    px.extend([0.42, 0.42, 0.42, 0.0]);
    // The standard preset's lab tone is a contrast curve; a flat line comes from the
    // black-and-white preset's linear tone, which is monochrome, so use the core: a 0-contrast
    // check through `contrast: -15` is not identity either. Compare against the core instead.
    let want: Vec<f32> = {
        use film_lab_core::{Analysis, Converter, Encoding, Preset, Sliders};
        let preset = Preset::parse("standard").unwrap();
        let mut p = preset.params(&Sliders::default());
        p.range = 6.0;
        let c = Converter::new(&Analysis::from_base(BASE, p.film_gamma), &p, Encoding::Srgb, None);
        scene.iter().map(|s| c.convert([negative(*s, 0), negative(*s, 1), negative(*s, 2)])[1]).collect()
    };
    assert_eq!(h.filter(&mut px, 13, 1, 4, format_code(8, true, MODE_RGB), &reference(r#", "_image": {"x": 0, "y": 0, "depth": 8}"#)).unwrap(), 0);
    for (i, s) in scene.iter().enumerate() {
        let got = &px[i * 4..i * 4 + 4];
        assert!(got[..3].iter().all(|v| close(*v, want[i], 0.004)), "scene {s}: got {got:?}, want {}", want[i]);
        assert_eq!(got[3], 1.0, "alpha untouched");
    }
    assert_eq!(&px[48..], &[0.42, 0.42, 0.42, 0.0], "transparent pixel untouched");
    // Black at the base, white at the top, neutral throughout.
    assert!(px[0] < 0.01 && px[9 * 4] > 0.99 && close(px[5 * 4], px[5 * 4 + 2], 1e-3), "{:?}", &px[..40]);
}

#[test]
fn the_measurement_travels_inside_the_module() {
    let wasm = std::fs::read(build_plugin().unwrap()).unwrap();
    // Black at the base, white at the scan value of a scene 5 stops up, no balance.
    let five = |ch: usize| color::srgb_encode(film::transmittance(libm::exp2f(5.0), BASE[ch])) * 255.0;
    let base = |ch: usize| color::srgb_encode(BASE[ch]) * 255.0;
    let json = format!(
        r#"{{"base_r": {}, "base_g": {}, "base_b": {}, "black_r": {}, "black_g": {}, "black_b": {}, "white_r": {}, "white_g": {}, "white_b": {}, "balance_r": 1, "balance_g": 1, "balance_b": 1}}"#,
        base(0),
        base(1),
        base(2),
        base(0),
        base(1),
        base(2),
        five(0),
        five(1),
        five(2)
    );
    let patched = embed_measurement(&wasm, &json).unwrap();
    assert_eq!(patched.len(), wasm.len(), "patching keeps the module's size");
    let mut filled = Host::load(&patched).unwrap();
    let mut blank = Host::load(&wasm).unwrap();
    // The block is read whenever the module holds one: a 5-stop range renders the scene brighter
    // than the base-only levels do, and a typed base in the JSON no longer counts.
    let measured = one(&mut filled, 0.3, r#"{"preset": "standard", "range": 6}"#)[1];
    let typed = one(&mut blank, 0.3, &reference(""))[1];
    assert!(measured > typed + 0.05, "{measured} vs {typed}");
    assert!(close(one(&mut filled, 0.3, &reference(""))[1], measured, 1e-5), "the block wins over base_*");
    // Without a block and without a typed base, the preset's own mask: finite, and not the
    // typed one; never a measurement of the band (which would differ from band to band).
    let fallback = one(&mut blank, 0.3, r#"{"preset": "standard", "range": 6}"#);
    assert!(fallback.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{fallback:?}");
    assert!(!close(fallback[1], typed, 1e-3), "the preset's own mask is not the typed base: {fallback:?} vs {typed}");
    assert!(embed_measurement(&wasm, &"x".repeat(600)).is_err());
    assert!(embed_measurement(b"\0asm", "{}").is_err());
}

#[test]
fn presets_and_sliders_change_the_output_as_documented() {
    let mut h = host();
    let standard = one(&mut h, 0.3, &reference(""));
    assert!(close(standard[0], standard[2], 1e-3), "standard is neutral: {standard:?}");
    // Sliders.
    assert!(one(&mut h, 0.3, &reference(r#", "exposure": 1"#))[1] > standard[1]);
    assert!(one(&mut h, 0.3, &reference(r#", "brightness": 20"#))[1] > standard[1]);
    let warm = one(&mut h, 0.3, &reference(r#", "temp": 40"#));
    assert!(warm[0] > warm[1] && warm[1] > warm[2], "{warm:?}");
    let magenta = one(&mut h, 0.3, &reference(r#", "tint": 40"#));
    assert!(magenta[1] < magenta[0], "{magenta:?}");
    let grey = one(&mut h, 0.3, &reference(r#", "saturation": -50, "temp": 40"#));
    assert!(close(grey[0], grey[2], 1e-3), "{grey:?}");
    let dark_standard = one(&mut h, 0.05, &reference(""))[1];
    assert!(one(&mut h, 0.05, &reference(r#", "contrast": 40"#))[1] < dark_standard, "more contrast drops a dark tone");
    // Presets: each is a different, finite rendering; the warm ones are warm, the mono one grey.
    let run = |h: &mut Host, preset: &str| one(h, 0.3, &format!(r#"{{"preset": "{preset}", {}, "range": 6}}"#, base_params()));
    let golden = run(&mut h, "gold 200");
    assert!(golden[0] > golden[2], "{golden:?}");
    let cool = run(&mut h, "superia 400");
    assert!(cool[0] < golden[0], "{cool:?} vs {golden:?}");
    let mono = run(&mut h, "monochrome");
    assert!(mono[0] == mono[1] && mono[1] == mono[2], "{mono:?}");
    let cinema = one(&mut h, 0.0, &format!(r#"{{"preset": "cinestill 800", {}}}"#, base_params()));
    assert!(cinema[1] > 0.0, "the cine stock lifts black: {cinema:?}");
    let mut seen = std::collections::BTreeSet::new();
    for p in PRESETS {
        let out = run(&mut h, p.name);
        assert!(out.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{}: {out:?}", p.name);
        seen.insert(((out[0] * 1e4) as i32, (out[1] * 1e4) as i32, (out[2] * 1e4) as i32));
    }
    assert_eq!(seen.len(), PRESETS.len(), "every preset renders differently");
    // An unknown preset is the standard one; the preset's own mask is the base without a typed one.
    let unknown = one(&mut h, 0.3, r#"{"preset": "kodachrome", "range": 6}"#);
    assert!(unknown.iter().all(|v| v.is_finite()), "{unknown:?}");
}

#[test]
fn a_neutral_sample_removes_a_cast() {
    let mut h = host();
    let cast = |s: f32| [color::srgb_encode(negative_linear(s, 0) * 0.8), negative(s, 1), negative(s, 2)];
    let run = |h: &mut Host, extra: &str| {
        let c = cast(0.4);
        let mut px = vec![c[0], c[1], c[2], 1.0];
        assert_eq!(h.filter(&mut px, 1, 1, 4, format_code(8, true, MODE_RGB), &reference(extra)).unwrap(), 0);
        [px[0], px[1], px[2]]
    };
    let before = run(&mut h, "");
    assert!(before[0] > before[1] + 0.02, "{before:?}");
    let c = cast(0.4);
    let after = run(&mut h, &format!(r#", "neutral_r": {}, "neutral_g": {}, "neutral_b": {}"#, c[0] * 255.0, c[1] * 255.0, c[2] * 255.0));
    assert!(close(after[0], after[1], 3e-3) && close(after[1], after[2], 3e-3), "{after:?}");
}

#[test]
fn linear_documents_are_read_and_written_linear() {
    let mut h = host();
    let mut px = vec![negative_linear(0.5, 0), negative_linear(0.5, 1), negative_linear(0.5, 2)];
    // Base values are in the document's encoding: linear here, so BASE × 255.
    let params = format!(
        r#"{{"preset": "standard", "base_r": {}, "base_g": {}, "base_b": {}, "range": 6}}"#,
        BASE[0] * 255.0,
        BASE[1] * 255.0,
        BASE[2] * 255.0
    );
    assert_eq!(h.filter(&mut px, 1, 1, 3, format_code(32, false, MODE_RGB), &params).unwrap(), 0);
    let mut want = rgba_band(&[0.5]);
    assert_eq!(h.filter(&mut want, 1, 1, 4, format_code(8, true, MODE_RGB), &reference("")).unwrap(), 0);
    assert!(px.iter().zip(&want).all(|(a, b)| close(*a, color::srgb_decode(*b), 0.004)), "{px:?} vs {want:?}");
}

#[test]
fn grayscale_converts_and_cmyk_is_refused() {
    let mut h = host();
    // A grey document: one channel, the base typed into base_r.
    let base = 0.5f32;
    let scan = |s: f32| color::srgb_encode(film::transmittance(1.0 + s * (libm::exp2f(RANGE_STOPS) - 1.0), base));
    let mut gray = vec![scan(0.0), scan(0.5), scan(1.0)];
    let params = format!(r#"{{"preset": "standard", "base_r": {}, "range": 6}}"#, color::srgb_encode(base) * 255.0);
    assert_eq!(h.filter(&mut gray, 3, 1, 1, format_code(8, false, MODE_GRAY), &params).unwrap(), 0);
    assert!(close(gray[0], 0.0, 0.004) && gray[1] > 0.5 && close(gray[2], 1.0, 0.004), "{gray:?}");
    let mut cmyk = vec![0.1; 5];
    assert_eq!(h.filter(&mut cmyk, 1, 1, 5, format_code(8, true, MODE_CMYK), "{}").unwrap(), 2);
    let mut none = vec![1.0];
    assert_eq!(h.filter(&mut none, 1, 1, 1, format_code(8, true, MODE_GRAY), "{}").unwrap(), 1);
}

#[test]
fn garbage_parameters_fall_back_to_defaults() {
    let mut h = host();
    let mut px = rgba_band(&[0.3]);
    let code =
        h.filter(&mut px, 1, 1, 4, format_code(8, true, MODE_RGB), r#"{"preset": ["x"], "exposure": "no", "base_r": null, "temp": 1e99"#).unwrap();
    assert_eq!(code, 0);
    assert!(px.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{px:?}");
}

#[test]
fn a_full_band_fits_the_hosts_instruction_budget() {
    let mut h = host();
    // 128 × 128 RGBA: enough samples that the per-sample cost dominates the fixed budget.
    let scene: Vec<f32> = (0..128 * 128).map(|i| (i % 128) as f32 / 127.0).collect();
    let samples = scene.len() as u64 * 4;
    // The most expensive path: a toned warm preset, every slider, a neutral sample.
    let costly = r#"{"preset": "gold 200", "exposure": 0.3, "brightness": 20, "contrast": 10, "temp": 5, "tint": 5, "saturation": 10, "neutral_r": 120, "neutral_g": 80, "neutral_b": 40, "_image": {"x": 0, "y": 0, "canvasWidth": 128, "canvasHeight": 128}}"#;
    for (name, params) in [("costly", costly), ("preset standard", r#"{"preset": "standard"}"#)] {
        // A one-pixel band is the fixed cost of a call: the parameters and the tables.
        let mut one = rgba_band(&scene[..1]);
        assert_eq!(h.filter(&mut one, 1, 1, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0, "{name}");
        let fixed = h.fuel_used();
        let mut px = rgba_band(&scene);
        assert_eq!(h.filter(&mut px, 128, 128, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0, "{name}");
        let per_sample = h.fuel_used() / samples;
        eprintln!("{name}: {fixed} instructions fixed per call, {} per sample beyond that", (h.fuel_used() - fixed) / (samples - 4));
        // The tables take a fixed cost per band (here spread over 64 k samples); a sample itself
        // is a few dozen instructions. Keep well inside the host's budget either way.
        assert!(per_sample < FUEL_PER_SAMPLE / 2, "{name}: {per_sample} instructions per sample (budget {FUEL_PER_SAMPLE})");
        eprintln!("{name}: {per_sample} instructions per sample");
    }
}
