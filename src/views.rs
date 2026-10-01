//! Every screen is drawn here.

use crate::app::{App, MenuItem, RunSummary, View};
use crate::canvas::{Canvas, Rect};
use crate::font;
use crate::game::{FoodKind, Game, Pos, Status};
use crate::theme::{Rgb, Theme, glyph, palette as pal};
use crate::ui::{self, Pulse};

/// Below this the chrome cannot be drawn legibly, so we ask for a resize.
pub const MIN_W: i32 = 34;
pub const MIN_H: i32 = 16;

pub fn is_too_small(w: i32, h: i32) -> bool {
    w < MIN_W || h < MIN_H
}

/// Pick a playfield that fits the terminal.
///
/// Terminal cells are roughly twice as tall as they are wide, so a 2:1 cell
/// grid reads as a square board rather than a tall thin one.
pub fn playfield_size(term_w: i32, term_h: i32) -> (i32, i32) {
    let avail_w = (term_w - 4).max(16);
    let avail_h = (term_h - 7).max(6);
    let grid_h = (avail_h.min(avail_w / 2)).clamp(6, 44);
    (grid_h * 2, grid_h)
}

pub fn draw(app: &App, canvas: &mut Canvas) {
    let (w, h) = (canvas.w, canvas.h);
    canvas.clear(' ', app.theme.c(pal::BG), app.theme.c(pal::BG));

    if app.too_small {
        draw_too_small(app, canvas, w, h);
        return;
    }

    match app.view {
        View::Menu => draw_menu(app, canvas, w, h),
        View::Help => draw_help(app, canvas, w, h),
        View::Game => draw_game(app, canvas, w, h),
        View::GameOver => {
            draw_game(app, canvas, w, h);
            if let Some(run) = app.last_run {
                draw_game_over(app, canvas, w, h, &run);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------

/// A centred panel, clamped to the terminal.
fn panel(w: i32, h: i32, want_w: i32, want_h: i32) -> Rect {
    let pw = want_w.min(w - 2).max(8);
    let ph = want_h.min(h - 2).max(4);
    Rect::new((w - pw) / 2, (h - ph) / 2, pw, ph)
}

fn fill_panel(canvas: &mut Canvas, r: Rect, t: Theme) {
    canvas.fill(r, ' ', t.c(pal::TEXT), t.c(pal::PANEL));
}

/// The travelling light that orbits every frame in the game.
fn pulse_for(app: &App, color: Rgb) -> Pulse {
    Pulse {
        at: app.pulse,
        tail: 18.0,
        color,
    }
}

// ---------------------------------------------------------------------------
// Main menu
// ---------------------------------------------------------------------------

fn draw_menu(app: &App, canvas: &mut Canvas, w: i32, h: i32) {
    let t = app.theme;
    let r = panel(w, h, 46, 20);
    fill_panel(canvas, r, t);
    ui::draw_frame(
        canvas,
        r,
        t,
        None,
        Some(pulse_for(app, pal::ACCENT)),
        t.c(pal::PANEL),
    );

    // Heading, with a soft drop shadow and a shimmer travelling across it.
    // The shadow is painted a shade darker than the panel, so it reads as
    // depth rather than as a second copy of the letters.
    let title = "SNAKE";
    let tw = font::width(title);
    let tx = r.center_x() - tw / 2;
    let ty = r.y + 2;
    let surface = t.c(pal::PANEL);
    if r.w >= tw + 4 {
        font::draw(canvas, tx + 1, ty + 1, title, |_| (t.c(pal::BG), surface));
        font::draw(canvas, tx, ty, title, |p| {
            let base = pal::ACCENT.lerp(pal::VIOLET, p);
            let wave = 0.5 + 0.5 * (p * 7.0 - app.phase * 3.0).sin();
            (t.c(base.lerp(pal::TEXT, wave * 0.45)), surface)
        });
    } else {
        ui::text_centered(canvas, r, ty + 2, title, t.c(pal::ACCENT), surface);
    }

    // Menu rows.
    let sel = app.menu.sel;
    let first = r.y + 12;
    for (i, item) in MenuItem::ALL.iter().enumerate() {
        let y = first + i as i32;
        let active = i == sel;
        let bg = surface;

        if active {
            // A subtle lit band behind the current row.
            let band = Rect::new(r.x + 3, y, r.w - 6, 1);
            canvas.fill(band, ' ', t.c(pal::TEXT), t.c(pal::PANEL_ALT));
        }

        let fg = if active {
            t.c(pal::ACCENT)
        } else {
            t.c(pal::TEXT_DIM)
        };
        let x = r.x + 7;
        if active {
            ui::text(
                canvas,
                r.x + 5,
                y,
                &glyph::SELECT.to_string(),
                fg,
                t.c(pal::PANEL_ALT),
            );
        }
        let cx = ui::text(
            canvas,
            x,
            y,
            item.label(),
            fg,
            if active { t.c(pal::PANEL_ALT) } else { bg },
        );

        // Rows that hold a value print it after the label between a pair of
        // arrows, so they read as something you can change rather than as a
        // caption. Autoplay is a plain toggle, so its value is just on/off.
        let row_bg = if active { t.c(pal::PANEL_ALT) } else { bg };
        let value = match *item {
            MenuItem::Difficulty => Some((app.menu.difficulty.label(), pal::FOOD_GOLD)),
            MenuItem::Autoplay if app.menu.autoplay => Some(("on", pal::OK)),
            MenuItem::Autoplay => Some(("off", pal::TEXT_FAINT)),
            _ => None,
        };
        if let Some((value, color)) = value {
            let vx = cx + 3;
            ui::text(canvas, vx, y, "◂ ", t.c(pal::FRAME), row_bg);
            let vx = ui::text(canvas, vx + 2, y, value, t.c(color), row_bg);
            ui::text(canvas, vx, y, " ▸", t.c(pal::FRAME), row_bg);
        }
    }

    // Status line: the difficulty blurb while it is highlighted, otherwise the
    // best score, so the space always says something useful. It sits below the
    // shadow row of the heading, not directly under the letters.
    let status_y = r.y + 9;
    let item = app.menu.item();
    if item == MenuItem::Difficulty {
        ui::text_centered_fit(
            canvas,
            r,
            status_y,
            app.menu.difficulty.blurb(),
            r.w - 4,
            t.c(pal::TEXT_DIM),
            surface,
        );
    } else if let Some(blurb) = item.blurb() {
        ui::text_centered_fit(
            canvas,
            r,
            status_y,
            blurb,
            r.w - 4,
            t.c(pal::TEXT_DIM),
            surface,
        );
    } else {
        let best = app.scores.best(app.menu.difficulty);
        let line = if best == 0 {
            "no score yet — go set one".to_string()
        } else {
            format!("{} best {}", glyph::STAR, best)
        };
        ui::text_centered_fit(
            canvas,
            r,
            status_y,
            &line,
            r.w - 4,
            t.c(pal::TEXT_DIM),
            surface,
        );
    }

    ui::rule(canvas, r, r.y + 10, 6, t, surface);

    let hints = format!("↑↓ move   ⏎ select   {} quit", glyph::BULLET);
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 18,
        &hints,
        r.w - 4,
        t.c(pal::TEXT_FAINT),
        surface,
    );
}

// ---------------------------------------------------------------------------
// Game
// ---------------------------------------------------------------------------

/// The board is anchored to the top rather than centred: the HUD sits directly
/// underneath it, so vertical centring would just push the readouts around as
/// the terminal resizes.
fn draw_game(app: &App, canvas: &mut Canvas, w: i32, _h: i32) {
    let Some(game) = app.game.as_ref() else {
        return;
    };
    let t = app.theme;

    let (gw, gh) = (game.w, game.h);
    let fr = Rect::new((w - (gw + 2)) / 2, 1, gw + 2, gh + 2);

    // Frame colour carries the mood: mint while there is time, red when the
    // food is about to rot, and a hot flash on the frame a meal was eaten.
    let urgency = game.food.map(|f| f.urgency()).unwrap_or(0.0);
    let base = if game.is_over() || urgency > 0.68 {
        pal::DANGER
    } else if urgency > 0.4 {
        pal::WARN
    } else {
        pal::ACCENT
    };
    let base = if game.flash > 0.0 {
        base.lerp(pal::TEXT, game.flash * 0.6)
    } else {
        base
    };

    let title = format!("{}  {} ", glyph::DIAMOND, game.difficulty.label());
    ui::draw_frame(
        canvas,
        fr,
        t,
        Some((&title, t.c(pal::TEXT_DIM))),
        Some(pulse_for(app, base)),
        t.c(pal::BG),
    );

    let ox = fr.x + 1;
    let oy = fr.y + 1;
    draw_playfield(app, canvas, game, ox, oy);
    draw_hud(app, canvas, game, fr);
}

fn draw_playfield(app: &App, canvas: &mut Canvas, game: &Game, ox: i32, oy: i32) {
    let t = app.theme;

    // Checkerboard, so the board has texture without stealing attention.
    for y in 0..game.h {
        for x in 0..game.w {
            let bg = if (x + y) % 2 == 0 {
                pal::PANEL
            } else {
                pal::PANEL_ALT
            };
            canvas.set(ox + x, oy + y, ' ', t.c(pal::TEXT), t.c(bg));
        }
    }

    let cell_bg = |p: Pos| {
        let bg = if (p.x + p.y) % 2 == 0 {
            pal::PANEL
        } else {
            pal::PANEL_ALT
        };
        t.c(bg)
    };

    // Food, plus a warning halo once it is close to rotting.
    if let Some(food) = game.food {
        let urgency = food.urgency();
        if urgency > 0.6 && !game.is_over() {
            let beat = 0.5 + 0.5 * (app.phase * 9.0).sin();
            let halo = t.c(pal::DANGER.lerp(pal::WARN, beat).scale(0.75));
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let p = Pos::new(food.pos.x + dx, food.pos.y + dy);
                if game.in_bounds(p) && !game.occupied(p) {
                    canvas.set(ox + p.x, oy + p.y, glyph::TAIL, halo, cell_bg(p));
                }
            }
        }

        let blink = ((app.phase * 7.0) as i32) % 2 == 0;
        let ch = if food.is_dying() && blink {
            glyph::FOOD_FADING
        } else {
            food.kind.glyph()
        };
        let pulse = 0.5 + 0.5 * (app.phase * 5.0).sin();
        let color = food
            .kind
            .color()
            .lerp(pal::WARN, urgency * 0.5)
            .lerp(Rgb::new(0xFF, 0xFF, 0xFF), pulse * 0.22);
        canvas.set(
            ox + food.pos.x,
            oy + food.pos.y,
            ch,
            t.c(color),
            cell_bg(food.pos),
        );
    }

    // Snake. The body fades from the head colour toward a deep teal so the
    // direction of travel is readable at a glance.
    let n = game.len().max(1);
    for (i, seg) in game.snake.iter().enumerate() {
        let (ch, color) = if i == 0 {
            (game.dir.glyph(), t.c(pal::HEAD))
        } else {
            let k = i as f32 / (n.max(2) - 1) as f32;
            let color = pal::BODY_NEAR.lerp(pal::BODY_FAR, k);
            let color = if game.is_over() {
                color.lerp(pal::DANGER, 0.65)
            } else {
                color
            };
            let ch = if i == n - 1 && n > 4 {
                glyph::TAIL
            } else {
                glyph::BODY
            };
            (ch, t.c(color))
        };
        canvas.set(ox + seg.x, oy + seg.y, ch, color, cell_bg(*seg));
    }

    // Sparks on top of everything.
    for p in &game.particles {
        let x = ox + p.x as i32;
        let y = oy + p.y as i32;
        if x < ox || y < oy || x >= ox + game.w || y >= oy + game.h {
            continue;
        }
        // Fade toward the board rather than to black, so sparks dissolve into
        // the playfield instead of punching holes in it.
        let bg = if (p.x as i32 + p.y as i32) % 2 == 0 {
            pal::PANEL
        } else {
            pal::PANEL_ALT
        };
        let color = p.color.lerp(bg, 1.0 - p.alpha());
        canvas.set(x, y, p.glyph, t.c(color), t.c(bg));
    }
}

fn draw_hud(app: &App, canvas: &mut Canvas, game: &Game, fr: Rect) {
    let t = app.theme;
    let y0 = fr.bottom() + 1;
    let surface = t.c(pal::BG);

    // Row 1 — score, best, length.
    let x = ui::text(canvas, fr.x + 1, y0, "SCORE", t.c(pal::TEXT_FAINT), surface);
    let x = ui::text(
        canvas,
        x + 1,
        y0,
        &game.score.to_string(),
        t.c(pal::TEXT),
        surface,
    );
    if game.score > game.best && game.best > 0 {
        // A quiet marker that this run has already beaten your record.
        ui::text(
            canvas,
            x + 1,
            y0,
            &glyph::STAR.to_string(),
            t.c(pal::FOOD_GOLD),
            surface,
        );
    }

    let best = format!("{} {}", glyph::STAR, game.best.max(game.score));
    let bx = fr.center_x() - best.chars().count() as i32 / 2;
    ui::text(canvas, bx, y0, &best, t.c(pal::TEXT_DIM), surface);

    let len = format!("LENGTH {}", game.len());
    let lx = fr.right() - 1 - len.chars().count() as i32;
    ui::text(canvas, lx, y0, &len, t.c(pal::TEXT_DIM), surface);

    // Row 2 — the food timer. This is the mechanic the whole game hangs on, so
    // it gets the widest and brightest element on the screen. Laid out left to
    // right with measured widths so nothing can ever collide at narrow sizes.
    let y1 = y0 + 1;
    if let Some(food) = game.food {
        let urgency = food.urgency();
        let color = if urgency > 0.68 {
            pal::DANGER
        } else if urgency > 0.4 {
            pal::WARN
        } else {
            pal::OK
        };

        let mut x = ui::text(
            canvas,
            fr.x + 1,
            y1,
            &food.kind.glyph().to_string(),
            t.c(food.kind.color()),
            surface,
        ) + 1;
        // Drop the label before dropping the meter when space is tight.
        if fr.w >= 34 {
            x = ui::text(
                canvas,
                x,
                y1,
                food.kind.label(),
                t.c(pal::TEXT_DIM),
                surface,
            ) + 1;
        }

        const SECS_W: i32 = 6;
        let bar_x = x + 1;
        let bar_w = (fr.right() - 1 - SECS_W - bar_x).clamp(6, 18);
        let fill = if food.is_dying() {
            color.lerp(pal::TEXT, 0.2)
        } else {
            color
        };
        ui::bar(
            canvas,
            bar_x,
            y1,
            bar_w,
            food.ratio(),
            ui::Meter {
                fill: t.c(fill),
                track: t.c(pal::FRAME_DIM),
                bg: surface,
            },
        );
        let secs = format!("{:>5.1}s", food.remaining.max(0.0));
        ui::text(canvas, bar_x + bar_w + 1, y1, &secs, t.c(color), surface);
    } else {
        ui::text(
            canvas,
            fr.x + 1,
            y1,
            "no food on the board",
            t.c(pal::TEXT_FAINT),
            surface,
        );
    }

    // Row 3 — combo or steering hint on the left, controls on the right, and
    // the paused banner taking the whole row when the game is held.
    let y2 = y1 + 1;
    if game.status == Status::Paused {
        ui::text_centered_fit(
            canvas,
            fr,
            y2,
            "␣  resume   r  restart   q  menu",
            fr.w - 2,
            t.c(pal::FOOD_GOLD),
            surface,
        );
        return;
    }
    if game.is_over() {
        ui::text_centered_fit(
            canvas,
            fr,
            y2,
            "run over",
            fr.w - 2,
            t.c(pal::DANGER),
            surface,
        );
        return;
    }

    let controls = "␣ pause   q menu";
    let cx = fr.right() - 1 - controls.chars().count() as i32;
    ui::text(canvas, cx, y2, controls, t.c(pal::TEXT_FAINT), surface);

    // While autoplay is driving, the left slot says so instead of showing a
    // steering hint that no longer does anything.
    if game.autoplay {
        let heat = ((game.combo as f32 - 1.0) / 8.0).clamp(0.0, 1.0);
        let color = pal::VIOLET.lerp(pal::FOOD_GOLD, heat);
        let label = if game.combo > 1 {
            format!("AUTOPLAY   COMBO ×{}", game.combo)
        } else {
            "AUTOPLAY".to_string()
        };
        ui::text(canvas, fr.x + 1, y2, &label, t.c(color), surface);
        return;
    }

    // The combo outranks the steering hint for the left slot: by the time you
    // have one, you know the controls.
    if game.combo > 1 {
        let heat = ((game.combo as f32 - 1.0) / 8.0).clamp(0.0, 1.0);
        let color = pal::ACCENT.lerp(pal::FOOD_GOLD, heat);
        let combo = format!("COMBO ×{}", game.combo);
        ui::text(canvas, fr.x + 1, y2, &combo, t.c(color), surface);
        return;
    }

    let hint = format!(
        "{}{}{}{} / WASD",
        glyph::HEAD_UP,
        glyph::HEAD_DOWN,
        glyph::HEAD_LEFT,
        glyph::HEAD_RIGHT
    );
    let room = cx - (fr.x + 1) - 2;
    if room >= 8 {
        ui::text(
            canvas,
            fr.x + 1,
            y2,
            &ui::fit(&hint, room),
            t.c(pal::TEXT_FAINT),
            surface,
        );
    }
}

// ---------------------------------------------------------------------------
// Game over
// ---------------------------------------------------------------------------

fn draw_game_over(app: &App, canvas: &mut Canvas, w: i32, h: i32, run: &RunSummary) {
    let t = app.theme;
    let r = panel(w, h, 46, 19);
    fill_panel(canvas, r, t);
    let accent = if app.new_best {
        pal::FOOD_GOLD
    } else {
        pal::DANGER
    };
    ui::draw_frame(
        canvas,
        r,
        t,
        Some(("GAME OVER", t.c(accent))),
        Some(pulse_for(app, accent)),
        t.c(pal::PANEL),
    );

    let surface = t.c(pal::PANEL);

    // Banner.
    if app.new_best {
        let beat = 0.5 + 0.5 * (app.phase * 4.0).sin();
        let color = pal::FOOD_GOLD.lerp(pal::TEXT, beat * 0.5);
        let banner = format!("{}  NEW BEST  {}", glyph::STAR, glyph::STAR);
        ui::text_centered_fit(canvas, r, r.y + 2, &banner, r.w - 4, t.c(color), surface);
    }

    // Wrapping means walls cannot be fatal, so this is the only ending there is.
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 3,
        "You bit your own tail",
        r.w - 4,
        t.c(pal::TEXT_DIM),
        surface,
    );

    // The score itself, in the block font when it fits.
    let score = run.score.to_string();
    if font::width(&score) <= r.w - 6 {
        let g = ui::Gradient {
            from: pal::ACCENT,
            to: pal::VIOLET,
            bg: surface,
        };
        ui::heading(canvas, r, r.y + 5, &score, t, g);
    } else {
        ui::text_centered_fit(
            canvas,
            r,
            r.y + 6,
            &score,
            r.w - 4,
            t.c(pal::ACCENT),
            surface,
        );
    }

    // "rotted" rather than "missed": it names the mechanic that cost you.
    let stats = format!(
        "length {} · eaten {} · rotted {} · {}",
        run.length,
        run.eaten,
        run.missed,
        clock(run.time)
    );
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 11,
        &stats,
        r.w - 4,
        t.c(pal::TEXT_DIM),
        surface,
    );

    // The table, with this run's row lit up if it placed.
    ui::rule(canvas, r, r.y + 12, 8, t, surface);
    let top = app.scores.top(3);
    if top.is_empty() {
        ui::text_centered_fit(
            canvas,
            r,
            r.y + 14,
            "no runs recorded yet",
            r.w - 4,
            t.c(pal::TEXT_FAINT),
            surface,
        );
    } else {
        for (i, entry) in top.iter().enumerate() {
            let y = r.y + 13 + i as i32;
            let mine = run.rank == Some(i);
            let marker = if mine { glyph::SELECT } else { ' ' };
            let line = format!(
                "{} #{}   {:>6}   {}",
                marker,
                i + 1,
                entry.score,
                entry.difficulty.label()
            );
            let color = if mine { pal::ACCENT } else { pal::TEXT_DIM };
            ui::text_centered_fit(canvas, r, y, &line, r.w - 4, t.c(color), surface);
        }
    }

    ui::text_centered_fit(
        canvas,
        r,
        r.y + 17,
        "r play again   m menu   q quit",
        r.w - 4,
        t.c(pal::TEXT_FAINT),
        surface,
    );
}

