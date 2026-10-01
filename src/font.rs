//! A 5x5 block font used for headings.
//!
//! Each glyph is five rows of five cells (`█` = on, ` ` = off). It is drawn
//! with the same cell buffer as everything else, so headings pick up the
//! palette, gradients and colour-mode downgrade for free.

use crossterm::style::Color;

use crate::canvas::Canvas;

/// Total advance per character, including the one-column gap.
pub const ADVANCE: i32 = 6;

/// The five rows of a glyph, or `None` if we have no art for it.
pub fn rows(c: char) -> Option<[&'static str; 5]> {
    let r = match c.to_ascii_uppercase() {
        'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
        'B' => ["████ ", "█   █", "████ ", "█   █", "████ "],
        'C' => [" ████", "█    ", "█    ", "█    ", " ████"],
        'D' => ["████ ", "█   █", "█   █", "█   █", "████ "],
        'E' => ["█████", "█    ", "████ ", "█    ", "█████"],
        'F' => ["█████", "█    ", "████ ", "█    ", "█    "],
        'G' => [" ████", "█    ", "█  ██", "█   █", " ███ "],
        'H' => ["█   █", "█   █", "█████", "█   █", "█   █"],
        'I' => ["█████", "  █  ", "  █  ", "  █  ", "█████"],
        'J' => ["    █", "    █", "    █", "█   █", " ███ "],
        'K' => ["█   █", "█  █ ", "███  ", "█  █ ", "█   █"],
        'L' => ["█    ", "█    ", "█    ", "█    ", "█████"],
        'M' => ["█   █", "██ ██", "█ █ █", "█   █", "█   █"],
        'N' => ["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
        'O' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
        'P' => ["████ ", "█   █", "████ ", "█    ", "█    "],
        'Q' => [" ███ ", "█   █", "█   █", "█  █ ", " ██ █"],
        'R' => ["████ ", "█   █", "████ ", "█  █ ", "█   █"],
        'S' => [" ████", "█    ", " ███ ", "    █", "████ "],
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
        'U' => ["█   █", "█   █", "█   █", "█   █", " ███ "],
        'V' => ["█   █", "█   █", "█   █", " █ █ ", "  █  "],
        'W' => ["█   █", "█   █", "█ █ █", "██ ██", "█   █"],
        'X' => ["█   █", " █ █ ", "  █  ", " █ █ ", "█   █"],
        'Y' => ["█   █", " █ █ ", "  █  ", "  █  ", "  █  "],
        'Z' => ["█████", "   █ ", "  █  ", " █   ", "█████"],
        '0' => [" ███ ", "█  ██", "█ █ █", "██  █", " ███ "],
        '1' => ["  █  ", " ██  ", "  █  ", "  █  ", "█████"],
        '2' => [" ███ ", "█   █", "   █ ", "  █  ", "█████"],
        '3' => ["████ ", "    █", " ███ ", "    █", "████ "],
        '4' => ["█  █ ", "█  █ ", "█████", "   █ ", "   █ "],
        '5' => ["█████", "█    ", "████ ", "    █", "████ "],
        '6' => [" ███ ", "█    ", "████ ", "█   █", " ███ "],
        '7' => ["█████", "    █", "   █ ", "  █  ", "  █  "],
        '8' => [" ███ ", "█   █", " ███ ", "█   █", " ███ "],
        '9' => [" ███ ", "█   █", " ████", "    █", " ███ "],
        '!' => ["  █  ", "  █  ", "  █  ", "     ", "  █  "],
        '-' => ["     ", "     ", "█████", "     ", "     "],
        '.' => ["     ", "     ", "     ", "     ", "  █  "],
        ':' => ["     ", "  █  ", "     ", "  █  ", "     "],
        '+' => ["     ", "  █  ", "█████", "  █  ", "     "],
        '/' => ["    █", "   █ ", "  █  ", " █   ", "█    "],
        ' ' => ["     ", "     ", "     ", "     ", "     "],
        _ => return None,
    };
    Some(r)
}

/// Width in cells that [`draw`] would occupy for `text`.
pub fn width(text: &str) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 { 0 } else { n * ADVANCE - 1 }
}

/// Draw `text` with its top-left corner at (`x`, `y`).
///
/// `tint` receives a normalised horizontal position across the whole string and
/// returns the foreground/background pair for that column, so callers can sweep
/// a gradient along the text. Off cells are left untouched, so whatever the
/// caller already painted underneath shows through.
pub fn draw<F>(canvas: &mut Canvas, x: i32, y: i32, text: &str, mut tint: F)
where
    F: FnMut(f32) -> (Color, Color),
{
    let total = text.chars().count().max(1) as f32;
    for (i, c) in text.chars().enumerate() {
        let Some(art) = rows(c) else { continue };
        let (fg, bg) = tint(i as f32 / total);
        for (row, line) in art.iter().enumerate() {
            for (col, px) in line.chars().enumerate() {
                if px == '█' {
                    canvas.set(
                        x + i as i32 * ADVANCE + col as i32,
                        y + row as i32,
                        '█',
                        fg,
                        bg,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crossterm::style::Color;

    /// Every character we claim to support, including the ones only reachable
    /// through headings like "GAME OVER".
    fn supported() -> Vec<char> {
        let mut v: Vec<char> = ('A'..='Z').collect();
        v.extend('0'..='9');
        v.extend(['!', '-', '.', ':', '+', '/', ' ']);
        v
    }

    #[test]
    fn every_glyph_is_exactly_five_rows_of_five() {
        for c in supported() {
            let art = rows(c).unwrap_or_else(|| panic!("no art for {c:?}"));
            for (i, row) in art.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    5,
                    "row {i} of {c:?} is not 5 cells: {row:?}"
                );
            }
        }
    }

    #[test]
    fn unknown_characters_are_dropped_not_fatal() {
        assert!(rows('~').is_none());
        assert!(rows('\u{1F600}').is_none());
        // Drawing something unsupported must still advance correctly.
        let mut c = Canvas::new(40, 8);
        draw(&mut c, 0, 0, "A~B", |_| (Color::Reset, Color::Reset));
    }

    #[test]
    fn width_accounts_for_the_inter_character_gap() {
        assert_eq!(width("A"), 5);
        assert_eq!(width("AB"), 11);
        assert_eq!(width(""), 0);
    }

    /// Not a correctness check: run with
    /// `cargo test font::tests::preview -- --ignored --nocapture` to actually
    /// look at the glyph art.
    #[test]
    #[ignore]
    fn preview() {
        for line in [
            "SNAKE",
            "GAME OVER",
            "0123456789",
            "ABCDEFGHIJKLM",
            "NOPQRSTUVWXYZ",
        ] {
            let mut c = Canvas::new(width(line).max(1) + 2, 6);
            draw(&mut c, 0, 0, line, |_| (Color::Reset, Color::Reset));
            for y in 0..5 {
                let row: String = (0..c.w).map(|x| c.get(x, y).ch).collect();
                println!("{}", row.trim_end());
            }
            println!();
        }
    }
}
