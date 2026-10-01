//! Drawing primitives shared by the menu and the game view.

use crossterm::style::Color;

use crate::canvas::{Canvas, Rect};
use crate::font;
use crate::theme::{Rgb, Theme, glyph, palette};

/// A travelling highlight that walks around a frame's perimeter.
#[derive(Clone, Copy)]
pub struct Pulse {
    /// Distance along the perimeter, in cells. Wraps.
    pub at: f32,
    /// How many cells of tail the highlight drags behind it.
    pub tail: f32,
    pub color: Rgb,
}

/// Number of cells on the border of `r`.
pub fn perimeter_len(r: Rect) -> i32 {
    (2 * r.w + 2 * r.h - 4).max(1)
}

/// The `i`th border cell of `r`, walking clockwise from the top-left corner.
/// `i` wraps, so callers can pass an ever-increasing counter.
pub fn perimeter_cell(r: Rect, i: i32) -> (i32, i32, char) {
    let len = perimeter_len(r);
    let i = i.rem_euclid(len);

    let top = r.w;
    let right = r.h - 2;
    let bottom = r.w;

    if i < top {
        let ch = match i {
            0 => glyph::TL,
            _ if i == r.w - 1 => glyph::TR,
            _ => glyph::H,
        };
        (r.x + i, r.y, ch)
    } else if i < top + right {
        (r.x + r.w - 1, r.y + 1 + (i - top), glyph::V)
    } else if i < top + right + bottom {
        let k = i - top - right;
        let ch = match k {
            0 => glyph::BR,
            _ if k == r.w - 1 => glyph::BL,
            _ => glyph::H,
        };
        (r.x + r.w - 1 - k, r.y + r.h - 1, ch)
    } else {
        let k = i - top - right - bottom;
        (r.x, r.y + r.h - 2 - k, glyph::V)
    }
}

/// Draw a rounded frame, optionally titled, optionally lit by a travelling
/// pulse. `pulse.at` is in perimeter cells.
pub fn draw_frame(
    canvas: &mut Canvas,
    r: Rect,
    t: Theme,
    title: Option<(&str, Color)>,
    pulse: Option<Pulse>,
    bg: Color,
) {
    let len = perimeter_len(r);
    for i in 0..len {
        let (x, y, ch) = perimeter_cell(r, i);

        let mut color = palette::FRAME;
        if let Some(p) = pulse {
            // How far behind the head this cell sits, measured around the loop.
            let behind = (p.at - i as f32).rem_euclid(len as f32);
            if behind < p.tail {
                // Ease the falloff so the tail fades smoothly instead of
                // stepping down one colour per cell.
                let k = 1.0 - behind / p.tail;
                let k = k * k;
                color = palette::FRAME.lerp(p.color, k);
            }
        }
        canvas.set(x, y, ch, t.c(color), bg);
    }

    if let Some((text, color)) = title {
        let w = text.chars().count() as i32 + 2;
        let x = r.center_x() - w / 2;
        canvas.put(x, r.y, " ", bg, bg);
        canvas.put(x + 1, r.y, text, color, bg);
        canvas.put(x + 1 + text.chars().count() as i32, r.y, " ", bg, bg);
    }
}

/// Write `s` at (`x`, `y`). Returns the x just past the text.
pub fn text(canvas: &mut Canvas, x: i32, y: i32, s: &str, fg: Color, bg: Color) -> i32 {
    canvas.put(x, y, s, fg, bg)
}

/// Write `s` horizontally centred inside `r`.
pub fn text_centered(canvas: &mut Canvas, r: Rect, y: i32, s: &str, fg: Color, bg: Color) -> i32 {
    let x = r.x + (r.w - s.chars().count() as i32) / 2;
    canvas.put(x, y, s, fg, bg)
}

/// Like [`text_centered`] but drops characters until it fits `max`, appending
/// an ellipsis when something was lost. Keeps narrow terminals readable.
pub fn text_centered_fit(
    canvas: &mut Canvas,
    r: Rect,
    y: i32,
    s: &str,
    max: i32,
    fg: Color,
    bg: Color,
) {
    let shown = fit(s, max);
    text_centered(canvas, r, y, &shown, fg, bg);
}

/// Truncate `s` to `max` display cells, marking the cut with a single `…`.
pub fn fit(s: &str, max: i32) -> String {
    let max = max.max(0) as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = chars[..max - 1].iter().collect();
    out.push('…');
    out
}

/// Colours for a [`bar`].
#[derive(Clone, Copy)]
pub struct Meter {
    /// The filled portion.
    pub fill: Color,
    /// The unfilled remainder.
    pub track: Color,
    pub bg: Color,
}

/// A horizontal meter. `ratio` is clamped to `0.0..=1.0`.
pub fn bar(canvas: &mut Canvas, x: i32, y: i32, width: i32, ratio: f32, m: Meter) {
    let ratio = ratio.clamp(0.0, 1.0);
    let exact = ratio * width as f32;
    let full = exact.floor() as i32;
    let frac = exact - full as f32;
    // A part-filled cell at the leading edge makes the meter read as
    // continuous rather than stepping a whole cell at a time.
    let half = frac > 0.34 && full < width;

    for i in 0..width {
        let (ch, color) = if i < full {
            (glyph::BAR_FULL, m.fill)
        } else if i == full && half {
            (glyph::BAR_HALF, m.fill)
        } else {
            (glyph::BAR_EMPTY, m.track)
        };
        canvas.set(x + i, y, ch, color, m.bg);
    }
}

/// A dim separator rule spanning `r`.
pub fn rule(canvas: &mut Canvas, r: Rect, y: i32, inset: i32, t: Theme, bg: Color) {
    let x0 = r.x + inset;
    let x1 = r.right() - inset;
    for x in x0..x1 {
        canvas.set(x, y, glyph::H, t.c(palette::FRAME_DIM), bg);
    }
}

/// A two-stop colour ramp laid across a heading.
#[derive(Clone, Copy)]
pub struct Gradient {
    pub from: Rgb,
    pub to: Rgb,
    pub bg: Color,
}

/// Draw a heading in the block font, centred horizontally in `r`, with a
/// gradient swept across the letters.
pub fn heading(canvas: &mut Canvas, r: Rect, y: i32, s: &str, t: Theme, g: Gradient) {
    let x = r.x + (r.w - font::width(s)) / 2;
    font::draw(canvas, x, y, s, |p| (t.c(g.from.lerp(g.to, p)), g.bg));
}