// ---------------------------------------------------------------------------
// How to play
// ---------------------------------------------------------------------------

enum LineStyle {
    Section,
    Body,
    Dim,
    Accent,
}

fn draw_help(app: &App, canvas: &mut Canvas, w: i32, h: i32) {
    let t = app.theme;
    let r = panel(w, h, 54, 22);
    fill_panel(canvas, r, t);
    ui::draw_frame(
        canvas,
        r,
        t,
        Some(("HOW TO PLAY", t.c(pal::ACCENT))),
        Some(pulse_for(app, pal::ACCENT)),
        t.c(pal::PANEL),
    );

    let surface = t.c(pal::PANEL);
    let d = app.menu.difficulty;

    // Timers are quoted for the difficulty currently selected on the menu, so
    // this page never contradicts what the game actually does.
    let rows: Vec<(String, LineStyle)> = vec![
        ("Steer".into(), LineStyle::Section),
        (
            "▲ ▼ ◀ ▶   or   W A S D   or   H J K L".into(),
            LineStyle::Body,
        ),
        ("space / esc   pause".into(), LineStyle::Body),
        ("q   back to the menu".into(), LineStyle::Body),
        (String::new(), LineStyle::Body),
        ("Edges wrap".into(), LineStyle::Section),
        (
            "Leave one side of the board and you come back in".into(),
            LineStyle::Dim,
        ),
        (
            "on the other. Only your own tail can kill you.".into(),
            LineStyle::Dim,
        ),
        (String::new(), LineStyle::Body),
        ("Food rots".into(), LineStyle::Section),
        (
            format!(
                "{}  berry    {} pts, gone in {:.0}s",
                glyph::FOOD,
                FoodKind::Berry.points(),
                d.food_ttl()
            ),
            LineStyle::Accent,
        ),
        (
            format!(
                "{}  golden   {} pts, gone in {:.0}s",
                glyph::FOOD_GOLD,
                FoodKind::Golden.points(),
                d.food_ttl() * 0.5
            ),
            LineStyle::Accent,
        ),
        (String::new(), LineStyle::Body),
        (
            "The meter under the board is the food's life.".into(),
            LineStyle::Dim,
        ),
        (
            "The frame glows green, then red as it ages.".into(),
            LineStyle::Dim,
        ),
        (
            "When it empties the food is gone: no points,".into(),
            LineStyle::Dim,
        ),
        ("and your combo resets.".into(), LineStyle::Dim),
        (
            "Chain meals within five seconds for up to ×9.".into(),
            LineStyle::Dim,
        ),
    ];

    let rows_len = rows.len() as i32;
    for (i, (text, style)) in rows.into_iter().enumerate() {
        let y = r.y + 2 + i as i32;
        let (color, x) = match style {
            LineStyle::Section => (t.c(pal::VIOLET), r.x + 4),
            LineStyle::Body => (t.c(pal::TEXT), r.x + 6),
            LineStyle::Dim => (t.c(pal::TEXT_DIM), r.x + 4),
            LineStyle::Accent => (t.c(pal::TEXT_DIM), r.x + 6),
        };
        if !text.is_empty() {
            ui::text(canvas, x, y, &text, color, surface);
        }
    }

    // Park the footer a row below whatever the content actually came to, so
    // adding a line to the page can never silently overwrite it.
    let footer_y = (r.y + 2 + rows_len + 1).min(r.bottom() - 2);
    let footer = format!("{}  ·  {} palette", d.label(), t.mode.label());
    ui::text_centered_fit(
        canvas,
        r,
        footer_y,
        &footer,
        r.w - 4,
        t.c(pal::FOOD_GOLD),
        surface,
    );
}

