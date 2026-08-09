//! The application's window icon, drawn in code rather than shipped as a file.
//!
//! `eframe` wants raw RGBA, so a PNG would mean pulling in an image decoder
//! for one 256×256 image. Generating it keeps the dependency list where it
//! is and the repository free of binary blobs — the same reasoning behind
//! every component symbol being hand-written vectors in `symbol.rs`, and
//! behind storing project files uncompressed.
//!
//! The shape is an AND gate: the flat back and round nose are the most
//! recognisable mark in digital logic, and they survive being scaled down to
//! 16 px where any detail would turn to mush. The leads disappear at that
//! size, which is fine — they are there for the large one.

use eframe::egui;

/// Rendered at this size; the window manager scales it down as needed.
const SIZE: usize = 256;
/// Samples per pixel per axis. Coverage is counted rather than computed
/// analytically: 16 samples is far cheaper to write correctly than an exact
/// area for a shape made of a rectangle, a disc and some bars, and the icon
/// is built once at startup.
const SAMPLES: usize = 4;

/// Behind everything — the app's accent blue, dark enough for a light glyph
/// to read on it.
const BACKGROUND: [u8; 3] = [42, 74, 133];
/// The gate itself.
const GLYPH: [u8; 3] = [240, 244, 250];

/// Every measurement as a fraction of the icon's side, so the same design
/// serves the rasterised window icon and the vector one drawn in the About
/// box. Two drawings of one shape is how they would drift.
mod shape {
    /// How far the rounded square sits in from the edge, and how round.
    pub const INSET: f32 = 0.06;
    pub const CORNER: f32 = 0.22;
    /// The gate's body.
    pub const TOP: f32 = 0.28;
    pub const BOTTOM: f32 = 0.72;
    pub const BACK: f32 = 0.30;
    /// Where the flat part ends and the round nose begins.
    pub const NOSE: f32 = 0.56;
    /// Half-thickness of a lead.
    pub const LEAD: f32 = 0.030;
    pub const LEAD_START: f32 = 0.16;
    pub const LEAD_END: f32 = 0.84;
}

pub fn app_icon() -> egui::IconData {
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (mut background, mut glyph) = (0.0f32, 0.0f32);
            for sub_y in 0..SAMPLES {
                for sub_x in 0..SAMPLES {
                    let step = 1.0 / SAMPLES as f32;
                    let px = x as f32 + (sub_x as f32 + 0.5) * step;
                    let py = y as f32 + (sub_y as f32 + 0.5) * step;
                    if in_background(px, py) {
                        background += 1.0;
                    }
                    if in_glyph(px, py) {
                        glyph += 1.0;
                    }
                }
            }
            let total = (SAMPLES * SAMPLES) as f32;
            let (background, glyph) = (background / total, glyph / total);

            // The glyph only ever sits on the background, so compositing is
            // a plain mix; the alpha is the background's, which is what
            // rounds the icon's corners.
            let offset = (y * SIZE + x) * 4;
            for channel in 0..3 {
                let base = BACKGROUND[channel] as f32;
                let over = GLYPH[channel] as f32;
                rgba[offset + channel] = (base * (1.0 - glyph) + over * glyph) as u8;
            }
            rgba[offset + 3] = (background * 255.0) as u8;
        }
    }

    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

/// The rounded square every platform expects an icon to be.
fn in_background(x: f32, y: f32) -> bool {
    let s = SIZE as f32;
    let inset = s * shape::INSET;
    in_rounded_rect(
        x,
        y,
        (inset, inset, s - inset, s - inset),
        s * shape::CORNER,
    )
}

/// An AND gate: a rectangle whose right end is a half-disc, with two leads
/// entering and one leaving.
fn in_glyph(x: f32, y: f32) -> bool {
    let s = SIZE as f32;
    let (top, bottom) = (s * shape::TOP, s * shape::BOTTOM);
    let back = s * shape::BACK;
    let nose = s * shape::NOSE;
    let radius = (bottom - top) / 2.0;
    let middle = (top + bottom) / 2.0;

    let body = (x >= back && x <= nose && y >= top && y <= bottom)
        || (x > nose && (x - nose).hypot(y - middle) <= radius);

    let lead = s * shape::LEAD;
    let inputs = (x >= s * shape::LEAD_START && x <= back)
        && ((y - (top + radius * 0.5)).abs() <= lead
            || (y - (bottom - radius * 0.5)).abs() <= lead);
    let output = x >= nose + radius && x <= s * shape::LEAD_END && (y - middle).abs() <= lead;

    body || inputs || output
}

fn in_rounded_rect(x: f32, y: f32, rect: (f32, f32, f32, f32), radius: f32) -> bool {
    let (left, top, right, bottom) = rect;
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    // Only the corner regions need the distance test; everything else is
    // inside by the bounds check above.
    let corner_x = if x < left + radius {
        left + radius
    } else if x > right - radius {
        right - radius
    } else {
        return true;
    };
    let corner_y = if y < top + radius {
        top + radius
    } else if y > bottom - radius {
        bottom - radius
    } else {
        return true;
    };
    (x - corner_x).hypot(y - corner_y) <= radius
}

