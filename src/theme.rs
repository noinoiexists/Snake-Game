//! Colour handling and the glyph vocabulary.
//!
//! Everything is authored in 24-bit RGB and downgraded at render time to
//! whatever the terminal actually advertises, so the game stays coherent on a
//! 256-colour or even 16-colour terminal instead of turning into mush.

use crossterm::style::Color;

/// How much colour we are allowed to emit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl ColorMode {
    /// Sniff the environment. Every terminal worth using sets one of these.
    pub fn detect() -> Self {
        if let Ok(ct) = std::env::var("COLORTERM") {
            let ct = ct.to_ascii_lowercase();
            if ct.contains("truecolor") || ct.contains("24bit") {
                return Self::TrueColor;
            }
        }
        match std::env::var("TERM") {
            Ok(t) => {
                let t = t.to_ascii_lowercase();
                if t.contains("truecolor") || t.contains("24bit") {
                    Self::TrueColor
                } else if t.contains("256") {
                    Self::Ansi256
                } else if t.contains("kitty") || t.contains("alacritty") || t.contains("wezterm") {
                    Self::TrueColor
                } else {
                    Self::Ansi16
                }
            }
            Err(_) => Self::Ansi256,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TrueColor => "24-bit",
            Self::Ansi256 => "256 colour",
            Self::Ansi16 => "16 colour",
        }
    }
}

/// A 24-bit colour. Kept separate from `crossterm::style::Color` so we can do
/// arithmetic on it (blending, fading) before resolving it for the terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Linear blend; `t == 0.0` yields `self`, `t == 1.0` yields `other`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }

    /// Scale brightness, saturating at the ends.
    pub fn scale(self, f: f32) -> Self {
        let s = |v: u8| ((v as f32) * f).clamp(0.0, 255.0) as u8;
        Self::new(s(self.r), s(self.g), s(self.b))
    }

    pub fn to_color(self, mode: ColorMode) -> Color {
        match mode {
            ColorMode::TrueColor => Color::Rgb {
                r: self.r,
                g: self.g,
                b: self.b,
            },
            ColorMode::Ansi256 => Color::AnsiValue(self.to_ansi256()),
            ColorMode::Ansi16 => self.to_ansi16(),
        }
    }

    /// Nearest entry in the xterm-256 palette.
    fn to_ansi256(self) -> u8 {
        let (r, g, b) = (self.r as i32, self.g as i32, self.b as i32);
        // Near-grey colours look much better on the dedicated grey ramp than
        // on the 6x6x6 cube, which has no true greys.
        if (r - g).abs() < 12 && (g - b).abs() < 12 && (r - b).abs() < 12 {
            if r < 8 {
                return 16;
            }
            if r > 248 {
                return 231;
            }
            return (232 + (((r as f32 - 8.0) / 247.0) * 24.0).round() as i32) as u8;
        }
        let q = |v: i32| ((v as f32 / 255.0) * 5.0).round() as i32;
        (16 + 36 * q(r) + 6 * q(g) + q(b)) as u8
    }

    /// Nearest of the 16 stock ANSI colours, by squared distance.
    fn to_ansi16(self) -> Color {
        const TABLE: [(Rgb, Color); 16] = [
            (Rgb::new(0, 0, 0), Color::Black),
            (Rgb::new(128, 0, 0), Color::DarkRed),
            (Rgb::new(0, 128, 0), Color::DarkGreen),
            (Rgb::new(128, 128, 0), Color::DarkYellow),
            (Rgb::new(0, 0, 128), Color::DarkBlue),
            (Rgb::new(128, 0, 128), Color::DarkMagenta),
            (Rgb::new(0, 128, 128), Color::DarkCyan),
            (Rgb::new(192, 192, 192), Color::Grey),
            (Rgb::new(128, 128, 128), Color::DarkGrey),
            (Rgb::new(255, 0, 0), Color::Red),
            (Rgb::new(0, 255, 0), Color::Green),
            (Rgb::new(255, 255, 0), Color::Yellow),
            (Rgb::new(0, 0, 255), Color::Blue),
            (Rgb::new(255, 0, 255), Color::Magenta),
            (Rgb::new(0, 255, 255), Color::Cyan),
            (Rgb::new(255, 255, 255), Color::White),
        ];
        let mut best = TABLE[0].1;
        let mut best_d = i32::MAX;
        for (rgb, color) in TABLE {
            let dr = rgb.r as i32 - self.r as i32;
            let dg = rgb.g as i32 - self.g as i32;
            let db = rgb.b as i32 - self.b as i32;
            let d = dr * dr + dg * dg + db * db;
            if d < best_d {
                best_d = d;
                best = color;
            }
        }
        best
    }
}