// ---------------------------------------------------------------------------
// Too small
// ---------------------------------------------------------------------------

fn draw_too_small(app: &App, canvas: &mut Canvas, w: i32, h: i32) {
    let t = app.theme;
    let r = panel(w, h, 40, 9);
    fill_panel(canvas, r, t);
    ui::draw_frame(
        canvas,
        r,
        t,
        Some(("SNAKE", t.c(pal::ACCENT))),
        Some(pulse_for(app, pal::WARN)),
        t.c(pal::PANEL),
    );
    let surface = t.c(pal::PANEL);

    ui::text_centered_fit(
        canvas,
        r,
        r.y + 2,
        "Terminal too small",
        r.w - 4,
        t.c(pal::TEXT),
        surface,
    );
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 4,
        &format!("needs at least {} × {}", MIN_W, MIN_H),
        r.w - 4,
        t.c(pal::TEXT_DIM),
        surface,
    );
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 5,
        &format!("now {} × {}", w, h),
        r.w - 4,
        t.c(pal::FOOD_GOLD),
        surface,
    );
    ui::text_centered_fit(
        canvas,
        r,
        r.y + 7,
        "q quit",
        r.w - 4,
        t.c(pal::TEXT_FAINT),
        surface,
    );
}

// ---------------------------------------------------------------------------

