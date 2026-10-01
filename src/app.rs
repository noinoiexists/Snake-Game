//! Application state: which screen is showing, and what keys do.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::game::{Difficulty, Dir, Game, Status};
use crate::rng::Rng;
use crate::scores::Scores;
use crate::theme::Theme;
use crate::views;

/// Which screen is on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Menu,
    Help,
    Game,
    GameOver,
}

/// The items on the main menu, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    Play,
    Difficulty,
    Autoplay,
    HowTo,
    Quit,
}

impl MenuItem {
    pub const ALL: [Self; 5] = [
        Self::Play,
        Self::Difficulty,
        Self::Autoplay,
        Self::HowTo,
        Self::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Difficulty => "Difficulty",
            Self::Autoplay => "Autoplay",
            Self::HowTo => "How to Play",
            Self::Quit => "Quit",
        }
    }

    /// One line explaining the row while it is highlighted. `None` for rows
    /// that speak for themselves.
    pub fn blurb(self) -> Option<&'static str> {
        match self {
            Self::Autoplay => Some("let the snake find its own way"),
            _ => None,
        }
    }
}

pub struct Menu {
    pub sel: usize,
    pub difficulty: Difficulty,
    /// Whether the chosen run should steer itself.
    pub autoplay: bool,
}

impl Menu {
    fn new(difficulty: Difficulty) -> Self {
        Self {
            sel: 0,
            difficulty,
            autoplay: false,
        }
    }

    pub fn item(&self) -> MenuItem {
        MenuItem::ALL[self.sel.min(MenuItem::ALL.len() - 1)]
    }

    fn move_by(&mut self, delta: i32) {
        let n = MenuItem::ALL.len() as i32;
        self.sel = (((self.sel as i32 + delta) % n + n) % n) as usize;
    }
}

/// Everything worth remembering about a finished run.
#[derive(Clone, Copy)]
pub struct RunSummary {
    pub score: u32,
    pub length: usize,
    pub eaten: u32,
    pub missed: u32,
    pub time: f32,
    pub difficulty: Difficulty,
    /// 0-based position in the high-score table, if it placed.
    pub rank: Option<usize>,
}

pub struct App {
    pub theme: Theme,
    pub view: View,
    pub menu: Menu,
    pub game: Option<Game>,
    pub scores: Scores,
    pub rng: Rng,
    pub width: i32,
    pub height: i32,
    pub should_quit: bool,
    /// Animated decorations: a travelling perimeter light and a slow shimmer.
    pub pulse: f32,
    pub phase: f32,
    pub last_run: Option<RunSummary>,
    pub new_best: bool,
    /// Set when the terminal is too small to draw a game.
    pub too_small: bool,
    /// Frame counter, used to blink things at a steady rate.
    pub frame: u64,
}

impl App {
    pub fn new(width: i32, height: i32) -> Self {
        let theme = Theme::new(crate::theme::ColorMode::detect());
        let scores = Scores::load();
        // Open on whatever the player last chose, defaulting to Normal.
        let seed = Rng::from_entropy();
        let difficulty = Difficulty::Normal;
        let mut app = Self {
            theme,
            view: View::Menu,
            menu: Menu::new(difficulty),
            game: None,
            scores,
            rng: seed,
            width,
            height,
            should_quit: false,
            pulse: 0.0,
            phase: 0.0,
            last_run: None,
            new_best: false,
            too_small: false,
            frame: 0,
        };
        app.refresh_size_flags();
        app
    }

    // --- geometry ---------------------------------------------------------

    fn refresh_size_flags(&mut self) {
        self.too_small = views::is_too_small(self.width, self.height);
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
        self.refresh_size_flags();
    }

    // --- main loop hooks --------------------------------------------------

    pub fn tick(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(0.1);
        self.phase += dt;
        self.frame = self.frame.wrapping_add(1);

        match self.view {
            View::Menu => {
                // The frame light orbits at a steady, calm pace.
                self.pulse += dt * 16.0;
            }
            View::Help => {
                self.pulse += dt * 10.0;
            }
            View::Game => {
                self.pulse += dt * 16.0;
                if let Some(game) = self.game.as_mut() {
                    game.update(dt, &mut self.rng);
                }
                if self.game.as_ref().is_some_and(Game::is_over) {
                    self.finish_run();
                }
            }
            View::GameOver => {
                self.pulse += dt * 16.0;
                // Keep simulating so the death sparks finish their arc.
                if let Some(game) = self.game.as_mut() {
                    game.update(dt, &mut self.rng);
                }
            }
        }
    }

