//! Film Lab: Convert Negative converts negatives synthesised from the core's own film model back
//! into the scene, and its presets, sliders and measurement sources behave as documented. What
//! every module shares (the ABI, hostile input, the budget) is in `abi.rs`.

mod common;

use common::{BASE, base_params, close, host, measurement_json, negative, negative_linear, one, param_order, rgba_band, wasm};
use film_lab_core::{PRESETS, RANGE_STOPS, color, film};
use film_lab_photocraft_harness::{Host, MODE_GRAY, MODE_RGB, Plugin, embed_measurement, format_code};
use serde_json::Value;

fn convert() -> Host {
    host(Plugin::Convert)
}

/// The scene-linear reference run: the standard preset with a linear tone is not in the dialog,
/// so these pass the hidden `range` and a typed base (the module holds no measurement).
fn reference(extra: &str) -> String {
    format!(r#"{{"preset": "standard", {}, "range": 6{extra}}}"#, base_params())
}

#[test]
fn a_short_manifest_the_preset_first() {
    let text = convert().manifest().unwrap();
    let m: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(m["id"], Plugin::Convert.id());
    assert_eq!(m["name"], "Film Lab: Convert Negative");
    assert_eq!(m["overlap"], 0, "no sharpening, no margins");
    assert_eq!(
        param_order(&text),
        ["preset", "exposure", "brightness", "contrast", "temp", "tint", "saturation"],
        "seven controls, the preset first"
    );
}

#[test]
fn a_typed_base_recovers_the_scene_from_a_synthetic_negative() {
    let mut h = convert();
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
    let wasm = wasm(Plugin::Convert);
    // Black at the base, white at the scan value of a scene 5 stops up, no balance.
    let patched = embed_measurement(&wasm, &measurement_json()).unwrap();
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
    let mut h = convert();
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
    let mut h = convert();
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
    let mut h = convert();
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
fn grayscale_converts() {
    let mut h = convert();
    // A grey document: one channel, the base typed into base_r.
    let base = 0.5f32;
    let scan = |s: f32| color::srgb_encode(film::transmittance(1.0 + s * (libm::exp2f(RANGE_STOPS) - 1.0), base));
    let mut gray = vec![scan(0.0), scan(0.5), scan(1.0)];
    let params = format!(r#"{{"preset": "standard", "base_r": {}, "range": 6}}"#, color::srgb_encode(base) * 255.0);
    assert_eq!(h.filter(&mut gray, 3, 1, 1, format_code(8, false, MODE_GRAY), &params).unwrap(), 0);
    assert!(close(gray[0], 0.0, 0.004) && gray[1] > 0.5 && close(gray[2], 1.0, 0.004), "{gray:?}");
}
