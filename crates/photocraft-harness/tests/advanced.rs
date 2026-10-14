//! Film Lab: Convert Negative (Advanced): left at its defaults it converts exactly as Convert
//! Negative does; a film base picked from the border wins over the preset's and the measured one;
//! every option of its menus does what it says.

mod common;

use common::{BASE, base_params, close, host, measurement_json, negative, one, param_order, wasm};
use film_lab_core::{PRESETS, color};
use film_lab_photocraft_harness::{Host, MODE_GRAY, MODE_RGB, Plugin, embed_measurement, format_code};
use serde_json::{Map, Value};

fn manifest() -> Value {
    serde_json::from_str(&host(Plugin::Advanced).manifest().unwrap()).unwrap()
}

/// What the host passes when the user changes nothing but `extra`: every parameter's default.
fn defaults(extra: &[(&str, Value)]) -> String {
    let m = manifest();
    let mut params: Map<String, Value> = m["params"].as_object().unwrap().iter().map(|(k, spec)| (k.clone(), spec["default"].clone())).collect();
    for (k, v) in extra {
        params.insert((*k).to_string(), v.clone());
    }
    Value::Object(params).to_string()
}

fn options(key: &str) -> Vec<String> {
    manifest()["params"][key]["options"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

/// The border's colour as the Info panel shows it: whole numbers, as the host passes an `int`.
fn picked(f: [f32; 3]) -> Vec<(&'static str, Value)> {
    let v = |ch: usize| Value::from((color::srgb_encode(BASE[ch] * f[ch]) * 255.0).round() as i64);
    vec![("filmBaseRed", v(0)), ("filmBaseGreen", v(1)), ("filmBaseBlue", v(2))]
}

#[test]
fn the_manifest_puts_the_film_base_after_the_preset() {
    let text = host(Plugin::Advanced).manifest().unwrap();
    let m: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(m["name"], "Film Lab: Convert Negative (Advanced)");
    assert_eq!(m["overlap"], 1, "sharpening needs a pixel of context around each band");
    let order = param_order(&text);
    assert_eq!(order[..4], ["preset", "filmBaseRed", "filmBaseGreen", "filmBaseBlue"], "the picked base comes right after the film");
    assert_eq!(order.len(), 25, "{order:?}");
    for key in ["filmBaseRed", "filmBaseGreen", "filmBaseBlue", "grayPointRed", "grayPointGreen", "grayPointBlue"] {
        let spec = &m["params"][key];
        assert_eq!(spec["type"], "int", "{key}: a field to type the Info panel's value into, not a slider");
        assert_eq!((spec["min"].as_i64(), spec["max"].as_i64(), spec["default"].as_i64()), (Some(0), Some(255), Some(0)), "{key}: 0 = unset");
    }
    assert_eq!(
        (m["params"]["toneCurve"]["default"].as_str(), m["params"]["colorLook"]["default"].as_str()),
        (Some("From Preset"), Some("From Preset"))
    );
    assert_eq!(m["params"]["amount"]["max"], 100, "the sharpening amount is a percentage");
}

#[test]
fn at_its_defaults_it_converts_as_convert_negative_does() {
    let measured = |plugin| Host::load(&embed_measurement(&wasm(plugin), &measurement_json()).unwrap()).unwrap();
    let (mut convert, mut advanced) = (host(Plugin::Convert), host(Plugin::Advanced));
    let (mut convert_m, mut advanced_m) = (measured(Plugin::Convert), measured(Plugin::Advanced));
    for p in PRESETS {
        for s in [0.0, 0.1, 0.3, 0.7, 1.0] {
            let simple = format!(r#"{{"preset": "{}"}}"#, p.name);
            let full = defaults(&[("preset", Value::from(p.name))]);
            let (a, b) = (one(&mut convert, s, &simple), one(&mut advanced, s, &full));
            assert!(a.iter().zip(b).all(|(x, y)| close(*x, y, 1e-6)), "{} at {s}, the preset's base: {a:?} vs {b:?}", p.name);
            let (a, b) = (one(&mut convert_m, s, &simple), one(&mut advanced_m, s, &full));
            assert!(a.iter().zip(b).all(|(x, y)| close(*x, y, 1e-6)), "{} at {s}, measured: {a:?} vs {b:?}", p.name);
        }
    }
}

#[test]
fn a_picked_base_neutralises_the_mask_the_preset_guessed_wrong() {
    let mut h = host(Plugin::Advanced);
    // The preset's typical mask is not this film's: grey scenes come out tinted.
    let guessed = one(&mut h, 0.4, &defaults(&[]));
    assert!((guessed[0] - guessed[2]).abs() > 0.02, "{guessed:?}");
    // The border's colour, read off the Info panel, puts it right (to the Info panel's precision).
    let picked_params = defaults(&picked([1.0; 3]));
    for s in [0.1, 0.4, 0.8] {
        let out = one(&mut h, s, &picked_params);
        assert!(close(out[0], out[1], 0.01) && close(out[1], out[2], 0.01), "scene {s}: {out:?}");
    }
    // Exact values convert exactly as Convert Negative with that base in its JSON.
    let mut convert = host(Plugin::Convert);
    let want = one(&mut convert, 0.4, &format!(r#"{{"preset": "Standard", {}}}"#, base_params()));
    let exact = base_params().replace("base_r", "filmBaseRed").replace("base_g", "filmBaseGreen").replace("base_b", "filmBaseBlue");
    let got = one(&mut h, 0.4, &format!(r#"{{"preset": "Standard", {exact}}}"#));
    assert!(want.iter().zip(got).all(|(a, b)| close(*a, b, 1e-6)), "{want:?} vs {got:?}");
}

#[test]
fn a_picked_base_wins_over_the_measured_one() {
    let wasm = embed_measurement(&wasm(Plugin::Advanced), &measurement_json()).unwrap();
    let mut h = Host::load(&wasm).unwrap();
    let measured = one(&mut h, 0.1, &defaults(&[]));
    // Picking the base the companion measured changes next to nothing: its black is the base.
    let same = one(&mut h, 0.1, &defaults(&picked([1.0; 3])));
    assert!(measured.iter().zip(same).all(|(a, b)| close(*a, b, 0.01)), "{measured:?} vs {same:?}");
    // A different pick is used: a redder border puts every scan value further below the base on
    // red, so the positive turns redder.
    let redder = one(&mut h, 0.1, &defaults(&picked([1.2, 1.0, 1.0])));
    assert!(redder[0] > measured[0] + 0.01 && close(redder[1], measured[1], 0.01), "{redder:?} vs {measured:?}");
    assert!(redder.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{redder:?}");
}

#[test]
fn a_grey_document_takes_its_base_from_the_red_field() {
    let mut h = host(Plugin::Advanced);
    let base = 0.5f32;
    let scan = |s: f32| color::srgb_encode(film_lab_core::film::transmittance(1.0 + s * (libm::exp2f(film_lab_core::RANGE_STOPS) - 1.0), base));
    let mut gray = vec![scan(0.0), scan(0.5), scan(1.0)];
    let params = defaults(&[("filmBaseRed", Value::from(color::srgb_encode(base) * 255.0)), ("dynamicRange", Value::from(6))]);
    assert_eq!(h.filter(&mut gray, 3, 1, 1, format_code(8, false, MODE_GRAY), &params).unwrap(), 0);
    assert!(close(gray[0], 0.0, 0.004) && gray[1] > 0.4 && close(gray[2], 1.0, 0.004), "{gray:?}");
}

#[test]
fn every_menu_option_does_something_of_its_own() {
    let mut h = host(Plugin::Advanced);
    // Three tones of the scene, so two options that agree on one value still differ.
    let render = |h: &mut Host, extra: &[(&str, Value)]| -> Vec<i64> {
        [0.05, 0.3, 0.7].iter().flat_map(|s| one(h, *s, &defaults(extra))).map(|v| (v * 1e5).round() as i64).collect()
    };
    for key in ["preset", "toneCurve", "colorLook", "whiteBalance"] {
        let mut seen = Vec::new();
        for option in options(key) {
            if option == "From Preset" {
                continue;
            }
            let out = render(&mut h, &[(key, Value::from(option.as_str()))]);
            assert!(!seen.contains(&out), "{key}: {option} renders like an earlier option (missing from the plug-in's table?)");
            seen.push(out);
        }
    }
    // "From Preset" is the preset's own: the Standard preset's tone curve and colour look.
    let base = render(&mut h, &[]);
    assert_eq!(render(&mut h, &[("toneCurve", Value::from("Lab Standard")), ("colorLook", Value::from("Neutral"))]), base);
    // An unlisted option (from a host that skipped validation) is the preset's own too.
    assert_eq!(render(&mut h, &[("toneCurve", Value::from("lab_standard")), ("colorLook", Value::from("nope"))]), base);
    let warm = one(&mut h, 0.3, &defaults(&[("whiteBalance", Value::from("Warm"))]));
    let as_shot = one(&mut h, 0.3, &defaults(&[]));
    assert!(warm[0] - warm[2] > as_shot[0] - as_shot[2], "warm is warmer: {warm:?} vs {as_shot:?}");
}

#[test]
fn the_tone_sliders_change_the_output() {
    let mut h = host(Plugin::Advanced);
    for (key, value, s) in [
        ("highlights", 30, 0.6),
        ("shadows", 30, 0.05),
        ("whites", 30, 0.6),
        ("blacks", 30, 0.05),
        ("fade", 60, 0.05),
        ("glow", 60, 0.6),
        ("dynamicRange", 6, 0.6),
    ] {
        let (before, after) = (one(&mut h, s, &defaults(&[])), one(&mut h, s, &defaults(&[(key, Value::from(value))])));
        assert!(before.iter().zip(after).any(|(a, b)| (a - b).abs() > 2e-3), "{key} = {value}: {before:?} vs {after:?}");
        assert!(after.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{key}: {after:?}");
    }
    let brighter = one(&mut h, 0.6, &defaults(&[("highlights", Value::from(30))]));
    assert!(brighter[1] > one(&mut h, 0.6, &defaults(&[]))[1], "more highlights brighten the light tones");
}

#[test]
fn sharpening_touches_the_interior_and_leaves_flat_areas_alone() {
    let mut h = host(Plugin::Advanced);
    // 8 × 8: dark left half, bright right half.
    let (w, ht) = (8usize, 8usize);
    let scene: Vec<f32> = (0..w * ht).map(|i| if i % w < w / 2 { 0.2 } else { 0.7 }).collect();
    let band = |h: &mut Host, extra: &[(&str, Value)]| {
        let mut px: Vec<f32> = scene.iter().flat_map(|s| [negative(*s, 0), negative(*s, 1), negative(*s, 2), 1.0]).collect();
        assert_eq!(h.filter(&mut px, w as u32, ht as u32, 4, format_code(8, true, MODE_RGB), &defaults(extra)).unwrap(), 0);
        px
    };
    let plain = band(&mut h, &[]);
    let lab = band(&mut h, &[("sharpening", Value::from("Lab")), ("amount", Value::from(100))]);
    let sharp = band(&mut h, &[("sharpening", Value::from("Scanner")), ("amount", Value::from(100))]);
    assert_ne!(lab, plain, "Lab sharpens");
    assert_ne!(lab, sharp, "Lab and Scanner differ");
    let g = |px: &[f32], x: usize, y: usize| px[(y * w + x) * 4 + 1];
    // Across the edge, the dark side darkens and the bright side brightens.
    assert!(
        g(&sharp, 3, 4) < g(&plain, 3, 4) - 1e-3 && g(&sharp, 4, 4) > g(&plain, 4, 4) + 1e-3,
        "{} {} vs {} {}",
        g(&sharp, 3, 4),
        g(&sharp, 4, 4),
        g(&plain, 3, 4),
        g(&plain, 4, 4)
    );
    // Far from the edge, and on the band's outer pixels (the host's overlap margin), nothing moves.
    assert!(close(g(&sharp, 1, 4), g(&plain, 1, 4), 1e-6) && close(g(&sharp, 6, 4), g(&plain, 6, 4), 1e-6));
    assert!(close(g(&sharp, 3, 0), g(&plain, 3, 0), 1e-6), "edge rows keep their values");
    assert!(sharp.iter().all(|v| v.is_finite()));
    // Off, or an amount of 0, is no sharpening.
    assert_eq!(band(&mut h, &[("sharpening", Value::from("Off")), ("amount", Value::from(100))]), plain);
    assert_eq!(band(&mut h, &[("sharpening", Value::from("Lab")), ("amount", Value::from(0))]), plain);
}
