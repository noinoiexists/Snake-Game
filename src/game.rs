//! Snake rules, food lifetime and the particle system.
//!
//! Nothing in here knows about terminals or colours — it is pure simulation
//! over a grid of `w * h` cells, with one twist over the arcade original: food
//! rots. A piece that is not eaten in time vanishes and breaks your combo, so
//! the board is never safe to ignore.

use std::collections::VecDeque;

use crate::rng::Rng;
use crate::theme::{Rgb, glyph, palette};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Height-to-width ratio of a terminal cell. Used to translate grid steps into
/// equal on-screen distances.
const CELL_ASPECT: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    /// True if the two directions are 180 degrees apart. Turning back on
    /// yourself is the one move snake forbids.
    pub fn is_opposite(self, other: Self) -> bool {
        let (ax, ay) = self.delta();
        let (bx, by) = other.delta();
        ax + bx == 0 && ay + by == 0
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Up | Self::Down)
    }

    pub fn glyph(self) -> char {
        glyph::head(self)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Chill,
    Normal,
    Insane,
}

impl Difficulty {
    pub const ALL: [Self; 3] = [Self::Chill, Self::Normal, Self::Insane];

    pub fn label(self) -> &'static str {
        match self {
            Self::Chill => "Chill",
            Self::Normal => "Normal",
            Self::Insane => "Insane",
        }
    }

    /// Short line shown under the difficulty picker.
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Chill => "slow snake, patient food",
            Self::Normal => "the classic balance",
            Self::Insane => "fast snake, food rots quickly",
        }
    }

    /// Starting speed in cells per second.
    pub fn start_speed(self) -> f32 {
        match self {
            Self::Chill => 6.0,
            Self::Normal => 8.5,
            Self::Insane => 12.0,
        }
    }

    /// Speed ceiling, approached as the snake grows.
    pub fn max_speed(self) -> f32 {
        match self {
            Self::Chill => 14.0,
            Self::Normal => 19.0,
            Self::Insane => 27.0,
        }
    }

    /// How long a berry survives before it rots, in seconds.
    pub fn food_ttl(self) -> f32 {
        match self {
            Self::Chill => 14.0,
            Self::Normal => 10.0,
            Self::Insane => 6.5,
        }
    }

    /// Index into `ALL`, used for the on-disk score table.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|d| *d == self).unwrap_or(1)
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Normal)
    }

    pub fn next(self) -> Self {
        match self {
            Self::Chill => Self::Normal,
            Self::Normal => Self::Insane,
            Self::Insane => Self::Chill,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoodKind {
    Berry,
    Golden,
}

impl FoodKind {
    pub fn points(self) -> u32 {
        match self {
            Self::Berry => 10,
            Self::Golden => 25,
        }
    }

    pub fn growth(self) -> u32 {
        match self {
            Self::Berry => 1,
            Self::Golden => 2,
        }
    }

    /// Golden fruit is worth more but rots far faster — that is the trade.
    pub fn ttl(self, difficulty: Difficulty) -> f32 {
        match self {
            Self::Berry => difficulty.food_ttl(),
            Self::Golden => difficulty.food_ttl() * 0.5,
        }
    }

    pub fn color(self) -> Rgb {
        match self {
            Self::Berry => palette::FOOD,
            Self::Golden => palette::FOOD_GOLD,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Berry => "Berry",
            Self::Golden => "Golden",
        }
    }

    pub fn glyph(self) -> char {
        match self {
            Self::Berry => glyph::FOOD,
            Self::Golden => glyph::FOOD_GOLD,
        }
    }
}

/// A piece of food with a shelf life.
#[derive(Clone, Copy, Debug)]
pub struct Food {
    pub pos: Pos,
    pub kind: FoodKind,
    pub remaining: f32,
    pub total: f32,
}

impl Food {
    /// Fraction of the shelf life left, in `0.0..=1.0`.
    pub fn ratio(&self) -> f32 {
        if self.total <= 0.0 {
            0.0
        } else {
            (self.remaining / self.total).clamp(0.0, 1.0)
        }
    }

    /// True once it is close enough to rotting that the UI should shout.
    pub fn is_dying(&self) -> bool {
        self.ratio() < 0.32
    }

    /// 0.0 (fresh) to 1.0 (about to rot), for colour ramps.
    pub fn urgency(&self) -> f32 {
        1.0 - self.ratio()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub glyph: char,
    pub color: Rgb,
}

impl Particle {
    /// 1.0 at birth, 0.0 at death.
    pub fn alpha(&self) -> f32 {
        if self.max_life <= 0.0 {
            0.0
        } else {
            (self.life / self.max_life).clamp(0.0, 1.0)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Playing,
    Paused,
    /// The board wraps at the edges, so running out of room is impossible and
    /// biting your own tail is the only way to lose.
    Dead,
}

pub struct Game {
    pub w: i32,
    pub h: i32,
    pub snake: VecDeque<Pos>,
    pub dir: Dir,
    pub food: Option<Food>,
    pub score: u32,
    pub eaten: u32,
    pub misses: u32,
    pub combo: u32,
    pub difficulty: Difficulty,
    pub particles: Vec<Particle>,
    pub status: Status,
    pub elapsed: f32,
    /// Decaying 0..1 pulse used for screen feedback on eating and dying.
    pub flash: f32,
    /// High score to beat, for display only.
    pub best: u32,

    /// Directions buffered from input, applied one per step so quick
    /// double-turns are not swallowed.
    pending: VecDeque<Dir>,
    /// Segments still owed to the snake from food already eaten.
    growth: u32,
    /// Seconds accumulated toward the next movement step.
    tick: f32,
    /// Seconds since the last meal, for combo decay.
    since_meal: f32,
}

impl Game {
    pub fn new(w: i32, h: i32, difficulty: Difficulty, best: u32) -> Self {
        let (w, h) = (w.max(8), h.max(6));
        let cy = h / 2;
        let cx = w / 2;
        let mut snake = VecDeque::new();
        for i in 0..3 {
            snake.push_back(Pos::new(cx - i, cy));
        }
        Self {
            w,
            h,
            snake,
            dir: Dir::Right,
            food: None,
            score: 0,
            eaten: 0,
            misses: 0,
            combo: 1,
            difficulty,
            particles: Vec::new(),
            status: Status::Playing,
            elapsed: 0.0,
            flash: 0.0,
            best,
            pending: VecDeque::new(),
            growth: 0,
            tick: 0.0,
            since_meal: 0.0,
        }
    }

    pub fn len(&self) -> usize {
        self.snake.len()
    }

    pub fn is_over(&self) -> bool {
        matches!(self.status, Status::Dead)
    }

    pub fn head(&self) -> Pos {
        self.snake[0]
    }

    /// True if any part of the snake is on `p`.
    pub fn occupied(&self, p: Pos) -> bool {
        self.snake.contains(&p)
    }

    /// True if `p` is inside the board.
    pub fn in_bounds(&self, p: Pos) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.w && p.y < self.h
    }

    /// Current speed in cells per second — grows with every meal.
    pub fn speed(&self) -> f32 {
        let d = self.difficulty;
        (d.start_speed() + self.eaten as f32 * 0.18).min(d.max_speed())
    }

    /// How long the current step should take.
    ///
    /// A terminal cell is roughly twice as tall as it is wide, so a vertical
    /// step covers about twice the distance on screen as a horizontal one.
    /// Giving vertical steps the same factor of extra time keeps the snake's
    /// apparent speed the same whichever way it is travelling, and makes
    /// crossing the board take as long vertically as it does horizontally.
    fn step_interval(&self) -> f32 {
        let base = 1.0 / self.speed();
        if self.dir.is_vertical() {
            base * CELL_ASPECT
        } else {
            base
        }
    }

    /// Buffer a turn. Reversals and repeats are dropped.
    pub fn turn(&mut self, d: Dir) {
        if !matches!(self.status, Status::Playing) {
            return;
        }
        let last = self.pending.back().copied().unwrap_or(self.dir);
        if last == d || d.is_opposite(last) {
            return;
        }
        // Three queued turns is the most a human can mean at once.
        if self.pending.len() < 3 {
            self.pending.push_back(d);
        }
    }

    pub fn toggle_pause(&mut self) {
        self.status = match self.status {
            Status::Playing => Status::Paused,
            Status::Paused => Status::Playing,
            dead => dead,
        };
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32, rng: &mut Rng) {
        // Particles and feedback run on every screen, including after death.
        self.flash = (self.flash - dt * 2.5).max(0.0);
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            // Cells are roughly twice as tall as they are wide, so vertical
            // motion and gravity are scaled to look right on screen.
            p.vy += 16.0 * dt;
            p.vx *= (1.0 - 2.0 * dt).max(0.0);
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);

        if !matches!(self.status, Status::Playing) {
            return;
        }

        self.elapsed += dt;
        self.since_meal += dt;
        if self.combo > 1 && self.since_meal > 5.0 {
            self.combo = 1;
        }

        // Rot the food.
        let mut rotted = false;
        if let Some(f) = &mut self.food {
            f.remaining -= dt;
            rotted = f.remaining <= 0.0;
        }
        if rotted && let Some(f) = self.food.take() {
            self.burst(f.pos, f.kind.color().scale(0.65), 10, rng);
            self.misses += 1;
            self.combo = 1;
            self.flash = 0.25;
        }
        if self.food.is_none() {
            self.spawn_food(rng);
        }

        // Fixed-step movement, bounded so a stalled frame cannot teleport the
        // snake across the board.
        self.tick += dt;
        for _ in 0..4 {
            if !matches!(self.status, Status::Playing) {
                break;
            }
            let interval = self.step_interval();
            if self.tick < interval {
                break;
            }
            self.tick -= interval;
            self.step(rng);
        }
        if self.tick > 1.0 {
            self.tick = 0.0;
        }
    }

    fn step(&mut self, rng: &mut Rng) {
        if let Some(d) = self.pending.pop_front() {
            self.dir = d;
        }
        let (dx, dy) = self.dir.delta();
        let stepped = Pos::new(self.head().x + dx, self.head().y + dy);
        // The board wraps: leaving one edge re-enters on the opposite one.
        // `rem_euclid` keeps this correct for a step in the negative direction,
        // where a plain `%` would produce a negative coordinate.
        let next = Pos::new(stepped.x.rem_euclid(self.w), stepped.y.rem_euclid(self.h));

        let eating = self.food.is_some_and(|f| f.pos == next);
        // The tail vacates its cell this step unless we are growing, so
        // following it is legal — the classic snake rule.
        let tail_moves = self.growth == 0 && !eating;
        let last = self.snake.len() - 1;
        for (i, seg) in self.snake.iter().enumerate() {
            if *seg == next && !(tail_moves && i == last) {
                self.die(rng);
                return;
            }
        }

        self.snake.push_front(next);

        if let Some(f) = self.food.take_if(|f| f.pos == next) {
            self.eaten += 1;
            self.combo = (self.combo + 1).min(9);
            self.score += f.kind.points() * self.combo;
            self.growth += f.kind.growth();
            self.since_meal = 0.0;
            self.flash = 0.6;
            self.burst(next, f.kind.color(), 16, rng);
            self.spawn_food(rng);
        }

        if self.growth > 0 {
            self.growth -= 1;
        } else {
            self.snake.pop_back();
        }
    }

    fn die(&mut self, rng: &mut Rng) {
        self.status = Status::Dead;
        self.burst(self.head(), palette::DANGER, 26, rng);
        self.flash = 1.0;
    }

    /// Place a new piece of food on a cell the snake is not using.
    fn spawn_food(&mut self, rng: &mut Rng) {
        let mut free = Vec::with_capacity((self.w * self.h) as usize);
        for y in 0..self.h {
            for x in 0..self.w {
                let p = Pos::new(x, y);
                if !self.snake.contains(&p) {
                    free.push(p);
                }
            }
        }
        let Some(pos) = free.get(rng.below(free.len() as u32) as usize).copied() else {
            return;
        };
        let kind = if rng.chance(0.18) {
            FoodKind::Golden
        } else {
            FoodKind::Berry
        };
        let total = kind.ttl(self.difficulty);
        self.food = Some(Food {
            pos,
            kind,
            remaining: total,
            total,
        });
    }

    /// Throw `n` sparks outward from a cell.
    fn burst(&mut self, at: Pos, color: Rgb, n: usize, rng: &mut Rng) {
        for _ in 0..n {
            let angle = rng.range_f32(0.0, std::f32::consts::TAU);
            let speed = rng.range_f32(2.5, 10.0);
            let life = rng.range_f32(0.25, 0.75);
            self.particles.push(Particle {
                // Cell centres, so the burst is symmetric.
                x: at.x as f32 + 0.5,
                y: at.y as f32 + 0.5,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed * 0.5,
                life,
                max_life: life,
                glyph: glyph::SPARKS[rng.below(glyph::SPARKS.len() as u32) as usize],
                color,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        let mut g = Game::new(20, 12, Difficulty::Normal, 0);
        g.food = None;
        g
    }

    #[test]
    fn starts_with_a_three_segment_snake_facing_right() {
        let g = game();
        assert_eq!(g.len(), 3);
        assert_eq!(g.dir, Dir::Right);
        assert!(g.snake.iter().all(|p| p.y == 6));
    }

    #[test]
    fn the_snake_wraps_instead_of_dying_at_the_edge() {
        let mut rng = Rng::from_seed(1);
        let mut g = game();
        // Start hard against the right wall, still travelling right.
        let y = g.head().y;
        g.snake = VecDeque::from([Pos::new(g.w - 1, y), Pos::new(g.w - 2, y)]);
        g.step(&mut rng);
        assert_eq!(g.status, Status::Playing, "the wall should not be fatal");
        assert_eq!(
            g.head(),
            Pos::new(0, y),
            "the snake should re-enter on the left"
        );
    }

    #[test]
    fn wrapping_works_in_every_direction() {
        let cases = [
            (Dir::Left, Pos::new(0, 5), Pos::new(19, 5)),
            (Dir::Right, Pos::new(19, 5), Pos::new(0, 5)),
            (Dir::Up, Pos::new(5, 0), Pos::new(5, 11)),
            (Dir::Down, Pos::new(5, 11), Pos::new(5, 0)),
        ];
        for (dir, from, want) in cases {
            let mut rng = Rng::from_seed(2);
            let mut g = game();
            g.dir = dir;
            g.snake = VecDeque::from([from]);
            g.step(&mut rng);
            assert_eq!(g.status, Status::Playing, "died heading {dir:?}");
            assert_eq!(g.head(), want, "wrong exit heading {dir:?}");
        }
    }

    #[test]
    fn vertical_steps_take_longer_than_horizontal_ones() {
        // Cells are twice as tall as they are wide, so a vertical step covers
        // twice the distance; it must therefore get twice the time.
        let mut g = game();
        g.dir = Dir::Right;
        let across = g.step_interval();
        g.dir = Dir::Down;
        let down = g.step_interval();
        assert!(
            (down / across - 2.0).abs() < 1e-6,
            "expected a 2:1 ratio, got {}",
            down / across
        );
    }

    #[test]
    fn crossing_the_board_takes_the_same_time_either_way() {
        // The board is twice as wide as it is tall and each cell is twice as
        // tall as it is wide, so it looks square. Wall-clock time to cross it
        // must therefore match, or the snake appears to move faster vertically.
        let dt = 1.0 / 240.0;
        let cross = |dir: Dir| {
            let mut rng = Rng::from_seed(3);
            let mut g = Game::new(24, 12, Difficulty::Normal, 0);
            g.dir = dir;
            g.snake = VecDeque::from([Pos::new(0, 0)]);
            let start = g.head();
            let mut t = 0.0;
            for _ in 0..200_000 {
                g.update(dt, &mut rng);
                t += dt;
                if g.head() == start && t > 0.05 {
                    break;
                }
            }
            t
        };
        let across = cross(Dir::Right);
        let down = cross(Dir::Down);
        let drift = (across - down).abs() / across;
        assert!(
            drift < 0.02,
            "across {across:.3}s vs down {down:.3}s ({:.1}% apart)",
            drift * 100.0
        );
    }

    #[test]
    fn reversing_is_refused() {
        let mut g = game();
        g.turn(Dir::Left);
        assert_eq!(g.dir, Dir::Right, "dir must not change until the next step");
        assert!(g.pending.is_empty());
    }

    #[test]
    fn a_queued_double_turn_is_honoured_in_order() {
        let mut g = game();
        g.turn(Dir::Up);
        g.turn(Dir::Left);
        assert_eq!(g.pending.len(), 2);
    }

    #[test]
    fn eating_grows_and_scores() {
        let mut rng = Rng::from_seed(3);
        let mut g = game();
        let ahead = Pos::new(g.head().x + 1, g.head().y);
        g.food = Some(Food {
            pos: ahead,
            kind: FoodKind::Berry,
            remaining: 5.0,
            total: 5.0,
        });
        let before = g.len();
        g.step(&mut rng);
        assert_eq!(g.len(), before + 1, "eating must add a segment");
        assert_eq!(g.score, FoodKind::Berry.points() * 2);
        assert_eq!(g.eaten, 1);
    }

    #[test]
    fn food_rots_and_breaks_the_combo() {
        let mut rng = Rng::from_seed(5);
        let mut g = game();
        g.food = Some(Food {
            pos: Pos::new(1, 1),
            kind: FoodKind::Berry,
            remaining: 0.1,
            total: 5.0,
        });
        g.combo = 4;
        // Step the clock past the remaining shelf life.
        g.update(0.2, &mut rng);
        assert_eq!(g.misses, 1);
        assert_eq!(g.combo, 1);
        // A fresh piece must have replaced it.
        assert!(g.food.is_some());
    }

    #[test]
    fn food_is_never_placed_under_the_snake() {
        let mut rng = Rng::from_seed(11);
        let mut g = game();
        for _ in 0..300 {
            g.spawn_food(&mut rng);
            let f = g.food.expect("spawn_food always places something");
            assert!(
                !g.snake.contains(&f.pos),
                "food landed on the snake at {:?}",
                f.pos
            );
        }
    }

    #[test]
    fn speed_rises_with_each_meal_but_is_capped() {
        let mut g = game();
        let start = g.speed();
        g.eaten = 10;
        assert!(g.speed() > start);
        g.eaten = 100_000;
        assert_eq!(g.speed(), g.difficulty.max_speed());
    }

    #[test]
    fn pausing_freezes_the_simulation() {
        let mut rng = Rng::from_seed(9);
        let mut g = game();
        g.toggle_pause();
        let head = g.head();
        for _ in 0..120 {
            g.update(1.0 / 60.0, &mut rng);
        }
        assert_eq!(g.head(), head);
        assert_eq!(g.status, Status::Paused);
    }
}
