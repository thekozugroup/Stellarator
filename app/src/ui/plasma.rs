//! The pixel field (DESIGN.md, Identity), ported from frontend/src/components/shared/plasma.tsx:
//! a square of cells stepping through a few palette colors with ordered dithering. The same seed
//! gives the same field as the web UI (FNV-1a over UTF-16 units, the same ramps, angle, bend and
//! 4×4 Bayer thresholds, and JavaScript's rounding), so a tile identifies the endpoint or prompt
//! set it stands for on every platform. Decorative: always next to the name it stands for, never
//! behind text.

use nsh_core::prelude::*;

use crate::format::js_round;

/// Color ramps from the palette (dark to light), one per field.
const RAMPS: [[u32; 5]; 6] = [
    [0x141a40, 0x3a46b4, 0x5566dc, 0xa1b5f8, 0xfc885e], // plasma: navy, periwinkle, coral core
    [0x3a46b4, 0x5566dc, 0xa1b5f8, 0xf5c2ab, 0xfc885e], // dawn
    [0x0b1515, 0x1f4a3a, 0x4f9a6a, 0x9ed49c, 0xe5f2c4], // field: ink to green
    [0x141a40, 0x2c3684, 0x5566dc, 0xa1b5f8, 0xdde5fd], // ice
    [0x2a1712, 0x9a4424, 0xe8703f, 0xfc885e, 0xffd3bd], // ember
    [0x0b1515, 0x3a46b4, 0xa1b5f8, 0x9ed49c, 0xe5f2c4], // aurora: periwinkle into green
];

/// Navy to periwinkle only: white on every cell is at least 4.8:1, for tiles that carry the mark.
const DEEP: [u32; 5] = [0x141a40, 0x232c78, 0x3a46b4, 0x4a58cc, 0x5566dc];

/// 4×4 Bayer matrix, as thresholds in [-0.5, 0.5).
const BAYER: [f64; 16] = {
    let order = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    let mut out = [0.0; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = order[i] as f64 / 16.0 - 0.5;
        i += 1;
    }
    out
};

