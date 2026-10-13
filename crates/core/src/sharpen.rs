//! Output sharpening: an unsharp mask on the luminance of the converted band, in two flavours.
//!
//! - `Lab`: soft and edge-focused; differences below a threshold (grain) are left alone.
//! - `Scanner`: gritty; every difference is amplified, grain included.
//!
//! The caller provides a scratch buffer of one `f32` per pixel (hosts without an allocator
//! grow their memory for it). Pixels on the band's outer edge keep their values; a host that
//! pads bands with a one-pixel overlap gets seamless results.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sharpen {
    #[default]
    Off,
    Lab,
    Scanner,
}

impl Sharpen {
    pub const ALL: [(Sharpen, &'static str); 3] = [(Sharpen::Off, "off"), (Sharpen::Lab, "lab"), (Sharpen::Scanner, "scanner")];

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, name)| *name == s).map(|(t, _)| *t)
    }

    /// (gain at `amount` 100, threshold below which a difference is grain and left alone).
    fn recipe(self) -> (f32, f32) {
        match self {
            Self::Off => (0.0, 0.0),
            Self::Lab => (1.5, 0.02),
            Self::Scanner => (2.5, 0.0),
        }
    }
}

/// How a band's samples are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: usize,
    pub height: usize,
    /// Interleaved samples per pixel.
    pub channels: usize,
    /// How many of them are colour (the rest is alpha).
    pub colour: usize,
}

/// Sharpens `px` in place: a band laid out as `l` says, colour samples encoded 0..1, with
/// `amount` 0..100. `scratch` must hold at least `width × height` values. A pixel is skipped
/// when `opaque` says so (alpha 0). Returns false (and changes nothing) when the buffers are
/// too small.
pub fn unsharp(px: &mut [f32], scratch: &mut [f32], l: Layout, mode: Sharpen, amount: f32, opaque: impl Fn(&[f32]) -> bool) -> bool {
    let Layout { width, height, channels, colour } = l;
    let (gain, threshold) = mode.recipe();
    let amount = if amount.is_finite() { amount.clamp(0.0, 100.0) / 100.0 } else { 0.0 };
    let gain = gain * amount;
    if gain <= 0.0 || width < 3 || height < 3 {
        // Nothing to do (off, or no interior pixel to sharpen).
        return true;
    }
    let Some(n) = width.checked_mul(height) else { return false };
    if colour == 0 || channels < colour {
        return false;
    }
    if px.len() < n.saturating_mul(channels) || scratch.len() < n {
        return false;
    }
    // Luminance (the mean of the colour channels: encoded values, so a plain average).
    for (i, pix) in px.chunks_exact(channels).take(n).enumerate() {
        let sum: f32 = pix.iter().take(colour).sum();
        if let Some(s) = scratch.get_mut(i) {
            *s = sum / colour as f32;
        }
    }
    let at = |x: usize, y: usize| scratch.get(y * width + x).copied().unwrap_or(0.0);
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let i = y * width + x;
            let Some(pix) = px.get_mut(i * channels..(i + 1) * channels) else { continue };
            if !opaque(pix) {
                continue;
            }
            // 3 × 3 binomial blur.
            let blur = (at(x - 1, y - 1)
                + 2.0 * at(x, y - 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x - 1, y)
                + 4.0 * at(x, y)
                + 2.0 * at(x + 1, y)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1))
                / 16.0;
            let d = at(x, y) - blur;
            let d = if d.abs() <= threshold { 0.0 } else { d - threshold.copysign(d) };
            if d == 0.0 {
                continue;
            }
            let delta = gain * d;
            for v in pix.iter_mut().take(colour) {
                *v = (*v + delta).clamp(0.0, 1.0);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use super::*;

    fn layout(channels: usize, colour: usize) -> Layout {
        Layout { width: 5, height: 5, channels, colour }
    }

    /// A 5 × 5 grey band with a bright centre pixel.
    fn band(centre: f32) -> Vec<f32> {
        let mut px = vec![0.5; 25];
        px[12] = centre;
        px
    }

    #[test]
    fn modes_parse_and_off_changes_nothing() {
        for (mode, name) in Sharpen::ALL {
            assert_eq!(Sharpen::parse(name), Some(mode));
        }
        let mut px = band(0.6);
        let before = px.clone();
        let mut scratch = vec![0.0; 25];
        assert!(unsharp(&mut px, &mut scratch, layout(1, 1), Sharpen::Off, 100.0, |_| true));
        assert_eq!(px, before);
        assert!(unsharp(&mut px, &mut scratch, layout(1, 1), Sharpen::Lab, 0.0, |_| true));
        assert_eq!(px, before);
    }

    #[test]
    fn lab_masks_grain_and_scanner_does_not() {
        let mut scratch = vec![0.0; 25];
        // A 0.01 bump is grain for Lab, detail for Scanner.
        let mut px = band(0.51);
        assert!(unsharp(&mut px, &mut scratch, layout(1, 1), Sharpen::Lab, 100.0, |_| true));
        assert_eq!(px[12], 0.51, "{px:?}");
        let mut px = band(0.51);
        assert!(unsharp(&mut px, &mut scratch, layout(1, 1), Sharpen::Scanner, 100.0, |_| true));
        assert!(px[12] > 0.51, "{px:?}");
        // A real edge is sharpened by both, more by Scanner; neighbours darken; edges untouched.
        let mut lab = band(0.8);
        unsharp(&mut lab, &mut scratch, layout(1, 1), Sharpen::Lab, 100.0, |_| true);
        let mut scanner = band(0.8);
        unsharp(&mut scanner, &mut scratch, layout(1, 1), Sharpen::Scanner, 100.0, |_| true);
        assert!(scanner[12] >= lab[12] && lab[12] > 0.8, "{lab:?} {scanner:?}");
        assert!(lab[11] < 0.5 && lab[0] == 0.5, "{lab:?}");
    }

    #[test]
    fn rgba_sharpens_colour_only_and_respects_alpha_and_sizes() {
        let mut px: Vec<f32> = (0..25).flat_map(|i| if i == 12 { [0.9, 0.9, 0.9, 1.0] } else { [0.4, 0.4, 0.4, 1.0] }).collect();
        px[11 * 4 + 3] = 0.0;
        let mut scratch = vec![0.0; 25];
        assert!(unsharp(&mut px, &mut scratch, layout(4, 3), Sharpen::Scanner, 50.0, |p| p[3] > 0.0));
        assert!(px[12 * 4] > 0.9 && px[12 * 4 + 3] == 1.0, "{:?}", &px[48..52]);
        assert_eq!(&px[44..48], &[0.4, 0.4, 0.4, 0.0], "transparent pixel untouched");
        assert!(!unsharp(&mut px, &mut scratch[..10], layout(4, 3), Sharpen::Lab, 50.0, |_| true), "short scratch");
        assert!(!unsharp(&mut px[..20], &mut scratch, layout(4, 3), Sharpen::Lab, 50.0, |_| true), "short pixels");
        let mut tiny = vec![0.5; 4];
        assert!(
            unsharp(&mut tiny, &mut scratch, Layout { width: 2, height: 2, channels: 1, colour: 1 }, Sharpen::Lab, 50.0, |_| true),
            "too small to sharpen is not an error"
        );
    }
}
