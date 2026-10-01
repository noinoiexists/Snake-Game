//! A terminal Snake with food that rots.
//!
//! Rendering is hand-rolled on top of `crossterm`: every frame is composed into
//! an off-screen cell buffer and diffed against the previous one, so only the
//! cells that actually changed reach the terminal.

#![forbid(unsafe_code)]

mod app;
mod canvas;
mod font;
mod game;
mod rng;
mod scores;
mod theme;
mod ui;
mod views;

use std::io::{self, Write};
use std::panic;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::App;
use canvas::Screen;

/// Target frame interval. 60 Hz is well within what the diff renderer can
/// sustain, and it makes the particles and menu shimmer look continuous.
const FRAME: Duration = Duration::from_millis(16);

/// Owns the terminal and guarantees it is handed back in a usable state.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Put the terminal back the way we found it. Safe to call more than once.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    let _ = io::stdout().flush();
}

/// If we panic mid-frame the terminal would otherwise be left in raw mode on
/// the alternate screen, which looks like a hung shell. Restore first, then
/// run the normal hook so the message still prints.
fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore();
        default_hook(info);
    }));
}

const USAGE: &str = "\
snake — a terminal Snake with food that rots

USAGE:
    snake [OPTIONS]

OPTIONS:
    -h, --help              Show this message
    -V, --version           Show the version
        --dump <VIEW>       Render one screen as plain text and exit,
                            without touching the terminal. VIEW is one of
                            menu, game, over, help, small.
        --size <WxH>        Size to render at when dumping (default 84x26)

CONTROLS:
    arrows / WASD / hjkl    Steer
    space, esc              Pause
    q                       Back to the menu (or quit, from the menu)
    r                       Play again from the pause screen
    ctrl-c                  Quit from anywhere
";

/// Options that are handled before the terminal is taken over.
enum Mode {
    Play,
    Dump { view: String, w: i32, h: i32 },
}

fn parse_args() -> Result<Option<Mode>, String> {
    let mut args = std::env::args().skip(1);
    let mut view = None;
    let mut size = (84, 26);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("snake {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--dump" => {
                view = Some(args.next().ok_or("--dump needs a view name")?);
            }
            "--size" => {
                let raw = args.next().ok_or("--size needs WxH")?;
                let (w, h) = raw
                    .split_once(['x', 'X'])
                    .ok_or("--size looks like 84x26")?;
                size = (
                    w.parse().map_err(|_| format!("bad width in {raw:?}"))?,
                    h.parse().map_err(|_| format!("bad height in {raw:?}"))?,
                );
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }

    Ok(Some(match view {
        Some(view) => Mode::Dump {
            view,
            w: size.0,
            h: size.1,
        },
        None => Mode::Play,
    }))
}

/// Render a screen to plain text, for eyeballing layout without a terminal.
fn dump(view: &str, w: i32, h: i32) -> Result<(), String> {
    let mut app = App::new(w, h);
    // Keep the preview independent of whatever is in the player's save file.
    app.scores = scores::Scores::in_memory();

    match view {
        "menu" => {}
        "help" => app.view = app::View::Help,
        "small" => app.resize(30, 12),
        "game" | "over" => {
            let (gw, gh) = views::playfield_size(w, h);
            let mut game = game::Game::new(gw, gh, game::Difficulty::Normal, 240);
            // Give the preview something to look at.
            let mut rng = rng::Rng::from_seed(20);
            for _ in 0..40 {
                game.update(1.0 / 30.0, &mut rng);
            }
            game.score = 180;
            game.combo = 3;
            if view == "over" {
                game.status = game::Status::Dead;
                game.update(0.1, &mut rng);
                app.last_run = Some(app::RunSummary {
                    score: 180,
                    length: 11,
                    eaten: 14,
                    missed: 2,
                    time: 96.0,
                    difficulty: game::Difficulty::Normal,
                    rank: None,
                });
                app.view = app::View::GameOver;
            }
            app.game = Some(game);
            if view == "game" {
                app.view = app::View::Game;
            }
        }
        other => return Err(format!("unknown view {other:?}")),
    }

    let mut canvas = canvas::Canvas::new(w, h);
    views::draw(&app, &mut canvas);

    // Plain text cannot show that a glyph is drawn in the background colour,
    // which is exactly how the heading's drop shadow works. Render those cells
    // as a light shade so the dump does not read as a smear of blocks.
    let shadow = app.theme.c(theme::palette::BG);
    for y in 0..h {
        let row: String = (0..w)
            .map(|x| {
                let cell = canvas.get(x, y);
                if cell.ch == '█' && cell.fg == shadow {
                    '░'
                } else {
                    cell.ch
                }
            })
            .collect();
        println!("{}", row.trim_end());
    }
    Ok(())
}

fn main() -> io::Result<()> {
    match parse_args() {
        Ok(None) => return Ok(()),
        Ok(Some(Mode::Dump { view, w, h })) => {
            if let Err(e) = dump(&view, w, h) {
                eprintln!("snake: {e}");
                std::process::exit(2);
            }
            return Ok(());
        }
        Ok(Some(Mode::Play)) => {}
        Err(e) => {
            eprintln!("snake: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    }

    install_panic_hook();

    let (cols, rows) = crossterm::terminal::size()?;
    let (cols, rows) = (cols.max(1) as i32, rows.max(1) as i32);

    let _guard = TerminalGuard::enter()?;
    let mut screen = Screen::new(cols, rows);
    let mut app = App::new(cols, rows);

    let mut last = Instant::now();
    while !app.should_quit {
        // Block until either input arrives or the next frame is due, so an
        // idle menu costs nothing and a keypress is felt immediately.
        let timeout = FRAME.saturating_sub(last.elapsed());
        if event::poll(timeout)? {
            // Drain everything queued; a fast typist can outrun the frame rate.
            loop {
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                    Event::Resize(w, h) => app.resize(w as i32, h as i32),
                    _ => {}
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        let now = Instant::now();
        let dt = now.saturating_duration_since(last);
        last = now;

        app.tick(dt);

        // Keep the drawing surface in step with the terminal, whatever the
        // cause of the change.
        if screen.size() != (app.width, app.height) {
            screen.resize(app.width, app.height);
        }

        views::draw(&app, screen.canvas());
        screen.present()?;
    }

    Ok(())
}
