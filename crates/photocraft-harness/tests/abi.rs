//! Every Film Lab module honours PhotoCraft's ABI v1 (`photocraft/docs/plugins.md`): the exports,
//! a manifest the host accepts, one measurement block, hostile input, and the instruction budget.

mod common;

use common::{host, rgba_band, wasm};
use film_lab_core::PRESETS;
use film_lab_photocraft_harness::{FUEL_PER_SAMPLE, MEASUREMENT_MARKER, MODE_CMYK, MODE_GRAY, MODE_RGB, Plugin, format_code, plugin_crate_dir};
use serde_json::Value;

/// The plug-ins' version, from `crates/photocraft/Cargo.toml` (`[workspace.package]`).
fn workspace_version() -> String {
    let toml = std::fs::read_to_string(plugin_crate_dir().join("Cargo.toml")).unwrap();
    let line = toml.lines().find(|l| l.starts_with("version = ")).unwrap();
    line.trim_start_matches("version = ").trim_matches('"').to_string()
}

#[test]
fn every_module_exports_abi_v1_and_a_manifest_the_host_accepts() {
    let mut ids = Vec::new();
    for plugin in Plugin::ALL {
        let mut h = host(plugin);
        assert_eq!(h.abi_version().unwrap(), 1, "{plugin:?}");
        let text = h.manifest().unwrap();
        assert!(text.len() <= 64 * 1024, "{plugin:?}");
        let m: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(m["id"], plugin.id());
        assert_eq!(m["version"], workspace_version(), "{plugin:?}: the manifest reports the workspace's version");
        assert_eq!(m["kind"], "filter");
        let name = m["name"].as_str().unwrap();
        assert!(name.starts_with("Film Lab: ") && name.chars().count() <= 64, "{name}");
        assert!(!name.ends_with('…'), "{name}: the host adds the menu's …; the dialog title and History show the name as it is");
        let params = m["params"].as_object().unwrap();
        let presets: Vec<&str> = m["params"]["preset"]["options"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(presets, PRESETS.iter().map(|p| p.name).collect::<Vec<_>>(), "{plugin:?}: the dialog lists the core's presets");
        assert_eq!(m["params"]["preset"]["default"], "Standard");
        let reserved = ["x", "y", "width", "height", "overlap", "mode", "depth", "alpha", "channels", "canvasWidth", "canvasHeight", "id", "layer"];
        for key in params.keys() {
            assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !key.starts_with('_'), "{plugin:?}: bad name {key}");
            assert!(!reserved.contains(&key.as_str()), "{plugin:?}: {key} clashes with the host's keys");
        }
        let wasm = wasm(plugin);
        assert_eq!(wasm.windows(MEASUREMENT_MARKER.len()).filter(|w| *w == MEASUREMENT_MARKER).count(), 1, "{plugin:?}: one measurement block");
        assert!(h.alloc(1 << 20).unwrap() > 0);
        ids.push(m["id"].as_str().unwrap().to_string());
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), Plugin::ALL.len(), "every module has its own id, or one install would replace the other");
}

/// The text PhotoCraft shows for a parameter key or a choice option (`source_label` in
/// `photocraft/crates/ui-egui/src/filter_dialog.rs`): text with a space as it is, else the first
/// letter capitalised and camelCase split into words.
fn photocraft_label(key: &str) -> String {
    if key.contains(' ') {
        return key.to_string();
    }
    let mut s = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            s.push(' ');
            s.push(ch);
        } else {
            s.push(ch);
        }
    }
    s
}

#[test]
fn every_label_and_option_reads_as_words() {
    assert_eq!(photocraft_label("filmBaseRed"), "Film Base Red");
    assert_eq!(photocraft_label("Portra 400"), "Portra 400");
    for plugin in Plugin::ALL {
        let m: Value = serde_json::from_str(&host(plugin).manifest().unwrap()).unwrap();
        for (key, spec) in m["params"].as_object().unwrap() {
            let mut texts = vec![photocraft_label(key)];
            texts.extend(spec["options"].as_array().into_iter().flatten().map(|o| photocraft_label(o.as_str().unwrap())));
            for text in texts {
                let words_ok = text.split(' ').all(|w| w.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit()));
                assert!(!text.contains('_') && words_ok, "{plugin:?} {key}: the dialog would show `{text}`");
            }
        }
    }
}

