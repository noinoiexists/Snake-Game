//! An off-screen cell buffer and a diffing presenter.
//!
//! The game redraws the whole frame into a [`Canvas`] every tick, then
//! [`Screen::present`] walks it against the previous frame and emits only the
//! cells that actually changed. On a mostly-static screen that is a handful of
//! escape sequences per frame, which is what keeps the animation smooth instead
//! of strobing.

use std::io::{self, BufWriter, Stdout, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};

/// A single character cell: a glyph plus its colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
}

impl Cell {
    /// A cell that can never equal a drawn cell, used to force a full repaint
    /// after a resize.
    const INVALID: Self = Self {
        ch: '\0',
        fg: Color::Reset,
        bg: Color::Reset,
    };
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }
}

/// A rectangle in cell coordinates. Signed so layout arithmetic can go
/// negative without wrapping; drawing clips against the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn center_x(&self) -> i32 {
        self.x + self.w / 2
    }
}

/// An off-screen grid of [`Cell`]s.
pub struct Canvas {
    pub w: i32,
    pub h: i32,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Self {
        let (w, h) = (w.max(0), h.max(0));
        Self {
            w,
            h,
            cells: vec![Cell::default(); (w * h) as usize],
        }
    }

    /// Resize and force every cell to repaint on the next present.
    pub fn invalidate(&mut self, w: i32, h: i32) {
        let (w, h) = (w.max(0), h.max(0));
        self.w = w;
        self.h = h;
        self.cells = vec![Cell::INVALID; (w * h) as usize];
    }

    /// Fill the whole canvas, used as the first call of every frame.
    pub fn clear(&mut self, ch: char, fg: Color, bg: Color) {
        let cell = Cell { ch, fg, bg };
        self.cells.fill(cell);
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            None
        } else {
            Some((y * self.w + x) as usize)
        }
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, ch: char, fg: Color, bg: Color) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = Cell { ch, fg, bg };
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Cell {
        self.index(x, y).map_or(Cell::INVALID, |i| self.cells[i])
    }

    /// Draw a string, clipped to the canvas. Returns the x just past the end.
    pub fn put(&mut self, x: i32, y: i32, s: &str, fg: Color, bg: Color) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            if cx >= self.w {
                break;
            }
            self.set(cx, y, ch, fg, bg);
            cx += 1;
        }
        cx
    }

    pub fn fill(&mut self, rect: Rect, ch: char, fg: Color, bg: Color) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.set(x, y, ch, fg, bg);
            }
        }
    }
}

/// Owns the terminal's alternate screen and turns [`Canvas`]es into output.
pub struct Screen {
    front: Canvas,
    back: Canvas,
    out: BufWriter<Stdout>,
    cur_fg: Color,
    cur_bg: Color,
    /// Set after a resize so the first present repaints everything.
    force_full: bool,
    /// Set when the terminal has stale cells outside the new canvas bounds
    /// that a repaint alone would not cover.
    needs_clear: bool,
}

impl Screen {
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            front: Canvas::new(w, h),
            back: Canvas::new(w, h),
            out: BufWriter::with_capacity(64 * 1024, io::stdout()),
            cur_fg: Color::Reset,
            cur_bg: Color::Reset,
            force_full: true,
            needs_clear: true,
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.back.w, self.back.h)
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == (self.back.w, self.back.h) {
            return;
        }
        self.back.invalidate(w, h);
        self.front.invalidate(w, h);
        self.force_full = true;
        // Shrinking leaves the old, larger picture on screen outside the new
        // bounds, and a diff against the new canvas would never touch it.
        self.needs_clear = true;
    }

    /// The canvas to draw this frame into.
    pub fn canvas(&mut self) -> &mut Canvas {
        &mut self.back
    }

    /// Diff against the previous frame and write the changes.
    pub fn present(&mut self) -> io::Result<()> {
        if self.needs_clear {
            queue!(self.out, Clear(ClearType::All))?;
            self.needs_clear = false;
            // Clearing resets the terminal's pen, so forget what we thought
            // it was holding.
            self.cur_fg = Color::Reset;
            self.cur_bg = Color::Reset;
        }

        let (w, h) = (self.back.w, self.back.h);
        // Never touch the very last cell of the last row: some terminals scroll
        // when it is written, which would smear the whole picture.
        let last_x = w - 1;
        let last_y = h - 1;

        let mut cursor: Option<(i32, i32)> = None;

        for y in 0..h {
            let mut x = 0;
            while x < w {
                if x == last_x && y == last_y {
                    break;
                }
                let cell = self.back.get(x, y);
                let prev = self.front.get(x, y);
                if !self.force_full && cell == prev {
                    x += 1;
                    continue;
                }

                // Coalesce a run of changed cells that share colours into one
                // write — consecutive glyphs of the same style cost nothing extra.
                let (fg, bg) = (cell.fg, cell.bg);
                let start = x;
                let mut run = String::new();
                while x < w && !(x == last_x && y == last_y) {
                    let c = self.back.get(x, y);
                    if !self.force_full && c == self.front.get(x, y) {
                        break;
                    }
                    if c.fg != fg || c.bg != bg {
                        break;
                    }
                    run.push(c.ch);
                    x += 1;
                }

                if cursor != Some((start, y)) {
                    queue!(self.out, MoveTo(start as u16, y as u16))?;
                }
                if self.cur_fg != fg {
                    queue!(self.out, SetForegroundColor(fg))?;
                    self.cur_fg = fg;
                }
                if self.cur_bg != bg {
                    queue!(self.out, SetBackgroundColor(bg))?;
                    self.cur_bg = bg;
                }
                queue!(self.out, Print(&run))?;
                cursor = Some((x, y));
            }
        }

        if self.cur_fg != Color::Reset || self.cur_bg != Color::Reset {
            queue!(self.out, ResetColor)?;
            self.cur_fg = Color::Reset;
            self.cur_bg = Color::Reset;
        }

        self.out.flush()?;
        std::mem::swap(&mut self.front, &mut self.back);
        self.force_full = false;
        // The back buffer is fully repainted each frame by the caller, but if a
        // caller ever skips a region we must not show the stale colours.
        Ok(())
    }
}
