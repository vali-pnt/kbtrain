//! Adaptive drill generation.
//!
//! Principles (to be expanded):
//! - Progressive unlocking: start with home-row keys, add one key at a time
//!   once the current set reaches a fluency threshold.
//! - Targeted practice: weight word selection toward the learner's weakest keys.
//! - Desirable difficulty: keep accuracy around ~95% — hard enough to learn,
//!   easy enough to stay in flow.

use rand::seq::IndexedRandom;

use crate::{layout::Layout, stats::Stats};

const WORDS: &str = include_str!("../assets/words_en.txt");

pub struct Trainer {
    words: Vec<&'static str>,
}

impl Trainer {
    pub fn new() -> Self {
        Self {
            words: WORDS.lines().map(str::trim).filter(|w| !w.is_empty()).collect(),
        }
    }

    /// Builds a drill of `len` words using only `unlocked` keys, favouring
    /// words that contain keys the learner is weak on.
    pub fn drill(&self, layout: &Layout, stats: &Stats, unlocked: usize, len: usize) -> String {
        let allowed: Vec<char> = layout.learning_order().into_iter().take(unlocked).collect();
        let pool: Vec<&str> = self
            .words
            .iter()
            .copied()
            .filter(|w| w.chars().all(|c| allowed.contains(&c)))
            .collect();
        if pool.is_empty() {
            return allowed.iter().collect();
        }

        let mut rng = rand::rng();
        let weighted: Vec<(&str, f64)> = pool
            .iter()
            .map(|w| {
                let weak: f64 = w.chars().map(|c| stats.weakness(c).unwrap_or(0.7)).sum();
                (*w, 0.1 + weak / w.len() as f64)
            })
            .collect();
        (0..len)
            .filter_map(|_| weighted.choose_weighted(&mut rng, |(_, wt)| *wt).ok())
            .map(|(w, _)| *w)
            .collect::<Vec<_>>()
            .join(" ")
    }
}