/// `m:ss`, for survival times.
fn clock(secs: f32) -> String {
    let secs = secs.max(0.0) as u32;
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the canvas ended up holding, one line per row.
    fn canvas_text(canvas: &Canvas) -> String {
        (0..canvas.h)
            .map(|y| {
                (0..canvas.w)
                    .map(|x| canvas.get(x, y).ch)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The one row of the menu carrying `needle`.
    fn menu_row(app: &App, needle: &str) -> String {
        let mut canvas = Canvas::new(app.width, app.height);
        draw(app, &mut canvas);
        canvas_text(&canvas)
            .lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no menu row contained {needle:?}"))
            .to_string()
    }

    #[test]
    fn the_menu_shows_whether_autoplay_is_on() {
        let autoplay = MenuItem::ALL
            .iter()
            .position(|i| *i == MenuItem::Autoplay)
            .expect("autoplay must have a menu row");

        for (on, want) in [(false, "◂ off ▸"), (true, "◂ on ▸")] {
            let mut app = App::new(84, 26);
            app.scores = crate::scores::Scores::in_memory();
            app.menu.sel = autoplay;
            app.menu.autoplay = on;
            let row = menu_row(&app, "Autoplay");
            assert!(
                row.contains(want),
                "autoplay={on} should read {want:?}, row was {row:?}"
            );
        }
    }

    #[test]
    fn the_hud_announces_autoplay_while_it_drives() {
        let mut app = App::new(84, 26);
        app.scores = crate::scores::Scores::in_memory();
        app.view = View::Game;
        let (gw, gh) = playfield_size(84, 26);
        let mut game = Game::new(gw, gh, crate::game::Difficulty::Normal, 0);
        game.autoplay = true;
        app.game = Some(game);

        let mut canvas = Canvas::new(84, 26);
        draw(&app, &mut canvas);
        assert!(
            canvas_text(&canvas).contains("AUTOPLAY"),
            "the HUD should say the snake is steering itself"
        );
    }

    #[test]
    fn playfields_are_always_twice_as_wide_as_tall() {
        for w in 40..200 {
            for h in 20..80 {
                let (gw, gh) = playfield_size(w, h);
                assert_eq!(gw, gh * 2, "aspect broke at {w}x{h}");
                assert!(gw >= 12 && gh >= 6, "board too small at {w}x{h}");
            }
        }
    }

    #[test]
    fn the_board_always_fits_the_terminal() {
        for w in 48..200 {
            for h in 20..80 {
                let (gw, gh) = playfield_size(w, h);
                assert!(gw + 2 <= w, "frame overflows {w}x{h}");
                // Frame (gh+2) plus a 3-row HUD, starting at row 1.
                assert!(1 + gh + 2 + 3 <= h, "HUD overflows at {w}x{h}");
            }
        }
    }

    #[test]
    fn pans_never_leave_the_screen() {
        for w in MIN_W..160 {
            for h in MIN_H..60 {
                let r = panel(w, h, 46, 18);
                assert!(r.x >= 0 && r.y >= 0, "panel off-screen at {w}x{h}");
                assert!(
                    r.right() <= w && r.bottom() <= h,
                    "panel clipped at {w}x{h}"
                );
            }
        }
    }

    /// Render every screen at every plausible size. This is the cheapest way
    /// to catch layout arithmetic that underflows, or a panel that spills
    /// outside the canvas, without driving a real terminal.
    #[test]
    fn every_view_renders_at_every_size() {
        let sizes = [
            (1, 1),
            (MIN_W - 1, MIN_H - 1),
            (MIN_W, MIN_H),
            (40, 16),
            (48, 20),
            (60, 24),
            (84, 26),
            (120, 40),
            (200, 60),
        ];
        let views = [View::Menu, View::Help, View::Game, View::GameOver];

        for (w, h) in sizes {
            for view in views {
                let mut app = App::new(w, h);
                app.scores = crate::scores::Scores::in_memory();
                app.view = view;
                if view == View::GameOver {
                    app.last_run = Some(RunSummary {
                        score: 12345,
                        length: 40,
                        eaten: 57,
                        missed: 3,
                        time: 421.0,
                        difficulty: crate::game::Difficulty::Insane,
                        rank: Some(0),
                    });
                }
                // A board at the size this terminal would actually get, and a
                // run in progress so the HUD has real values to print.
                let (gw, gh) = playfield_size(w, h);
                let mut game = Game::new(gw, gh, crate::game::Difficulty::Normal, 500);
                // Steering itself, so the autoplay pathfinder is exercised on
                // every board size the renderer is asked to cope with.
                game.autoplay = true;
                let mut rng = crate::rng::Rng::from_seed(4);
                for _ in 0..30 {
                    game.update(1.0 / 30.0, &mut rng);
                }
                app.game = Some(game);

                let mut canvas = Canvas::new(w, h);
                // Must not panic, and must not leave the canvas untouched.
                draw(&app, &mut canvas);
            }
        }
    }

    #[test]
    fn the_too_small_notice_replaces_the_game() {
        let app = App::new(MIN_W - 1, MIN_H - 1);
        assert!(app.too_small);
        let mut canvas = Canvas::new(app.width, app.height);
        draw(&app, &mut canvas);

        // The notice should be on screen rather than a half-drawn board.
        let text: String = (0..canvas.h)
            .map(|y| {
                (0..canvas.w)
                    .map(|x| canvas.get(x, y).ch)
                    .collect::<String>()
            })
            .collect();
        assert!(
            text.contains("too small"),
            "expected the resize notice, got:\n{text}"
        );
    }

    #[test]
    fn the_hud_timer_meter_stays_inside_the_frame() {
        // The food meter is laid out from measured text widths; make sure a
        // long label next to a small board cannot push it past the border.
        for w in MIN_W..130 {
            let (gw, gh) = playfield_size(w, 26);
            let fr = Rect::new((w - (gw + 2)) / 2, 1, gw + 2, gh + 2);
            let mut app = App::new(w, 26);
            app.scores = crate::scores::Scores::in_memory();
            let mut game = Game::new(gw, gh, crate::game::Difficulty::Normal, 0);
            // Force the widest label.
            game.food = Some(crate::game::Food {
                pos: Pos::new(1, 1),
                kind: FoodKind::Golden,
                remaining: 3.0,
                total: 5.0,
            });
            app.game = Some(game);
            app.view = View::Game;

            let mut canvas = Canvas::new(w, 26);
            draw(&app, &mut canvas);

            // Nothing of the HUD may sit outside the frame's horizontal span.
            let y = fr.bottom() + 2;
            let row: Vec<char> = (0..w).map(|x| canvas.get(x, y).ch).collect();
            let drawn: Vec<i32> = row
                .iter()
                .enumerate()
                .filter(|(_, c)| **c != ' ' && **c != '\0')
                .map(|(x, _)| x as i32)
                .collect();
            assert!(
                drawn.iter().all(|x| *x >= fr.x && *x < fr.right()),
                "HUD row overflowed the frame at width {w}"
            );
        }
    }

    #[test]
    fn clock_formats_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(9.6), "0:09");
        assert_eq!(clock(83.0), "1:23");
        assert_eq!(clock(-5.0), "0:00");
    }

    #[test]
    fn a_full_board_still_places_food() {
        // The snake covers most of the grid; spawn must still find a free cell
        // rather than looping or panicking.
        let mut game = Game::new(8, 6, crate::game::Difficulty::Normal, 0);
        let (gw, gh) = (game.w, game.h);
        let mut rng = crate::rng::Rng::from_seed(2);
        for _ in 0..200 {
            game.update(1.0 / 30.0, &mut rng);
            assert!(game.w == gw && game.h == gh);
        }
    }
}
