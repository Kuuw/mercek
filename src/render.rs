use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont};
use tiny_skia::{
    FillRule, Paint, PathBuilder, PixmapMut, Rect,
    Stroke, Transform,
};

use crate::color_name;

// Fonts
static FONT_SEMIBOLD_BYTES: &[u8] =
    include_bytes!("../resources/font/SNPro/SNPro-Semibold.otf");
static FONT_REGULAR_BYTES: &[u8] =
    include_bytes!("../resources/font/SNPro/SNPro-Regular.otf");

// Constants
/// Radius of the magnifier lens in pixels
const LENS_RADIUS: f32 = 80.0;
/// Width of the crosshair lines
const CROSSHAIR_WIDTH: f32 = 1.5;
/// Width of the lens border ring
const RING_WIDTH: f32 = 3.0;
/// Height of the hex label box (two-line: hex + color name)
const LABEL_HEIGHT: f32 = 46.0;
/// Font size (px) for the hex string (semibold)
const HEX_FONT_SIZE: f32 = 13.0;
/// Font size (px) for the color name (regular, muted)
const NAME_FONT_SIZE: f32 = 11.0;

fn text_width(text: &str, font: &FontRef<'_>, scale: PxScale) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0_f32;
    let mut prev: Option<GlyphId> = None;
    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);
        if let Some(p) = prev {
            width += scaled.kern(p, gid);
        }
        width += scaled.h_advance(gid);
        prev = Some(gid);
    }
    width
}

fn draw_text(
    pixmap: &mut PixmapMut,
    text: &str,
    x: f32,
    y: f32,
    font: &FontRef<'_>,
    scale: PxScale,
    color: [u8; 4],
) {
    use ab_glyph::point;

    let scaled = font.as_scaled(scale);
    let mut cursor_x = x;
    let mut prev: Option<GlyphId> = None;

    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;

    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);

        if let Some(p) = prev {
            cursor_x += scaled.kern(p, gid);
        }

        let glyph = gid.with_scale_and_position(scale, point(cursor_x, y));
        cursor_x += scaled.h_advance(gid);
        prev = Some(gid);

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let bx = bounds.min.x as i32;
            let by = bounds.min.y as i32;

            let data = pixmap.data_mut();

            outlined.draw(|gx, gy, cov| {
                let px = bx + gx as i32;
                let py = by + gy as i32;
                if px < 0 || py < 0 || px >= pw || py >= ph {
                    return;
                }

                let idx = ((py * pw + px) * 4) as usize;
                if idx + 3 >= data.len() {
                    return;
                }

                let src_a = (cov * color[3] as f32).round() as u32;
                if src_a == 0 {
                    return;
                }
                let inv_a = 255 - src_a;

                data[idx]     = ((data[idx]     as u32 * inv_a + color[0] as u32 * src_a) / 255) as u8;
                data[idx + 1] = ((data[idx + 1] as u32 * inv_a + color[1] as u32 * src_a) / 255) as u8;
                data[idx + 2] = ((data[idx + 2] as u32 * inv_a + color[2] as u32 * src_a) / 255) as u8;
                data[idx + 3] = (data[idx + 3] as u32 + (src_a * (255 - data[idx + 3] as u32) / 255)) as u8;
            });
        }
    }
}