    fn finish_run(&mut self) {
        let Some(g) = self.game.as_ref() else { return };
        let summary = RunSummary {
            score: g.score,
            length: g.len(),
            eaten: g.eaten,
            missed: g.misses,
            time: g.elapsed,
            difficulty: g.difficulty,
            rank: None,
        };

        let previous_best = self.scores.best(summary.difficulty);
        let rank = self.scores.submit(summary.score, summary.difficulty);
        self.scores.save();

        self.new_best = summary.score > previous_best && summary.score > 0;
        self.last_run = Some(RunSummary { rank, ..summary });
        self.view = View::GameOver;
    }

    fn start_game(&mut self, difficulty: Difficulty) {
        let (w, h) = views::playfield_size(self.width, self.height);
        let best = self.scores.best(difficulty);
        let mut game = Game::new(w, h, difficulty, best);
        game.autoplay = self.menu.autoplay;
        self.game = Some(game);
        self.new_best = false;
        self.view = View::Game;
    }

    // --- input ------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Ctrl-C still has to work: raw mode means we must honour it ourselves.
        if ctrl && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }

        match self.view {
            View::Menu => self.menu_key(key),
            View::Help => self.help_key(key),
            View::Game => self.game_key(key),
            View::GameOver => self.over_key(key),
        }
    }

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.menu.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.menu.move_by(1),
            KeyCode::Left | KeyCode::Char('h') => self.cycle_difficulty(false),
            KeyCode::Right | KeyCode::Char('l') => self.cycle_difficulty(true),
            KeyCode::Esc | KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_menu_item(),
            _ => {}
        }
    }

    fn cycle_difficulty(&mut self, forward: bool) {
        self.menu.difficulty = if forward {
            self.menu.difficulty.next()
        } else {
            self.menu.difficulty.next().next()
        };
    }

    fn activate_menu_item(&mut self) {
        match self.menu.item() {
            MenuItem::Play => self.start_game(self.menu.difficulty),
            MenuItem::Difficulty => self.cycle_difficulty(true),
            MenuItem::Autoplay => self.menu.autoplay = !self.menu.autoplay,
            MenuItem::HowTo => self.view = View::Help,
            MenuItem::Quit => self.should_quit = true,
        }
    }

    fn help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Char(' ')
            | KeyCode::Char('q')
            | KeyCode::Backspace => self.view = View::Menu,
            _ => {}
        }
    }

    fn game_key(&mut self, key: KeyEvent) {
        /// What a keypress asked for, resolved without holding a borrow on the
        /// game so the handlers below can freely touch the rest of `App`.
        enum Act {
            Turn(Dir),
            TogglePause,
            Restart,
            ToMenu,
        }

        let Some(game) = self.game.as_ref() else {
            self.view = View::Menu;
            return;
        };
        // Copy what we need; the borrow of `game` ends here.
        let difficulty = game.difficulty;
        let paused = game.status == Status::Paused;

        let act = if paused {
            // Paused: only resume, restart and the exits are live, so a stray
            // keypress cannot send the snake into a wall while you read.
            match key.code {
                KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char(' ') | KeyCode::Enter => {
                    Some(Act::TogglePause)
                }
                KeyCode::Char('r') => Some(Act::Restart),
                KeyCode::Char('q') => Some(Act::ToMenu),
                _ => None,
            }
        } else {
            match key.code {
                KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => Some(Act::Turn(Dir::Up)),
                KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                    Some(Act::Turn(Dir::Down))
                }
                KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => {
                    Some(Act::Turn(Dir::Left))
                }
                KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') => {
                    Some(Act::Turn(Dir::Right))
                }
                KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char(' ') => Some(Act::TogglePause),
                KeyCode::Char('q') => Some(Act::ToMenu),
                _ => None,
            }
        };

        match act {
            None => {}
            Some(Act::Turn(d)) => {
                if let Some(g) = self.game.as_mut() {
                    g.turn(d);
                }
            }
            Some(Act::TogglePause) => {
                if let Some(g) = self.game.as_mut() {
                    g.toggle_pause();
                }
            }
            Some(Act::Restart) => self.start_game(difficulty),
            Some(Act::ToMenu) => self.view = View::Menu,
        }
    }

    fn over_key(&mut self, key: KeyEvent) {
        let difficulty = self
            .last_run
            .map(|r| r.difficulty)
            .or_else(|| self.game.as_ref().map(|g| g.difficulty))
            .unwrap_or(Difficulty::Normal);

        match key.code {
            KeyCode::Char('r') | KeyCode::Enter | KeyCode::Char(' ') => self.start_game(difficulty),
            KeyCode::Char('m') | KeyCode::Esc | KeyCode::Backspace => {
                self.view = View::Menu;
                self.game = None;
            }
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// An app whose score table is detached from disk, so running the suite
    /// never clobbers the player's real high scores.
    fn app() -> App {
        let mut a = App::new(120, 40);
        a.scores = Scores::in_memory();
        a
    }

    #[test]
    fn menu_selection_wraps_both_ways() {
        let mut m = Menu::new(Difficulty::Normal);
        m.move_by(-1);
        assert_eq!(
            m.item(),
            MenuItem::Quit,
            "up from the top wraps to the bottom"
        );
        m.move_by(1);
        assert_eq!(m.item(), MenuItem::Play);
    }

    #[test]
    fn quit_item_requests_exit() {
        let mut a = app();
        a.menu.sel = MenuItem::ALL.len() - 1;
        assert_eq!(a.menu.item(), MenuItem::Quit);
        a.on_key(key(KeyCode::Enter));
        assert!(a.should_quit);
    }

    #[test]
    fn play_starts_a_game_at_the_chosen_difficulty() {
        let mut a = app();
        a.menu.difficulty = Difficulty::Insane;
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.view, View::Game);
        assert_eq!(a.game.as_ref().unwrap().difficulty, Difficulty::Insane);
    }

    #[test]
    fn the_menu_toggle_flips_autoplay_both_ways() {
        let mut a = app();
        a.menu.sel = MenuItem::ALL
            .iter()
            .position(|i| *i == MenuItem::Autoplay)
            .unwrap();
        assert!(!a.menu.autoplay, "autoplay should start off");

        a.on_key(key(KeyCode::Enter));
        assert!(a.menu.autoplay);
        assert_eq!(a.view, View::Menu, "the toggle must not start a game");

        a.on_key(key(KeyCode::Enter));
        assert!(!a.menu.autoplay, "a second press should turn it back off");
    }

    #[test]
    fn autoplay_carries_from_the_menu_into_the_run() {
        let mut a = app();
        a.menu.autoplay = true;
        a.start_game(Difficulty::Normal);
        assert!(a.game.as_ref().unwrap().autoplay);
    }

    #[test]
    fn ctrl_c_always_quits() {
        let mut a = app();
        a.view = View::Game;
        a.game = Some(Game::new(40, 20, Difficulty::Normal, 0));
        let mut k = key(KeyCode::Char('c'));
        k.modifiers = KeyModifiers::CONTROL;
        a.on_key(k);
        assert!(a.should_quit);
    }

    #[test]
    fn escape_pauses_rather_than_dropping_the_run() {
        let mut a = app();
        a.start_game(Difficulty::Normal);
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.view, View::Game);
        assert_eq!(a.game.as_ref().unwrap().status, Status::Paused);
    }

    #[test]
    fn arrow_keys_steer() {
        let mut a = app();
        a.start_game(Difficulty::Normal);
        let start = a.game.as_ref().unwrap().head();

        a.on_key(key(KeyCode::Up));

        // Turns are buffered and land on the next movement step rather than
        // being applied to `dir` the instant the key is read, so assert on
        // where the snake actually goes.
        for _ in 0..120 {
            a.tick(Duration::from_millis(16));
            if a.game.as_ref().unwrap().head() != start {
                break;
            }
        }
        let head = a.game.as_ref().unwrap().head();
        assert_eq!(
            head.x, start.x,
            "the snake drifted sideways while turning up"
        );
        assert!(
            head.y < start.y,
            "expected upward motion, head went to {head:?}"
        );
    }

    #[test]
    fn game_over_lands_on_the_summary_screen() {
        let mut a = app();
        a.start_game(Difficulty::Normal);
        if let Some(g) = a.game.as_mut() {
            g.score = 100;
            g.status = Status::Dead;
        }
        a.tick(Duration::from_millis(16));
        assert_eq!(a.view, View::GameOver);
        assert_eq!(a.last_run.unwrap().score, 100);
    }

    #[test]
    fn a_new_best_is_flagged() {
        let mut a = app();
        a.start_game(Difficulty::Normal);
        if let Some(g) = a.game.as_mut() {
            g.score = 999_999;
            g.status = Status::Dead;
        }
        a.tick(Duration::from_millis(16));
        assert!(a.new_best, "a record-breaking score should be celebrated");
    }
}