#[test]
fn cmyk_and_alpha_only_bands_are_refused() {
    for plugin in Plugin::ALL {
        let mut h = host(plugin);
        let mut cmyk = vec![0.1; 5];
        assert_eq!(h.filter(&mut cmyk, 1, 1, 5, format_code(8, true, MODE_CMYK), "{}").unwrap(), 2, "{plugin:?}");
        let mut none = vec![1.0];
        assert_eq!(h.filter(&mut none, 1, 1, 1, format_code(8, true, MODE_GRAY), "{}").unwrap(), 1, "{plugin:?}");
    }
}

#[test]
fn garbage_parameters_fall_back_to_defaults() {
    let garbage = r#"{"preset": ["x"], "exposure": "no", "base_r": null, "temp": 1e99, "tone": 3, "sharpen": "max", "range": -1e30"#;
    for plugin in Plugin::ALL {
        let mut h = host(plugin);
        let mut px = rgba_band(&[0.3]);
        assert_eq!(h.filter(&mut px, 1, 1, 4, format_code(8, true, MODE_RGB), garbage).unwrap(), 0, "{plugin:?}");
        assert!(px.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{plugin:?}: {px:?}");
    }
}

#[test]
fn a_full_band_fits_the_hosts_instruction_budget() {
    // 128 × 128 RGBA: enough samples that the per-sample cost dominates the fixed budget.
    let scene: Vec<f32> = (0..128 * 128).map(|i| (i % 128) as f32 / 127.0).collect();
    let samples = scene.len() as u64 * 4;
    // The most expensive paths: a toned warm preset, every slider, a neutral sample; on the
    // advanced module also a picked base, the tone controls and sharpening.
    let costly = r#"{"preset": "Gold 200", "exposure": 0.3, "brightness": 20, "contrast": 10, "temp": 5, "tint": 5, "saturation": 10, "neutral_r": 120, "neutral_g": 80, "neutral_b": 40, "_image": {"x": 0, "y": 0, "canvasWidth": 128, "canvasHeight": 128}}"#;
    let advanced = r#"{"preset": "Gold 200", "filmBaseRed": 200, "filmBaseGreen": 130, "filmBaseBlue": 80, "exposure": 0.3, "brightness": 20, "contrast": 10, "toneCurve": "Cinematic Rich", "highlights": 10, "shadows": -10, "whites": 5, "blacks": 5, "fade": 20, "glow": 20, "dynamicRange": 4, "whiteBalance": "Warm", "temp": 5, "tint": 5, "grayPointRed": 120, "grayPointGreen": 80, "grayPointBlue": 40, "colorLook": "Print", "saturation": 10, "sharpening": "Scanner", "amount": 80}"#;
    let runs = [
        (Plugin::Convert, "costly", costly),
        (Plugin::Convert, "preset standard", r#"{"preset": "Standard"}"#),
        (Plugin::Advanced, "costly", costly),
        (Plugin::Advanced, "every control and sharpening", advanced),
    ];
    for (plugin, name, params) in runs {
        let mut h = host(plugin);
        // A one-pixel band is the fixed cost of a call: the parameters and the tables.
        let mut one = rgba_band(&scene[..1]);
        assert_eq!(h.filter(&mut one, 1, 1, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0, "{plugin:?} {name}");
        let fixed = h.fuel_used();
        let mut px = rgba_band(&scene);
        assert_eq!(h.filter(&mut px, 128, 128, 4, format_code(8, true, MODE_RGB), params).unwrap(), 0, "{plugin:?} {name}");
        let per_sample = h.fuel_used() / samples;
        eprintln!("{plugin:?} {name}: {fixed} instructions fixed per call, {} per sample beyond that", (h.fuel_used() - fixed) / (samples - 4));
        // The tables take a fixed cost per band (here spread over 64 k samples); a sample itself
        // is a few dozen instructions. Keep well inside the host's budget either way.
        assert!(per_sample < FUEL_PER_SAMPLE / 2, "{plugin:?} {name}: {per_sample} instructions per sample (budget {FUEL_PER_SAMPLE})");
    }
}
