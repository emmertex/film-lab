//! What the plug-ins' tests share: negatives synthesised from the core's own film model, and a
//! one-pixel run.
#![allow(dead_code)]

use film_lab_core::{Preset, RANGE_STOPS, color, film};
use film_lab_photocraft_harness::{Host, MODE_RGB, Plugin, build_plugin, format_code};

/// A film base in linear light (orange mask).
pub const BASE: [f32; 3] = [0.6, 0.35, 0.15];

pub fn wasm(plugin: Plugin) -> Vec<u8> {
    std::fs::read(build_plugin(plugin).unwrap()).unwrap()
}

pub fn host(plugin: Plugin) -> Host {
    Host::load(&wasm(plugin)).unwrap()
}

/// The linear scan value of relative scene exposure `e` (≥ 1) on channel `ch`: a scan of the
/// standard preset's film, its channels spread as the levels from a base alone expect.
pub fn scan_linear(e: f32, ch: usize) -> f32 {
    film::transmittance_at(e, BASE[ch], Preset::default_preset().scan_gamma()[ch])
}

/// The linear scan value of scene brightness `s` (0..1 of the positive) on channel `ch`.
pub fn negative_linear(s: f32, ch: usize) -> f32 {
    scan_linear(1.0 + s * (libm::exp2f(RANGE_STOPS) - 1.0), ch)
}

/// The sRGB-encoded scan value of scene brightness `s` on channel `ch`.
pub fn negative(s: f32, ch: usize) -> f32 {
    color::srgb_encode(negative_linear(s, ch))
}

/// The `base_*` values for [`BASE`] as the Info panel would show them.
pub fn base_params() -> String {
    let v = |ch: usize| color::srgb_encode(BASE[ch]) * 255.0;
    format!(r#""base_r": {}, "base_g": {}, "base_b": {}"#, v(0), v(1), v(2))
}

/// A measurement block for [`BASE`]: black at the base, white at the scan value of a scene
/// 5 stops up, no balance.
pub fn measurement_json() -> String {
    let five = |ch: usize| color::srgb_encode(scan_linear(libm::exp2f(5.0), ch)) * 255.0;
    let base = |ch: usize| color::srgb_encode(BASE[ch]) * 255.0;
    format!(
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
    )
}

/// An RGBA band of `scene` values (grey, one per pixel), all opaque.
pub fn rgba_band(scene: &[f32]) -> Vec<f32> {
    scene.iter().flat_map(|s| [negative(*s, 0), negative(*s, 1), negative(*s, 2), 1.0]).collect()
}

pub fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// Runs one grey scene value through the plug-in.
pub fn one(h: &mut Host, s: f32, params: &str) -> [f32; 3] {
    let mut px = rgba_band(&[s]);
    assert_eq!(h.filter(&mut px, 1, 1, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0);
    [px[0], px[1], px[2]]
}

/// The manifest's parameter names in manifest order (a name may recur later as an option value:
/// first occurrence).
pub fn param_order(text: &str) -> Vec<String> {
    let m: serde_json::Value = serde_json::from_str(text).unwrap();
    let params = m["params"].as_object().unwrap();
    let mut order: Vec<String> = Vec::new();
    for k in text.split('"').filter(|k| params.contains_key(*k)) {
        if !order.iter().any(|o| o == k) {
            order.push(k.to_string());
        }
    }
    order
}