/// FNV-1a over the seed's UTF-16 code units, as the web UI hashes it (`charCodeAt`).
pub fn hash(seed: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for unit in seed.encode_utf16() {
        h ^= u32::from(unit);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The cells of a field, row by row: `cells * cells` colors as 0xRRGGBB.
pub fn plasma_cells(seed: &str, cells: usize, deep: bool) -> Vec<u32> {
    let h = hash(seed);
    let ramp: &[u32; 5] = if deep { &DEEP } else { &RAMPS[h as usize % RAMPS.len()] };
    let angle = f64::from((h >> 8) % 8) * (std::f64::consts::PI / 4.0);
    let bend = (f64::from((h >> 16) % 7) - 3.0) / 6.0;
    let (dx, dy) = (angle.cos(), angle.sin());
    let levels = (ramp.len() - 1) as f64;
    let mut out = Vec::with_capacity(cells * cells);
    for y in 0..cells {
        for x in 0..cells {
            let u = if cells > 1 { x as f64 / (cells - 1) as f64 - 0.5 } else { 0.0 };
            let v = if cells > 1 { y as f64 / (cells - 1) as f64 - 0.5 } else { 0.0 };
            let along = u * dx + v * dy;
            let across = -u * dy + v * dx;
            let t = (0.5 + along / 1.42 + bend * across * across).clamp(0.0, 1.0);
            let level = js_round(t * levels + BAYER[(y % 4) * 4 + (x % 4)] * 0.9);
            out.push(ramp[level.clamp(0.0, levels) as usize]);
        }
    }
    out
}

/// A square tile `size` points wide with `cells` cells along each side and `radius` corners
/// (5 at 20 px, 8 at 28 px, 10 at 56 px). `deep` keeps to navy and periwinkle, for a tile with the
/// white mark on it.
pub fn plasma(seed: &str, cells: usize, size: f32, radius: f32, deep: bool) -> impl IntoElement {
    let colors = plasma_cells(seed, cells, deep);
    let cell = size / cells as f32;
    let rows = colors.chunks(cells).map(|row| {
        view()
            .flex_row()
            .h(px(cell))
            .children(row.iter().map(|&c| view().w(px(cell)).h(px(cell)).bg(rgb(c))).collect::<Vec<_>>())
    });
    view()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .rounded(px(radius))
        .overflow_hidden()
        .accessibility_elements_hidden(true)
        .children(rows.collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colors as the web UI's `plasmaCells` writes them, for comparison.
    fn hex(cells: &[u32]) -> String {
        cells.iter().map(|c| format!("{c:06x}")).collect::<Vec<_>>().join(",")
    }

    // Values from frontend/src/components/shared/plasma.tsx's plasmaCells, run with Node.
    #[test]
    fn the_hash_is_fnv1a_over_utf16() {
        assert_eq!(hash("endpoint-1"), 2_711_254_326);
        assert_eq!(hash("runs-empty"), 2_359_527_111);
        assert_eq!(hash(""), 2_166_136_261);
    }

    #[test]
    fn the_same_seed_draws_the_web_uis_field() {
        assert_eq!(
            hex(&plasma_cells("endpoint-1", 6, false)),
            "fc885e,fc885e,a1b5f8,a1b5f8,3a46b4,3a46b4,fc885e,a1b5f8,a1b5f8,5566dc,5566dc,3a46b4,a1b5f8,a1b5f8,5566dc,5566dc,3a46b4,3a46b4,a1b5f8,5566dc,5566dc,3a46b4,5566dc,3a46b4,3a46b4,5566dc,3a46b4,3a46b4,141a40,141a40,3a46b4,3a46b4,3a46b4,3a46b4,3a46b4,141a40"
        );
        assert_eq!(
            hex(&plasma_cells("endpoint-2", 6, false)),
            "a1b5f8,a1b5f8,5566dc,5566dc,3a46b4,3a46b4,f5c2ab,a1b5f8,a1b5f8,5566dc,5566dc,3a46b4,f5c2ab,f5c2ab,a1b5f8,a1b5f8,5566dc,5566dc,fc885e,f5c2ab,f5c2ab,a1b5f8,a1b5f8,5566dc,f5c2ab,f5c2ab,a1b5f8,f5c2ab,a1b5f8,a1b5f8,fc885e,f5c2ab,fc885e,f5c2ab,f5c2ab,a1b5f8"
        );
        assert_eq!(
            hex(&plasma_cells("endpoint-empty", 8, false)),
            "2a1712,2a1712,9a4424,9a4424,9a4424,e8703f,e8703f,fc885e,9a4424,9a4424,9a4424,9a4424,e8703f,e8703f,fc885e,fc885e,2a1712,9a4424,9a4424,e8703f,e8703f,fc885e,fc885e,fc885e,9a4424,9a4424,e8703f,e8703f,fc885e,fc885e,fc885e,fc885e,2a1712,9a4424,9a4424,e8703f,e8703f,fc885e,fc885e,ffd3bd,9a4424,9a4424,e8703f,e8703f,e8703f,e8703f,fc885e,fc885e,2a1712,9a4424,9a4424,e8703f,e8703f,fc885e,e8703f,fc885e,2a1712,2a1712,9a4424,9a4424,e8703f,e8703f,fc885e,fc885e"
        );
        assert_eq!(
            hex(&plasma_cells("prompt-set-7", 4, true)),
            "232c78,232c78,141a40,141a40,3a46b4,3a46b4,3a46b4,141a40,4a58cc,4a58cc,232c78,232c78,5566dc,4a58cc,4a58cc,232c78"
        );
        assert_eq!(hex(&plasma_cells("", 3, false)), "fc885e,f5c2ab,a1b5f8,f5c2ab,a1b5f8,a1b5f8,a1b5f8,5566dc,3a46b4");
    }
}