/// Renders the overlay frame directly into the provided ARGB8888 canvas buffer.
pub fn render_frame(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    screenshot: &[u8],
    shot_width: u32,
    shot_height: u32,
    cursor_x: f64,
    cursor_y: f64,
    lens_x: f64,
    lens_y: f64,
    zoom: f32,
    locked: bool,
) -> [u8; 4] {
    let w = width;
    let h = height;

    let scale_x = shot_width as f32 / w as f32;
    let scale_y = shot_height as f32 / h as f32;

    let cx = (cursor_x as f32).clamp(0.0, w.saturating_sub(1) as f32);
    let cy = (cursor_y as f32).clamp(0.0, h.saturating_sub(1) as f32);

    let lx = (lens_x as f32).clamp(0.0, w.saturating_sub(1) as f32);
    let ly = (lens_y as f32).clamp(0.0, h.saturating_sub(1) as f32);

    // If locked, sampling happens at the magnified pixel under the mouse cursor.
    // If not locked, sampling happens at the exact center under the lens.
    let (src_cx, src_cy) = if locked {
        let dx = cx - lx;
        let dy = cy - ly;
        (
            ((lx + dx / zoom) * scale_x).clamp(0.0, shot_width.saturating_sub(1) as f32),
            ((ly + dy / zoom) * scale_y).clamp(0.0, shot_height.saturating_sub(1) as f32),
        )
    } else {
        (
            (lx * scale_x).clamp(0.0, shot_width.saturating_sub(1) as f32),
            (ly * scale_y).clamp(0.0, shot_height.saturating_sub(1) as f32),
        )
    };

    let pixel_color = sample_pixel(screenshot, shot_width, shot_height, src_cx as u32, src_cy as u32);

    canvas.fill(0);

    let mut pixmap = PixmapMut::from_bytes(canvas, w, h)
        .expect("Failed to create PixmapMut from canvas");

    // 1. Draw magnifier lens at lens coordinates (lx, ly)
    draw_magnifier(&mut pixmap, screenshot, shot_width, shot_height, lx, ly, scale_x, scale_y, zoom);

    // 2. Draw crosshair at cursor position if locked, or at center (lx, ly) if unlocked
    if locked {
        draw_crosshair(&mut pixmap, cx, cy);
    } else {
        draw_crosshair(&mut pixmap, lx, ly);
    }

    // 3. Draw lens ring and hex label around the lens anchor
    draw_lens_ring(&mut pixmap, lx, ly, pixel_color);
    draw_hex_label(&mut pixmap, lx, ly, pixel_color, h);

    // 4. RGBA to BGRA swizzle around modified bounds
    let margin = (LENS_RADIUS + RING_WIDTH + 10.0) as i32;
    let label_extra = (LABEL_HEIGHT + 20.0) as i32;
    let x0 = ((lx as i32 - margin).max(0) as u32) as usize;
    let y0 = ((ly as i32 - margin - label_extra).max(0) as u32) as usize;
    let x1 = ((lx as i32 + margin).min(w as i32) as u32) as usize;
    let y1 = ((ly as i32 + margin + label_extra).min(h as i32) as u32) as usize;

    for row in y0..y1 {
        let row_start = row * w as usize * 4;
        let start = row_start + x0 * 4;
        let end = row_start + x1 * 4;
        if end <= canvas.len() {
            for pixel in canvas[start..end].chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
    }

    pixel_color
}

fn sample_pixel(screenshot: &[u8], shot_w: u32, shot_h: u32, x: u32, y: u32) -> [u8; 4] {
    let x = x.min(shot_w.saturating_sub(1));
    let y = y.min(shot_h.saturating_sub(1));
    let idx = ((y * shot_w + x) * 4) as usize;
    if idx + 3 < screenshot.len() {
        [screenshot[idx], screenshot[idx + 1], screenshot[idx + 2], screenshot[idx + 3]]
    } else {
        [0, 0, 0, 255]
    }
}

fn draw_magnifier(
    pixmap: &mut PixmapMut,
    screenshot: &[u8],
    shot_w: u32,
    shot_h: u32,
    cx: f32,
    cy: f32,
    scale_x: f32,
    scale_y: f32,
    zoom: f32,
) {
    let lens_r = LENS_RADIUS;

    let lens_left   = (cx - lens_r).floor() as i32;
    let lens_top    = (cy - lens_r).floor() as i32;
    let lens_right  = (cx + lens_r).ceil()  as i32;
    let lens_bottom = (cy + lens_r).ceil()  as i32;

    let pw = pixmap.width()  as i32;
    let ph = pixmap.height() as i32;

    let data = pixmap.data_mut();

    for py in lens_top..lens_bottom {
        for px in lens_left..lens_right {
            if px < 0 || py < 0 || px >= pw || py >= ph {
                continue;
            }

            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            if dx * dx + dy * dy > lens_r * lens_r {
                continue;
            }

            let src_x = (cx + dx / zoom) * scale_x;
            let src_y = (cy + dy / zoom) * scale_y;

            let color = sample_pixel(
                screenshot, shot_w, shot_h,
                src_x.round() as u32, src_y.round() as u32,
            );

            let idx = ((py * pw + px) * 4) as usize;
            if idx + 3 < data.len() {
                data[idx]     = color[0];
                data[idx + 1] = color[1];
                data[idx + 2] = color[2];
                data[idx + 3] = 255;
            }
        }
    }
}

fn draw_crosshair(pixmap: &mut PixmapMut, cx: f32, cy: f32) {
    let r = 12.0_f32; // Crosshair size

    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 200);
    paint.anti_alias = true;

    let stroke = Stroke { width: CROSSHAIR_WIDTH, ..Stroke::default() };
    let dark_stroke = Stroke { width: CROSSHAIR_WIDTH + 1.5, ..Stroke::default() };
    let mut dark_paint = Paint::default();
    dark_paint.set_color_rgba8(0, 0, 0, 100);
    dark_paint.anti_alias = true;

    for (_, p, s) in [
        (true,  &dark_paint, &dark_stroke),
        (false, &paint,      &stroke),
    ] {
        if let Some(path) = { let mut pb = PathBuilder::new(); pb.move_to(cx - r, cy); pb.line_to(cx + r, cy); pb.finish() } {
            pixmap.stroke_path(&path, p, s, Transform::identity(), None);
        }
        if let Some(path) = { let mut pb = PathBuilder::new(); pb.move_to(cx, cy - r); pb.line_to(cx, cy + r); pb.finish() } {
            pixmap.stroke_path(&path, p, s, Transform::identity(), None);
        }
    }

    let mut center_paint = Paint::default();
    center_paint.set_color_rgba8(255, 255, 255, 255);
    center_paint.anti_alias = true;

    if let Some(rect) = Rect::from_xywh(cx - 1.5, cy - 1.5, 3.0, 3.0) {
        pixmap.fill_rect(rect, &center_paint, Transform::identity(), None);
    }
}

