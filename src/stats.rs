//! The learner model and its persistence.
//!
//! Every key, bigram, trigram and "trouble word" has:
//! - exponential moving averages of latency and error rate, so recent
//!   performance matters more than old history, and
//! - a [`Memory`] — a forgetting-curve model used for spaced repetition.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Smoothing for moving averages.
const ALPHA: f64 = 0.15;
/// Inter-key gaps longer than this are pauses, not typing speed.
pub const LATENCY_CAP_MS: f64 = 2000.0;
/// Samples needed before a key's speed is trusted.
pub const MIN_SAMPLES: u32 = 12;
/// Error rate at or below which a key counts as accurate.
pub const MASTERY_ERROR: f64 = 0.08;
const DAY_SECS: f64 = 86_400.0;

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Forgetting-curve memory: recall probability decays as
/// `R = 0.9^(elapsed / stability)`, so an item is "due" (R < 90%) once
/// `stability` days have passed. Successful reviews grow stability — more
/// so when the review comes late (the spacing effect); lapses shrink it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Days until recall probability drops to 90%.
    pub stability: f64,
    pub last_review: i64,
    pub reps: u32,
    pub lapses: u32,
}

impl Memory {
    pub fn retrievability(&self, now: i64) -> f64 {
        if self.reps == 0 || self.stability <= 0.0 {
            return 0.0;
        }
        let days = (now - self.last_review).max(0) as f64 / DAY_SECS;
        0.9f64.powf(days / self.stability)
    }

    pub fn is_due(&self, now: i64) -> bool {
        self.reps > 0 && self.retrievability(now) < 0.9
    }

    /// Days until due (negative when overdue).
    pub fn due_in_days(&self, now: i64) -> f64 {
        self.stability - (now - self.last_review) as f64 / DAY_SECS
    }

    /// Records a review with quality in [0, 1] (≥ 0.6 counts as success).
    pub fn review(&mut self, now: i64, quality: f64) {
        let r = self.retrievability(now);
        if self.reps == 0 {
            self.stability = if quality >= 0.6 { 0.5 + quality } else { 0.2 };
        } else if quality >= 0.6 {
            let growth = 1.0 + 2.5 * quality * (1.0 - r + 0.15);
            self.stability = (self.stability * growth).min(365.0);
        } else {
            self.lapses += 1;
            self.stability = (self.stability * 0.4).max(0.15);
        }
        self.reps += 1;
        self.last_review = now;
    }
}

/// Performance on a key or n-gram.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub hits: u32,
    pub misses: u32,
    /// EWMA of inter-key latency in milliseconds (correct strokes only).
    pub latency_ms: Option<f64>,
    /// EWMA of error rate in [0, 1].
    pub error_rate: f64,
    pub best_ms: Option<f64>,
    #[serde(default)]
    pub memory: Memory,
}

impl Stat {
    pub fn record(&mut self, correct: bool, latency_ms: Option<f64>) {
        if correct {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        let e = if correct { 0.0 } else { 1.0 };
        self.error_rate = self.error_rate * (1.0 - ALPHA) + e * ALPHA;
        if let (true, Some(l)) = (correct, latency_ms.filter(|l| *l > 0.0 && *l < LATENCY_CAP_MS)) {
            self.latency_ms = Some(match self.latency_ms {
                Some(prev) => prev * (1.0 - ALPHA) + l * ALPHA,
                None => l,
            });
            if self.hits >= MIN_SAMPLES / 2 {
                let cur = self.latency_ms.unwrap_or(l);
                self.best_ms = Some(self.best_ms.map_or(cur, |b| b.min(cur)));
            }
        }
    }

    pub fn count(&self) -> u32 {
        self.hits + self.misses
    }

    /// Typing speed this latency corresponds to, in words per minute.
    pub fn wpm(&self) -> Option<f64> {
        self.latency_ms.map(latency_to_wpm)
    }

    /// Speed relative to the target, damped by inaccuracy. 1.0 = target met.
    pub fn confidence(&self, target_ms: f64) -> f64 {
        let Some(l) = self.latency_ms else { return 0.0 };
        let samples = (self.hits as f64 / MIN_SAMPLES as f64).min(1.0);
        let accuracy = (1.0 - (self.error_rate - MASTERY_ERROR).max(0.0) * 3.0).max(0.0);
        (target_ms / l).min(2.0) * accuracy * samples
    }

    pub fn mastered(&self, target_ms: f64) -> bool {
        self.hits >= MIN_SAMPLES
            && self.error_rate <= MASTERY_ERROR
            && self.latency_ms.is_some_and(|l| l <= target_ms)
    }

    /// Normalised weakness in [0, 1]; higher means needs more practice.
    pub fn weakness(&self, target_ms: f64) -> f64 {
        let lat = self.latency_ms.map_or(0.7, |l| ((l / target_ms) - 0.5).clamp(0.0, 1.5) / 1.5);
        let err = (self.error_rate * 5.0).clamp(0.0, 1.0);
        (0.6 * lat + 0.4 * err).clamp(0.0, 1.0)
    }
}

pub fn latency_to_wpm(ms: f64) -> f64 {
    60_000.0 / (ms.max(1.0) * 5.0)
}

pub fn wpm_to_latency(wpm: f64) -> f64 {
    60_000.0 / (wpm.max(1.0) * 5.0)
}

/// A word the learner stumbled on, scheduled for review.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WordStat {
    pub attempts: u32,
    pub misses: u32,
    pub memory: Memory,
}