/// Paints the same mark with vectors, for showing it inside the app.
///
/// Not the rasterised icon scaled down: at the size the About box wants it,
/// a 256 px bitmap would be resampled and soft, and this is line work that
/// costs nothing to draw properly.
pub fn paint(painter: &egui::Painter, rect: egui::Rect) {
    let s = rect.width().min(rect.height());
    let at = |fx: f32, fy: f32| rect.min + egui::vec2(fx * s, fy * s);
    let background = egui::Color32::from_rgb(BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]);
    let glyph = egui::Color32::from_rgb(GLYPH[0], GLYPH[1], GLYPH[2]);

    painter.rect_filled(
        egui::Rect::from_min_max(
            at(shape::INSET, shape::INSET),
            at(1.0 - shape::INSET, 1.0 - shape::INSET),
        ),
        s * shape::CORNER,
        background,
    );

    // The body: flat back, then the nose swept as an arc. Convex, so it can
    // be filled as one polygon.
    let radius = (shape::BOTTOM - shape::TOP) / 2.0;
    let middle = (shape::TOP + shape::BOTTOM) / 2.0;
    let mut body = vec![at(shape::BACK, shape::TOP), at(shape::NOSE, shape::TOP)];
    let steps = 24;
    for step in 0..=steps {
        let angle =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / steps as f32;
        body.push(at(
            shape::NOSE + radius * angle.cos(),
            middle + radius * angle.sin(),
        ));
    }
    body.push(at(shape::NOSE, shape::BOTTOM));
    body.push(at(shape::BACK, shape::BOTTOM));
    painter.add(egui::Shape::convex_polygon(body, glyph, egui::Stroke::NONE));

    let bar = |from: f32, to: f32, y: f32| {
        painter.rect_filled(
            egui::Rect::from_min_max(at(from, y - shape::LEAD), at(to, y + shape::LEAD)),
            0.0,
            glyph,
        );
    };
    bar(shape::LEAD_START, shape::BACK, shape::TOP + radius * 0.5);
    bar(shape::LEAD_START, shape::BACK, shape::BOTTOM - radius * 0.5);
    bar(shape::NOSE + radius, shape::LEAD_END, middle);
}

/// Encoding the icon as a file, for the places a window icon can't reach —
/// a desktop entry, a README, a store listing.
///
/// A module of its own because only the `write-icon` tool calls it, and that
/// tool includes this file by path: without the blanket allow, building the
/// application itself would warn about every function here being unused.
pub mod png {
    #![allow(dead_code)]

    use super::app_icon;

    /// Encodes the icon as a PNG.
    ///
    /// Written by hand rather than with an encoder crate, which would be a
    /// dependency for one file. PNG allows **stored** deflate blocks, so there
    /// is no compression to implement — and the result costs almost nothing in
    /// the repository anyway, since git compresses its own blobs and flat
    /// colours pack away to nothing. The same bargain the `.slgx` container
    /// makes.
    pub fn encode() -> Vec<u8> {
        let icon = app_icon();
        let (width, height) = (icon.width, icon.height);

        // Each scanline is prefixed with its filter type; 0 means "none", which
        // is what makes the raw bytes writable as they are.
        let mut raw = Vec::with_capacity((height * (1 + width * 4)) as usize);
        for row in 0..height as usize {
            raw.push(0);
            let start = row * width as usize * 4;
            raw.extend_from_slice(&icon.rgba[start..start + width as usize * 4]);
        }

        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        // 8 bits per channel, colour type 6 (RGBA), no interlacing.
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &header);
        chunk(&mut out, b"IDAT", &zlib_stored(&raw));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }

    /// A zlib stream carrying `data` in uncompressed blocks.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        // 0x78 0x01: deflate, 32K window, no preset dictionary — the header a
        // decoder expects even when nothing is actually compressed.
        let mut out = vec![0x78, 0x01];
        let mut rest = data;
        loop {
            let take = rest.len().min(u16::MAX as usize);
            let (block, remainder) = rest.split_at(take);
            let last = remainder.is_empty();
            out.push(u8::from(last));
            out.extend_from_slice(&(take as u16).to_le_bytes());
            out.extend_from_slice(&(!(take as u16)).to_le_bytes());
            out.extend_from_slice(block);
            if last {
                break;
            }
            rest = remainder;
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn adler32(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_has_the_size_it_claims() {
        let icon = app_icon();
        assert_eq!(icon.width, SIZE as u32);
        assert_eq!(icon.height, SIZE as u32);
        assert_eq!(icon.rgba.len(), SIZE * SIZE * 4);
        // Platforms want a multiple of four in each direction.
        assert_eq!(icon.width % 4, 0);
    }

    #[test]
    fn the_corners_are_transparent_and_the_middle_is_not() {
        let icon = app_icon();
        let alpha = |x: usize, y: usize| icon.rgba[(y * SIZE + x) * 4 + 3];

        // A square icon would need no rounding; if the corners ever stopped
        // being cut, this is what would notice.
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(SIZE - 1, SIZE - 1), 0);
        assert_eq!(alpha(SIZE / 2, SIZE / 2), 255);
    }

    #[test]
    fn the_gate_reads_against_its_background() {
        let icon = app_icon();
        let pixel = |x: usize, y: usize| {
            let offset = (y * SIZE + x) * 4;
            [
                icon.rgba[offset],
                icon.rgba[offset + 1],
                icon.rgba[offset + 2],
            ]
        };

        // Inside the gate's body, and in the gap above it: the icon is only
        // legible if those two are different colours.
        assert_eq!(pixel(SIZE / 2, SIZE / 2), GLYPH);
        assert_eq!(pixel(SIZE / 2, SIZE / 5), BACKGROUND);
    }
}