fn draw_lens_ring(pixmap: &mut PixmapMut, cx: f32, cy: f32, pixel_color: [u8; 4]) {
    let r = LENS_RADIUS;

    let mut ring_paint = Paint::default();
    ring_paint.set_color_rgba8(pixel_color[0], pixel_color[1], pixel_color[2], 255);
    ring_paint.anti_alias = true;

    let ring_stroke = Stroke { width: RING_WIDTH + 2.0, ..Stroke::default() };
    if let Some(path) = create_circle_path(cx, cy, r) {
        pixmap.stroke_path(&path, &ring_paint, &ring_stroke, Transform::identity(), None);
    }

    let mut inner_paint = Paint::default();
    inner_paint.set_color_rgba8(255, 255, 255, 220);
    inner_paint.anti_alias = true;

    let inner_stroke = Stroke { width: RING_WIDTH, ..Stroke::default() };
    if let Some(path) = create_circle_path(cx, cy, r - 2.0) {
        pixmap.stroke_path(&path, &inner_paint, &inner_stroke, Transform::identity(), None);
    }
}

fn draw_hex_label(pixmap: &mut PixmapMut, cx: f32, cy: f32, pixel_color: [u8; 4], screen_h: u32) {
    let hex = format!("#{:02X}{:02X}{:02X}", pixel_color[0], pixel_color[1], pixel_color[2]);

    let name_text = match color_name::lookup(pixel_color[0], pixel_color[1], pixel_color[2]) {
        Some(name) => format!("≈ {name}"),
        None       => "...".to_string(),
    };

    let font_sb  = FontRef::try_from_slice(FONT_SEMIBOLD_BYTES).expect("SNPro-Semibold load failed");
    let font_reg = FontRef::try_from_slice(FONT_REGULAR_BYTES).expect("SNPro-Regular load failed");

    let hex_scale  = PxScale::from(HEX_FONT_SIZE);
    let name_scale = PxScale::from(NAME_FONT_SIZE);

    let hex_w  = text_width(&hex,       &font_sb,  hex_scale);
    let name_w = text_width(&name_text, &font_reg, name_scale);

    let swatch_size = 14.0_f32;
    let pad_h = 8.0_f32;
    let gap   = 3.0_f32;

    let content_w   = swatch_size + 6.0 + hex_w.max(name_w);
    let label_width = content_w + pad_h * 2.0;

    let label_y = if cy + LENS_RADIUS + LABEL_HEIGHT + 15.0 > screen_h as f32 {
        cy - LENS_RADIUS - LABEL_HEIGHT - 10.0
    } else {
        cy + LENS_RADIUS + 10.0
    };
    let label_x = cx - label_width / 2.0;

    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(20, 20, 22, 235);
    bg_paint.anti_alias = true;

    let corner_r = 7.0;
    if let Some(path) = create_rounded_rect(label_x, label_y, label_width, LABEL_HEIGHT, corner_r) {
        pixmap.fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None);
    }

    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(pixel_color[0], pixel_color[1], pixel_color[2], 180);
    border_paint.anti_alias = true;

    let border_stroke = Stroke { width: 1.0, ..Stroke::default() };
    if let Some(path) = create_rounded_rect(label_x, label_y, label_width, LABEL_HEIGHT, corner_r) {
        pixmap.stroke_path(&path, &border_paint, &border_stroke, Transform::identity(), None);
    }

    let swatch_x = label_x + pad_h;
    let swatch_y = label_y + (LABEL_HEIGHT - swatch_size) / 2.0;

    let mut swatch_paint = Paint::default();
    swatch_paint.set_color_rgba8(pixel_color[0], pixel_color[1], pixel_color[2], 255);
    if let Some(path) = create_rounded_rect(swatch_x, swatch_y, swatch_size, swatch_size, 3.0) {
        pixmap.fill_path(&path, &swatch_paint, FillRule::Winding, Transform::identity(), None);
    }

    let mut swatch_border = Paint::default();
    swatch_border.set_color_rgba8(255, 255, 255, 60);
    swatch_border.anti_alias = true;
    let swatch_stroke = Stroke { width: 0.8, ..Stroke::default() };
    if let Some(path) = create_rounded_rect(swatch_x, swatch_y, swatch_size, swatch_size, 3.0) {
        pixmap.stroke_path(&path, &swatch_border, &swatch_stroke, Transform::identity(), None);
    }

    let text_left = swatch_x + swatch_size + 6.0;

    let scaled_sb  = font_sb.as_scaled(hex_scale);
    let scaled_reg = font_reg.as_scaled(name_scale);

    let hex_line_h  = scaled_sb.ascent()  - scaled_sb.descent();
    let name_line_h = scaled_reg.ascent() - scaled_reg.descent();
    let block_h = hex_line_h + gap + name_line_h;

    let block_top = label_y + (LABEL_HEIGHT - block_h) / 2.0;
    let base1 = block_top + scaled_sb.ascent();
    let base2 = block_top + hex_line_h + gap + scaled_reg.ascent();

    draw_text(pixmap, &hex,       text_left, base1, &font_sb,  hex_scale,  [255, 255, 255, 255]);
    draw_text(pixmap, &name_text, text_left, base2, &font_reg, name_scale, [180, 180, 185, 220]);
}

fn create_circle_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let k = 0.5522847498;
    let kr = k * r;
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
    pb.cubic_to(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
    pb.cubic_to(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
    pb.cubic_to(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
    pb.close();
    pb.finish()
}

fn create_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w, y, x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x, y + h, x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y, x, y, x + r, y);
    pb.close();
    pb.finish()
}