/// Everything learned about the user on one keyboard layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutProfile {
    pub keys: BTreeMap<char, Stat>,
    pub bigrams: BTreeMap<String, Stat>,
    pub trigrams: BTreeMap<String, Stat>,
    pub words: BTreeMap<String, WordStat>,
    /// Number of keys (in learning order) unlocked; 0 = not yet started.
    pub unlocked: usize,
    /// Unix timestamps at which each key beyond the initial set unlocked.
    pub unlock_log: Vec<(i64, char)>,
}

impl LayoutProfile {
    /// Records one keystroke on `ch` (the expected char), with the preceding
    /// context for n-gram stats. `latency` is only `Some` for clean
    /// transitions (the previous key was typed correctly, in sequence).
    pub fn record(&mut self, ch: char, correct: bool, latency: Option<f64>, prev: &[char]) {
        if ch == ' ' || ch == '\n' {
            self.keys.entry(' ').or_default().record(correct, latency);
            return;
        }
        let key = ch.to_lowercase().next().unwrap_or(ch);
        self.keys.entry(key).or_default().record(correct, latency);

        let is_letter = |c: &char| c.is_alphabetic();
        if !is_letter(&ch) {
            return;
        }
        let lower: Vec<char> = prev.iter().rev().take(2).rev().flat_map(|c| c.to_lowercase()).collect();
        if let Some(&p) = lower.last().filter(|c| c.is_alphabetic()) {
            let bigram: String = [p, key].iter().collect();
            self.bigrams.entry(bigram).or_default().record(correct, latency);
            if lower.len() == 2 && lower[0].is_alphabetic() {
                let trigram: String = [lower[0], p, key].iter().collect();
                self.trigrams.entry(trigram).or_default().record(correct, latency);
            }
        }
    }

    pub fn key(&self, ch: char) -> Option<&Stat> {
        self.keys.get(&ch.to_lowercase().next().unwrap_or(ch))
    }

    pub fn total_keystrokes(&self) -> u64 {
        self.keys.values().map(|s| s.count() as u64).sum()
    }

    /// Number of distinct trouble words / n-grams / keys due for review.
    pub fn due_count(&self, now: i64, target_ms: f64) -> usize {
        let words = self.words.values().filter(|w| w.memory.is_due(now)).count();
        let grams = self
            .bigrams
            .values()
            .filter(|s| s.memory.is_due(now) && s.weakness(target_ms) > 0.35)
            .count();
        let keys = self.keys.values().filter(|s| s.memory.is_due(now) && s.weakness(target_ms) > 0.35).count();
        words + grams + keys
    }
}

/// Summary of one finished drill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub ts: i64,
    pub layout: String,
    /// Mode id, e.g. "learn" or "test-30".
    pub mode: String,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
    pub secs: f64,
    pub chars: u32,
    pub errors: u32,
    pub max_combo: u32,
    /// Keys mastered on this layout after the session.
    pub mastered: u32,
    pub unlocked: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DayRecord {
    pub secs: f64,
    pub sessions: u32,
    pub chars: u32,
    pub best_wpm: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub version: u32,
    pub layouts: BTreeMap<String, LayoutProfile>,
    pub history: Vec<SessionRecord>,
    pub days: BTreeMap<NaiveDate, DayRecord>,
    /// Reading position (char offset) in the user's own text files.
    pub file_progress: BTreeMap<String, usize>,
}

