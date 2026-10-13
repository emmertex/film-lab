//! `film-lab compare`: how close a conversion is to a reference positive. Both images are
//! resampled to a common grid, so slightly different crops still line up roughly, and the
//! report gives per-channel statistics, a coarse grid of mean colours, and the differences.

use image::DynamicImage;
use image::imageops::FilterType;
use serde_json::{Value, json};

/// Cells per side of the grid.
const GRID: u32 = 4;
/// The common size both images are resampled to.
const SIZE: u32 = 256;

/// Per-channel mean and 5th / 50th / 95th percentiles (0..255), and the grid of cell means.
fn stats(img: &DynamicImage) -> (Value, Vec<[f64; 3]>) {
    let small = img.resize_exact(SIZE, SIZE, FilterType::Triangle).to_rgb8();
    let mut channels: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut cells = vec![[0.0f64; 3]; (GRID * GRID) as usize];
    let mut counts = vec![0u32; (GRID * GRID) as usize];
    for (x, y, p) in small.enumerate_pixels() {
        let cell = ((y * GRID / SIZE) * GRID + x * GRID / SIZE) as usize;
        for ch in 0..3 {
            channels[ch].push(p[ch]);
            if let Some(c) = cells.get_mut(cell) {
                c[ch] += f64::from(p[ch]);
            }
        }
        if let Some(n) = counts.get_mut(cell) {
            *n += 1;
        }
    }
    for (c, n) in cells.iter_mut().zip(&counts) {
        for v in c.iter_mut() {
            *v /= f64::from((*n).max(1));
        }
    }
    let mut per_channel = Vec::new();
    for (name, values) in ["r", "g", "b"].iter().zip(channels.iter_mut()) {
        values.sort_unstable();
        let n = values.len().max(1);
        let q = |p: f64| f64::from(values.get(((n - 1) as f64 * p) as usize).copied().unwrap_or(0));
        let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
        per_channel.push(json!({"channel": name, "mean": round(mean), "p5": q(0.05), "p50": q(0.5), "p95": q(0.95)}));
    }
    (Value::Array(per_channel), cells)
}

fn round(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// The comparison report of `ours` against `reference`.
pub fn report(ours: &DynamicImage, reference: &DynamicImage) -> Value {
    let (ours_stats, ours_cells) = stats(ours);
    let (ref_stats, ref_cells) = stats(reference);
    let mut grid = Vec::new();
    let mut total = [0.0f64; 3];
    for (i, (a, b)) in ours_cells.iter().zip(&ref_cells).enumerate() {
        let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        for ch in 0..3 {
            total[ch] += d[ch].abs();
        }
        grid.push(json!({"cell": [i as u32 % GRID, i as u32 / GRID], "ours": a.map(round), "reference": b.map(round), "delta": d.map(round)}));
    }
    let n = ours_cells.len().max(1) as f64;
    json!({
        "ours": ours_stats,
        "reference": ref_stats,
        "meanAbsDelta": {"r": round(total[0] / n), "g": round(total[1] / n), "b": round(total[2] / n)},
        "grid": grid,
    })
}
