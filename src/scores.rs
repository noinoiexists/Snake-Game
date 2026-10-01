//! High-score table, persisted as a small plain-text file.
//!
//! The format is one `score difficulty_index` pair per line. It is deliberately
//! trivial so a corrupted or hand-edited file just loses a row instead of
//! breaking the game; every read path falls back to defaults.

use std::path::PathBuf;

use crate::game::Difficulty;

/// How many scores the table keeps.
pub const MAX_ENTRIES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScoreEntry {
    pub score: u32,
    pub difficulty: Difficulty,
}

impl ScoreEntry {
    fn parse(line: &str) -> Option<Self> {
        let mut parts = line.split_whitespace();
        let score = parts.next()?.parse().ok()?;
        let index = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        Some(Self {
            score,
            difficulty: Difficulty::from_index(index),
        })
    }
}

pub struct Scores {
    entries: Vec<ScoreEntry>,
    path: Option<PathBuf>,
}

impl Scores {
    /// Load the table, or start empty if there is nothing readable.
    pub fn load() -> Self {
        // No home directory to write to: play on, just do not persist.
        let Some(path) = data_dir().map(|d| d.join("scores.txt")) else {
            return Self::in_memory();
        };
        let entries = std::fs::read_to_string(&path)
            .ok()
            .map(|body| body.lines().filter_map(ScoreEntry::parse).collect())
            .unwrap_or_default();
        let mut scores = Self {
            entries,
            path: Some(path),
        };
        scores.sort_and_trim();
        scores
    }

    /// An empty table that never reads or writes disk. Used by tests, and as
    /// the fallback when no home directory can be determined.
    pub fn in_memory() -> Self {
        Self {
            entries: Vec::new(),
            path: None,
        }
    }

    /// Best score for a given difficulty.
    pub fn best(&self, difficulty: Difficulty) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.difficulty == difficulty)
            .map(|e| e.score)
            .max()
            .unwrap_or(0)
    }

    /// The top `n` entries, highest first.
    pub fn top(&self, n: usize) -> &[ScoreEntry] {
        &self.entries[..n.min(self.entries.len())]
    }

    /// Record a run. Returns its 0-based rank if it made the table.
    pub fn submit(&mut self, score: u32, difficulty: Difficulty) -> Option<usize> {
        if score == 0 {
            return None;
        }
        self.entries.push(ScoreEntry { score, difficulty });
        self.sort_and_trim();
        self.entries
            .iter()
            .position(|e| e.score == score && e.difficulty == difficulty)
    }

    /// Best effort: a read-only home directory must not abort a run.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body: String = self
            .entries
            .iter()
            .map(|e| format!("{} {}\n", e.score, e.difficulty.index()))
            .collect();
        let _ = std::fs::write(path, body);
    }

    fn sort_and_trim(&mut self) {
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.score));
        self.entries.truncate(MAX_ENTRIES);
    }
}

/// Per-user data directory, following the platform convention.
fn data_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("terminal-snake"));
    }
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Application Support/terminal-snake")
    } else {
        home.join(".local/share/terminal-snake")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores() -> Scores {
        Scores {
            entries: Vec::new(),
            path: None,
        }
    }

    #[test]
    fn submit_ranks_by_score() {
        let mut s = scores();
        s.submit(50, Difficulty::Normal);
        s.submit(120, Difficulty::Normal);
        assert_eq!(s.submit(90, Difficulty::Chill), Some(1));
        assert_eq!(s.top(1)[0].score, 120);
    }

    #[test]
    fn zero_scores_are_not_recorded() {
        let mut s = scores();
        assert_eq!(s.submit(0, Difficulty::Normal), None);
        assert!(s.top(1).is_empty());
    }

    #[test]
    fn the_table_is_capped() {
        let mut s = scores();
        for i in 1..=(MAX_ENTRIES as u32 + 10) {
            s.submit(i * 10, Difficulty::Normal);
        }
        assert_eq!(s.top(MAX_ENTRIES).len(), MAX_ENTRIES);
        assert_eq!(s.top(1)[0].score, (MAX_ENTRIES as u32 + 10) * 10);
    }

    #[test]
    fn best_is_per_difficulty() {
        let mut s = scores();
        s.submit(20, Difficulty::Chill);
        s.submit(99, Difficulty::Insane);
        assert_eq!(s.best(Difficulty::Chill), 20);
        assert_eq!(s.best(Difficulty::Normal), 0);
        assert_eq!(s.best(Difficulty::Insane), 99);
    }

    #[test]
    fn entries_round_trip_through_the_file_format() {
        let line = format!("120 {}", Difficulty::Insane.index());
        let parsed = ScoreEntry::parse(&line).expect("parses");
        assert_eq!(parsed.score, 120);
        assert_eq!(parsed.difficulty, Difficulty::Insane);
    }

    #[test]
    fn junk_lines_are_skipped_not_fatal() {
        assert_eq!(ScoreEntry::parse(""), None);
        assert_eq!(ScoreEntry::parse("not a number"), None);
        assert_eq!(ScoreEntry::parse("42").map(|e| e.score), Some(42));
    }
}