impl Profile {
    pub fn default_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "kbtrain").map(|d| d.data_dir().join("profile.json"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self { version: 1, ..Self::default() });
        }
        let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Writes atomically (temp file + rename) so a crash never corrupts data.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn layout_mut(&mut self, id: &str) -> &mut LayoutProfile {
        self.layouts.entry(id.to_string()).or_default()
    }

    pub fn layout(&self, id: &str) -> Option<&LayoutProfile> {
        self.layouts.get(id)
    }

    pub fn add_practice(&mut self, day: NaiveDate, secs: f64, chars: u32) {
        let d = self.days.entry(day).or_default();
        d.secs += secs;
        d.chars += chars;
    }

    pub fn add_session(&mut self, day: NaiveDate, rec: SessionRecord) {
        self.add_practice(day, rec.secs, rec.chars);
        let d = self.days.entry(day).or_default();
        d.sessions += 1;
        d.best_wpm = d.best_wpm.max(rec.wpm);
        self.history.push(rec);
    }

    pub fn today_secs(&self) -> f64 {
        self.days.get(&today()).map_or(0.0, |d| d.secs)
    }

    /// Consecutive days (ending today, or yesterday if today isn't done yet)
    /// on which the daily goal was met.
    pub fn streak(&self, goal_secs: f64) -> u32 {
        let met = |d: NaiveDate| self.days.get(&d).is_some_and(|r| r.secs >= goal_secs);
        let mut day = today();
        if !met(day) {
            day = day.pred_opt().unwrap();
        }
        let mut n = 0;
        while met(day) {
            n += 1;
            day = day.pred_opt().unwrap();
        }
        n
    }

    pub fn longest_streak(&self, goal_secs: f64) -> u32 {
        let (mut best, mut cur, mut prev): (u32, u32, Option<NaiveDate>) = (0, 0, None);
        for (&d, r) in &self.days {
            if r.secs < goal_secs {
                cur = 0;
                prev = None;
                continue;
            }
            cur = match prev {
                Some(p) if p.succ_opt() == Some(d) => cur + 1,
                _ => 1,
            };
            prev = Some(d);
            best = best.max(cur);
        }
        best
    }

    /// Best WPM for a mode on a layout, excluding the most recent `skip_last` sessions.
    pub fn best_wpm(&self, layout: &str, mode: &str, skip_last: usize) -> Option<f64> {
        let n = self.history.len().saturating_sub(skip_last);
        self.history[..n]
            .iter()
            .filter(|r| r.layout == layout && r.mode == mode)
            .map(|r| r.wpm)
            .max_by(f64::total_cmp)
    }

    pub fn total_secs(&self) -> f64 {
        self.days.values().map(|d| d.secs).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_grows_on_success_and_shrinks_on_lapse() {
        let mut m = Memory::default();
        assert!(!m.is_due(0));
        m.review(0, 1.0);
        let s1 = m.stability;
        assert!(s1 > 1.0);
        assert!(!m.is_due(0));
        let later = (s1 * DAY_SECS) as i64 + 10;
        assert!(m.is_due(later));
        let mut early = m.clone();
        early.review(60, 1.0);
        m.review(later, 1.0);
        assert!(m.stability > s1 * 1.5);
        assert!(m.stability > early.stability, "spacing effect: late success grows stability more");
        let s2 = m.stability;
        m.review(later + 60, 0.2);
        assert!(m.stability < s2);
        assert_eq!(m.lapses, 1);
    }

    #[test]
    fn stat_mastery_and_weakness() {
        let target = wpm_to_latency(35.0);
        let mut s = Stat::default();
        for _ in 0..MIN_SAMPLES {
            s.record(true, Some(target * 0.8));
        }
        assert!(s.mastered(target));
        assert!(s.weakness(target) < 0.3);
        s.record(false, None);
        s.record(false, None);
        assert!(!s.mastered(target));
        // Pauses don't count as typing speed.
        let before = s.latency_ms;
        s.record(true, Some(5000.0));
        assert_eq!(s.latency_ms, before);
    }

    #[test]
    fn records_ngrams() {
        let mut p = LayoutProfile::default();
        p.record('t', true, None, &[]);
        p.record('h', true, Some(150.0), &['t']);
        p.record('E', true, Some(120.0), &['t', 'h']);
        assert!(p.bigrams.contains_key("th"));
        assert!(p.bigrams.contains_key("he"));
        assert!(p.trigrams.contains_key("the"));
        assert_eq!(p.key('e').unwrap().hits, 1);
    }

    #[test]
    fn profile_roundtrips_through_json() {
        let mut p = Profile::default();
        let lp = p.layout_mut("qwerty");
        lp.record('a', true, Some(200.0), &[]);
        lp.record(';', false, None, &['a']);
        lp.words.insert("hello".into(), WordStat { attempts: 2, misses: 1, memory: Memory::default() });
        p.add_session(
            today(),
            SessionRecord {
                ts: 1,
                layout: "qwerty".into(),
                mode: "learn".into(),
                wpm: 40.0,
                raw_wpm: 42.0,
                accuracy: 0.97,
                consistency: 80.0,
                secs: 30.0,
                chars: 100,
                errors: 3,
                max_combo: 40,
                mastered: 5,
                unlocked: 9,
            },
        );
        let dir = std::env::temp_dir().join(format!("kbtrain-test-{}", std::process::id()));
        let path = dir.join("profile.json");
        p.save(&path).unwrap();
        let q = Profile::load(&path).unwrap();
        fs::remove_dir_all(dir).ok();
        assert_eq!(p, q);
    }

    #[test]
    fn streaks() {
        let mut p = Profile::default();
        let t = today();
        for i in 0..3 {
            p.add_practice(t - chrono::Days::new(i), 600.0, 10);
        }
        p.add_practice(t - chrono::Days::new(5), 600.0, 10);
        p.add_practice(t - chrono::Days::new(6), 600.0, 10);
        assert_eq!(p.streak(600.0), 3);
        assert_eq!(p.longest_streak(600.0), 3);
        assert_eq!(p.streak(601.0), 0);
    }
}
