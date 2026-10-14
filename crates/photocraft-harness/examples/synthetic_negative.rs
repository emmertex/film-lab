//! Writes a synthetic colour negative (`negative.png`) and the scene it was made from
//! (`expected.png`) into a directory, to run the advanced plug-in on through PhotoCraft itself:
//!
//! ```sh
//! cargo run -p film-lab-photocraft-harness --example synthetic_negative -- /tmp/neg
//! photocraft-cli run /tmp/neg/negative.png \
//!   --cmd plugin.install --params '{"path": ".../film_lab_convert_advanced.wasm"}' \
//!   --cmd plugin.run --params '{"id": "io.github.smyrgeorge.film-lab.convert-advanced", "params": {"filmBaseRed": 205, "filmBaseGreen": 162, "filmBaseBlue": 110, "toneCurve": "Linear"}}' \
//!   --out /tmp/neg/positive.png
//! ```
//!
//! The negative has a film border of the base colour around the frame (sample it, or crop it
//! away before an `auto` analysis). It prints the base as the plug-in's `base_*` parameters.

use std::path::Path;

use film_lab_core::{RANGE_STOPS, color, film};

const BASE: [f32; 3] = [0.6, 0.35, 0.15];
const W: u32 = 640;
const H: u32 = 480;
const BORDER: u32 = 40;

/// The scene in linear light: a grey ramp, a colour-patch row, and a soft sky-to-ground gradient.
fn scene(x: u32, y: u32) -> [f32; 3] {
    let fx = (x - BORDER) as f32 / (W - 2 * BORDER - 1) as f32;
    let fy = (y - BORDER) as f32 / (H - 2 * BORDER - 1) as f32;
    if fy < 0.2 {
        let v = color::srgb_decode(fx);
        [v, v, v]
    } else if fy < 0.4 {
        let patches: [[f32; 3]; 8] = [
            [0.8, 0.1, 0.1],
            [0.1, 0.6, 0.1],
            [0.1, 0.1, 0.8],
            [0.8, 0.7, 0.1],
            [0.1, 0.6, 0.7],
            [0.7, 0.1, 0.7],
            [0.9, 0.9, 0.9],
            [0.02, 0.02, 0.02],
        ];
        patches[((fx * 8.0) as usize).min(7)]
    } else {
        let t = (fy - 0.4) / 0.6;
        let sky = [0.25, 0.45, 0.9];
        let ground = [0.35, 0.25, 0.1];
        let mut c = [0.0; 3];
        for ch in 0..3 {
            c[ch] = sky[ch] * (1.0 - t) + ground[ch] * t;
        }
        // A bright sun and a deep shadow so the full range is present.
        let (dx, dy) = (fx - 0.8, (fy - 0.5) / 0.6);
        if dx * dx + dy * dy < 0.004 {
            c = [1.0, 0.98, 0.9];
        }
        if (fx - 0.15).abs() < 0.05 && t > 0.6 {
            c = [0.0; 3];
        }
        c
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let white = libm::exp2f(RANGE_STOPS) - 1.0;
    let mut neg = Vec::with_capacity((W * H * 3) as usize);
    let mut pos = Vec::with_capacity((W * H * 3) as usize);
    for y in 0..H {
        for x in 0..W {
            let border = x < BORDER || y < BORDER || x >= W - BORDER || y >= H - BORDER;
            let s = if border { [0.0; 3] } else { scene(x, y) };
            for ch in 0..3 {
                let t = film::transmittance(1.0 + s[ch].clamp(0.0, 1.0) * white, BASE[ch]);
                neg.push((color::srgb_encode(t) * 255.0).round() as u8);
                pos.push((color::srgb_encode(s[ch]) * 255.0).round() as u8);
            }
        }
    }
    write(&dir.join("negative.png"), &neg);
    write(&dir.join("expected.png"), &pos);
    println!("wrote {}/negative.png and expected.png ({W}×{H})", dir.display());
    println!(
        "film base (the advanced dialog's Film Base Red / Green / Blue): {:.0} {:.0} {:.0}",
        color::srgb_encode(BASE[0]) * 255.0,
        color::srgb_encode(BASE[1]) * 255.0,
        color::srgb_encode(BASE[2]) * 255.0
    );
}

fn write(path: &Path, rgb: &[u8]) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), W, H);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(rgb).unwrap();
}
