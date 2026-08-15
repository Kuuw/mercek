//! Async color-name lookup using a KD-tree

use kiddo::ImmutableKdTree;
use kiddo::SquaredEuclidean;
use serde::Deserialize;
use std::sync::OnceLock;
use std::thread;

// Color Database
static COLOR_DB_JSON: &[u8] =
    include_bytes!("../resources/color-names/colornames.short.json");

// Global Singleton
/// Stores the ready-to-query tree + name list.
struct ColorIndex {
    tree: ImmutableKdTree<f64, 3>,
    names: Vec<String>,
}

/// Once the background thread finishes, the result lands here.
static INDEX: OnceLock<ColorIndex> = OnceLock::new();

#[derive(Deserialize)]
struct Entry {
    name: String,
    hex: String,
}

/// sRGB to Linear
fn srgb_to_linear(c: f64) -> f64 {
    let c = c / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear sRGB to CIE XYZ (D65 illuminant).
fn linear_rgb_to_xyz(r: f64, g: f64, b: f64) -> [f64; 3] {
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
    [x, y, z]
}

/// CIE XYZ to CIELAB (D65 reference white).
fn xyz_to_lab(x: f64, y: f64, z: f64) -> [f64; 3] {
    // D65 reference white
    const XN: f64 = 0.95047;
    const YN: f64 = 1.00000;
    const ZN: f64 = 1.08883;

    fn f(t: f64) -> f64 {
        const DELTA: f64 = 6.0 / 29.0;
        if t > DELTA * DELTA * DELTA {
            t.cbrt()
        } else {
            t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
        }
    }

    let fx = f(x / XN);
    let fy = f(y / YN);
    let fz = f(z / ZN);

    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    [l, a, b]
}

fn rgb_to_lab(r: u8, g: u8, b: u8) -> [f64; 3] {
    let rl = srgb_to_linear(r as f64);
    let gl = srgb_to_linear(g as f64);
    let bl = srgb_to_linear(b as f64);
    let [x, y, z] = linear_rgb_to_xyz(rl, gl, bl);
    xyz_to_lab(x, y, z)
}

/// Parse hex into separate rgb values.
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Initiate and build KD-tree, called once in the beginning.
pub fn init() {
    thread::spawn(|| {
        // Parse JSON
        let entries: Vec<Entry> =
            serde_json::from_slice(COLOR_DB_JSON).expect("Failed to parse color-names JSON");

        let mut points: Vec<[f64; 3]> = Vec::with_capacity(entries.len());
        let mut names: Vec<String> = Vec::with_capacity(entries.len());

        for entry in &entries {
            if let Some((r, g, b)) = parse_hex(&entry.hex) {
                points.push(rgb_to_lab(r, g, b));
                names.push(entry.name.clone());
            }
        }

        // Build the immutable KD-tree from the CIELAB points
        let tree: ImmutableKdTree<f64, 3> = (points.as_slice()).into();

        let count = names.len();
        let _ = INDEX.set(ColorIndex { tree, names });
        log::info!("Color-name KD-tree built with {} entries", count);
    });
}

/// Non-blocking colour name lookup.
pub fn lookup(r: u8, g: u8, b: u8) -> Option<&'static str> {
    let index = INDEX.get()?;
    let lab = rgb_to_lab(r, g, b);
    let result = index.tree.nearest_one::<SquaredEuclidean>(&lab);
    Some(&index.names[result.item as usize])
}