/// The palette. Deep-navy background, mint snake, coral food.
pub mod palette {
    use super::Rgb;

    // Surfaces
    pub const BG: Rgb = Rgb::new(0x0A, 0x0D, 0x14);
    pub const PANEL: Rgb = Rgb::new(0x0F, 0x14, 0x1E);
    pub const PANEL_ALT: Rgb = Rgb::new(0x12, 0x18, 0x24);

    // Structure
    pub const FRAME: Rgb = Rgb::new(0x2B, 0x35, 0x4A);
    pub const FRAME_DIM: Rgb = Rgb::new(0x1A, 0x21, 0x2E);

    // Accents
    pub const ACCENT: Rgb = Rgb::new(0x5E, 0xEA, 0xC0);
    pub const VIOLET: Rgb = Rgb::new(0xA7, 0x8B, 0xFA);

    // Type
    pub const TEXT: Rgb = Rgb::new(0xE6, 0xED, 0xF3);
    pub const TEXT_DIM: Rgb = Rgb::new(0x8B, 0x96, 0xA8);
    pub const TEXT_FAINT: Rgb = Rgb::new(0x44, 0x4E, 0x60);

    // Snake
    pub const HEAD: Rgb = Rgb::new(0xB8, 0xFF, 0xE2);
    pub const BODY_NEAR: Rgb = Rgb::new(0x4A, 0xE3, 0xA8);
    pub const BODY_FAR: Rgb = Rgb::new(0x17, 0x6B, 0x62);

    // Food
    pub const FOOD: Rgb = Rgb::new(0xFF, 0x6B, 0x6B);
    pub const FOOD_GOLD: Rgb = Rgb::new(0xFF, 0xD1, 0x66);

    // Signals
    pub const DANGER: Rgb = Rgb::new(0xFF, 0x5C, 0x5C);
    pub const WARN: Rgb = Rgb::new(0xFF, 0xB4, 0x54);
    pub const OK: Rgb = Rgb::new(0x5E, 0xEA, 0xC0);
}

/// Resolves authored [`Rgb`] values into terminal colours.
#[derive(Clone, Copy)]
pub struct Theme {
    pub mode: ColorMode,
}

impl Theme {
    pub fn new(mode: ColorMode) -> Self {
        Self { mode }
    }

    /// The one call used everywhere: `t.c(palette::ACCENT)`.
    pub fn c(self, rgb: Rgb) -> Color {
        rgb.to_color(self.mode)
    }
}

/// Every non-ASCII character the game draws, in one place.
pub mod glyph {
    // Box drawing — rounded corners with a double-line body.
    pub const TL: char = '╭';
    pub const TR: char = '╮';
    pub const BL: char = '╰';
    pub const BR: char = '╯';
    pub const H: char = '─';
    pub const V: char = '│';

    // Snake
    pub const HEAD_UP: char = '▲';
    pub const HEAD_DOWN: char = '▼';
    pub const HEAD_LEFT: char = '◀';
    pub const HEAD_RIGHT: char = '▶';
    pub const BODY: char = '●';
    pub const TAIL: char = '·';

    // Food
    pub const FOOD: char = '◆';
    pub const FOOD_GOLD: char = '★';
    pub const FOOD_FADING: char = '◇';

    // Meters and chrome
    pub const BAR_FULL: char = '▰';
    pub const BAR_EMPTY: char = '▱';
    pub const BAR_HALF: char = '▪';
    pub const SELECT: char = '❯';
    pub const BULLET: char = '•';
    pub const STAR: char = '★';
    pub const DIAMOND: char = '◆';

    // Particles
    pub const SPARKS: [char; 5] = ['·', '∙', '✦', '✧', '*'];

    /// The head glyph for a direction.
    pub fn head(dir: crate::game::Dir) -> char {
        use crate::game::Dir;
        match dir {
            Dir::Up => HEAD_UP,
            Dir::Down => HEAD_DOWN,
            Dir::Left => HEAD_LEFT,
            Dir::Right => HEAD_RIGHT,
        }
    }
}
