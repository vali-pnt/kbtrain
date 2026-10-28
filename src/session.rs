//! A single typing drill: target text, typed input, keystroke timing, and the
//! metrics derived from them.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::stats::LATENCY_CAP_MS;

/// Gaps longer than this stop the clock (AFK protection) outside timed tests.
const IDLE_CAP: f64 = 3.0;

/// What happens when a wrong key is pressed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorMode {
    /// The mistake is shown and the cursor moves on; fix it or not.
    #[default]
    Continue,
    /// The cursor waits until the correct key is pressed.
    Stop,
    /// The mistake is inserted, and nothing else is accepted until it is
    /// backspaced and corrected.
    Backspace,
}

impl ErrorMode {
    pub const ALL: [ErrorMode; 3] = [ErrorMode::Continue, ErrorMode::Stop, ErrorMode::Backspace];

    pub fn label(self) -> &'static str {
        match self {
            ErrorMode::Continue => "continue",
            ErrorMode::Stop => "stop on error",
            ErrorMode::Backspace => "must backspace",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub index: usize,
    pub expected: char,
    pub typed: char,
    /// Seconds of active typing time since the drill started.
    pub at: f64,
}

impl Stroke {
    pub fn correct(&self) -> bool {
        self.expected == self.typed
    }
}

/// Result of feeding one character to the session.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Typed {
        index: usize,
        expected: char,
        correct: bool,
        /// Latency from the previous key, only for clean in-sequence
        /// transitions (so it reflects real typing speed).
        latency_ms: Option<f64>,
        /// Up to two preceding target chars when the transition was clean.
        context: Vec<char>,
    },
    /// Input refused until an earlier mistake is fixed.
    Blocked,
    Ignored,
}

#[derive(Debug, Clone)]
pub struct WordResult {
    pub word: String,
    pub errors: u32,
    pub secs: f64,
}

impl WordResult {
    pub fn wpm(&self) -> f64 {
        if self.secs <= 0.0 {
            return 0.0;
        }
        (self.word.chars().count() as f64 + 1.0) / 5.0 / (self.secs / 60.0)
    }
}

#[derive(Debug)]
pub struct Session {
    pub target: Vec<char>,
    pub input: Vec<char>,
    /// Positions filled automatically (indentation after a newline).
    auto: Vec<bool>,
    pub strokes: Vec<Stroke>,
    /// Positions that were ever mistyped (to show corrected errors).
    pub ever_wrong: Vec<bool>,
    pub error_mode: ErrorMode,
    pub auto_indent: bool,
    /// Wall-clock limit for timed tests.
    pub time_limit: Option<f64>,
    started: Option<Instant>,
    last: Option<Instant>,
    active: f64,
    finished: Option<f64>,
    pub combo: u32,
    pub max_combo: u32,
    pub last_error: Option<Instant>,
    pub last_blocked: Option<Instant>,
    /// When the combo broke and how long it was (for the break animation).
    pub combo_broken: Option<(Instant, u32)>,
}

impl Session {
    pub fn new(text: &str, error_mode: ErrorMode) -> Self {
        let target: Vec<char> = text.chars().collect();
        Self {
            ever_wrong: vec![false; target.len()],
            target,
            input: Vec::new(),
            auto: Vec::new(),
            strokes: Vec::new(),
            error_mode,
            auto_indent: true,
            time_limit: None,
            started: None,
            last: None,
            active: 0.0,
            finished: None,
            combo: 0,
            max_combo: 0,
            last_error: None,
            last_blocked: None,
            combo_broken: None,
        }
    }

    /// Appends more text (used by timed tests that outlast their word list).
    pub fn extend(&mut self, text: &str) {
        let more: Vec<char> = text.chars().collect();
        self.ever_wrong.extend(std::iter::repeat_n(false, more.len()));
        self.target.extend(more);
    }

    pub fn is_started(&self) -> bool {
        self.started.is_some()
    }

    pub fn is_done(&self) -> bool {
        self.finished.is_some()
    }

    pub fn expected(&self) -> Option<char> {
        if self.is_done() {
            return None;
        }
        self.target.get(self.input.len()).copied()
    }

    pub fn cursor(&self) -> usize {
        self.input.len()
    }

    pub fn last_input(&self) -> Option<Instant> {
        self.last
    }

    pub fn wrong_count(&self) -> usize {
        self.input.iter().zip(&self.target).filter(|(a, b)| a != b).count()
    }

    pub fn type_char(&mut self, c: char) -> Outcome {
        self.type_char_at(c, Instant::now())
    }

    pub fn type_char_at(&mut self, c: char, now: Instant) -> Outcome {
        let Some(expected) = self.expected() else { return Outcome::Ignored };
        if self.error_mode == ErrorMode::Backspace && self.wrong_count() > 0 {
            self.last_blocked = Some(now);
            return Outcome::Blocked;
        }
        self.advance_clock(now);
        let index = self.input.len();
        let correct = c == expected;
        let prev = self.strokes.last();
        let clean = prev.is_some_and(|p| p.correct() && p.index < index && (p.index + 1..index).all(|i| self.auto[i]));
        let latency_ms = match prev {
            Some(p) if clean => Some((self.active - p.at) * 1000.0).filter(|l| *l < LATENCY_CAP_MS),
            _ => None,
        };
        let context = if clean { self.target[index.saturating_sub(2)..index].to_vec() } else { Vec::new() };
        self.strokes.push(Stroke { index, expected, typed: c, at: self.active });

        if correct {
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
        } else {
            if self.combo >= 10 {
                self.combo_broken = Some((now, self.combo));
            }
            self.combo = 0;
            self.last_error = Some(now);
            self.ever_wrong[index] = true;
        }

        if correct || self.error_mode != ErrorMode::Stop {
            self.input.push(c);
            self.auto.push(false);
            if correct && expected == '\n' && self.auto_indent {
                while let Some(&ws) = self.target.get(self.input.len()).filter(|c| **c == ' ' || **c == '\t') {
                    self.input.push(ws);
                    self.auto.push(true);
                }
            }
        }
        self.check_finished();
        Outcome::Typed { index, expected, correct, latency_ms, context }
    }

    fn check_finished(&mut self) {
        let complete = self.input.len() >= self.target.len()
            && (self.error_mode != ErrorMode::Backspace || self.wrong_count() == 0);
        if complete && self.finished.is_none() {
            self.finished = Some(self.elapsed());
        }
    }

    fn advance_clock(&mut self, now: Instant) {
        if self.started.is_none() {
            self.started = Some(now);
        }
        if let Some(last) = self.last {
            self.active += (now - last).as_secs_f64().min(IDLE_CAP);
        }
        self.last = Some(now);
    }

    pub fn backspace(&mut self) {
        if self.is_done() {
            return;
        }
        while let Some(auto) = self.auto.pop() {
            self.input.pop();
            if !auto {
                break;
            }
        }
    }

    /// Deletes back to the start of the current (or previous) word.
    pub fn backspace_word(&mut self) {
        if self.is_done() {
            return;
        }
        while self.input.last().is_some_and(|c| c.is_whitespace()) {
            self.backspace();
        }
        while self.input.last().is_some_and(|c| !c.is_whitespace()) {
            self.backspace();
        }
    }

    /// Ends a timed test once its limit passes. Returns true if it just ended.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let (Some(limit), Some(start), None) = (self.time_limit, self.started, self.finished) {
            if (now - start).as_secs_f64() >= limit {
                self.finished = Some(limit);
                return true;
            }
        }
        false
    }

    /// Seconds elapsed: wall clock for timed tests, active typing time otherwise.
    pub fn elapsed(&self) -> f64 {
        if let Some(f) = self.finished {
            return f;
        }
        let Some(start) = self.started else { return 0.0 };
        if let Some(limit) = self.time_limit {
            return start.elapsed().as_secs_f64().min(limit);
        }
        let idle = self.last.map_or(0.0, |l| l.elapsed().as_secs_f64().min(IDLE_CAP));
        self.active + idle
    }

    pub fn remaining(&self) -> Option<f64> {
        self.time_limit.map(|l| (l - self.elapsed()).max(0.0))
    }

    pub fn correct_chars(&self) -> usize {
        self.input.iter().zip(&self.target).filter(|(a, b)| a == b).count()
    }

    fn per_minute(&self, chars: f64) -> f64 {
        let secs = self.elapsed();
        if secs < 1.0 {
            return 0.0;
        }
        chars / 5.0 / (secs / 60.0)
    }

    /// Words per minute over correctly typed characters (5 chars = 1 word).
    pub fn wpm(&self) -> f64 {
        self.per_minute(self.correct_chars() as f64)
    }

    /// Words per minute over every keystroke, mistakes included.
    pub fn raw_wpm(&self) -> f64 {
        self.per_minute(self.strokes.len() as f64)
    }

    pub fn accuracy(&self) -> f64 {
        if self.strokes.is_empty() {
            return 1.0;
        }
        self.strokes.iter().filter(|s| s.correct()).count() as f64 / self.strokes.len() as f64
    }

    pub fn errors(&self) -> u32 {
        self.strokes.iter().filter(|s| !s.correct()).count() as u32
    }

    pub fn progress(&self) -> f64 {
        if let Some(limit) = self.time_limit {
            return (self.elapsed() / limit).min(1.0);
        }
        if self.target.is_empty() {
            return 1.0;
        }
        self.input.len() as f64 / self.target.len() as f64
    }

    /// Keystrokes per one-second bucket.
    fn buckets(&self) -> Vec<(u32, u32)> {
        let secs = self.elapsed().ceil().max(1.0) as usize;
        let mut b = vec![(0u32, 0u32); secs];
        for s in &self.strokes {
            let i = (s.at as usize).min(secs - 1);
            b[i].0 += 1;
            if !s.correct() {
                b[i].1 += 1;
            }
        }
        b
    }

    /// Steadiness of pace (0–100), from the variation in per-second speed.
    pub fn consistency(&self) -> f64 {
        let b: Vec<f64> = self.buckets().iter().map(|(n, _)| *n as f64).collect();
        if b.len() < 2 {
            return 100.0;
        }
        let mean = b.iter().sum::<f64>() / b.len() as f64;
        if mean <= 0.0 {
            return 0.0;
        }
        let var = b.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / b.len() as f64;
        let cv = var.sqrt() / mean;
        // Kogasa curve: maps an unbounded coefficient of variation into 0–100.
        100.0 * (1.0 - (cv + cv.powi(3) / 3.0 + cv.powi(5) / 5.0).tanh())
    }

    /// Per-second (time, running wpm, instantaneous raw wpm, errors).
    pub fn timeline(&self) -> Vec<(f64, f64, f64, u32)> {
        let mut correct = 0u32;
        self.buckets()
            .iter()
            .enumerate()
            .map(|(i, (n, e))| {
                correct += n - e;
                let t = (i + 1) as f64;
                (t, correct as f64 / 5.0 / (t / 60.0), *n as f64 * 12.0, *e)
            })
            .collect()
    }

    /// Per-word errors and timing, for spotting trouble words.
    pub fn word_results(&self) -> Vec<WordResult> {
        let mut out = Vec::new();
        let mut i = 0;
        let n = self.input.len().min(self.target.len());
        while i < n {
            if self.target[i].is_whitespace() {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.target.len() && !self.target[i].is_whitespace() {
                i += 1;
            }
            if i > n {
                break; // unfinished word
            }
            let strokes: Vec<&Stroke> = self.strokes.iter().filter(|s| s.index >= start && s.index < i).collect();
            let errors = strokes.iter().filter(|s| !s.correct()).count() as u32;
            let secs = match (
                self.strokes.iter().rev().find(|s| s.index < start),
                strokes.last(),
            ) {
                (Some(before), Some(last)) => last.at - before.at,
                _ => 0.0,
            };
            out.push(WordResult { word: self.target[start..i].iter().collect(), errors, secs });
        }
        out
    }

    /// Misses per expected character during this drill.
    pub fn key_errors(&self) -> BTreeMap<char, u32> {
        let mut m = BTreeMap::new();
        for s in self.strokes.iter().filter(|s| !s.correct()) {
            *m.entry(s.expected).or_insert(0) += 1;
        }
        m
    }

    pub fn since_error(&self) -> Option<Duration> {
        self.last_error.map(|t| t.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(s: &mut Session, text: &str, t0: Instant, step_ms: u64) {
        for (i, c) in text.chars().enumerate() {
            s.type_char_at(c, t0 + Duration::from_millis(step_ms * (i as u64 + 1)));
        }
    }

    #[test]
    fn continue_mode_moves_past_errors() {
        let mut s = Session::new("abc", ErrorMode::Continue);
        type_str(&mut s, "axc", Instant::now(), 100);
        assert!(s.is_done());
        assert_eq!(s.errors(), 1);
        assert_eq!(s.correct_chars(), 2);
        assert!(s.ever_wrong[1]);
    }

    #[test]
    fn stop_mode_waits_for_correct_key() {
        let mut s = Session::new("ab", ErrorMode::Stop);
        type_str(&mut s, "axxb", Instant::now(), 100);
        assert!(s.is_done());
        assert_eq!(s.input, vec!['a', 'b']);
        assert_eq!(s.errors(), 2);
        assert_eq!(s.combo, 1);
    }

    #[test]
    fn backspace_mode_blocks_until_fixed() {
        let t0 = Instant::now();
        let mut s = Session::new("abc", ErrorMode::Backspace);
        type_str(&mut s, "ax", t0, 100);
        assert_eq!(s.type_char_at('c', t0 + Duration::from_millis(300)), Outcome::Blocked);
        s.backspace();
        type_str(&mut s, "bc", t0 + Duration::from_millis(400), 100);
        assert!(s.is_done());
        assert_eq!(s.input, vec!['a', 'b', 'c']);
        // A wrong final char must be fixed too.
        let mut s = Session::new("ab", ErrorMode::Backspace);
        type_str(&mut s, "ax", t0, 100);
        assert!(!s.is_done());
    }

    #[test]
    fn clean_latency_and_context() {
        let t0 = Instant::now();
        let mut s = Session::new("the", ErrorMode::Continue);
        s.type_char_at('t', t0);
        s.type_char_at('h', t0 + Duration::from_millis(150));
        match s.type_char_at('e', t0 + Duration::from_millis(250)) {
            Outcome::Typed { latency_ms, context, .. } => {
                assert!((latency_ms.unwrap() - 100.0).abs() < 1.0);
                assert_eq!(context, vec!['t', 'h']);
            }
            o => panic!("{o:?}"),
        }
        // After an error, the next transition isn't clean.
        let mut s = Session::new("abc", ErrorMode::Continue);
        type_str(&mut s, "ax", t0, 100);
        match s.type_char_at('c', t0 + Duration::from_millis(300)) {
            Outcome::Typed { latency_ms, .. } => assert_eq!(latency_ms, None),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn auto_indent_after_newline() {
        let mut s = Session::new("{\n    x\n}", ErrorMode::Continue);
        type_str(&mut s, "{\n", Instant::now(), 100);
        assert_eq!(s.cursor(), 6);
        assert_eq!(s.expected(), Some('x'));
        s.backspace();
        assert_eq!(s.cursor(), 1, "backspace removes the auto indent with the newline");
    }

    #[test]
    fn idle_time_is_capped() {
        let t0 = Instant::now();
        let mut s = Session::new("ab", ErrorMode::Continue);
        s.type_char_at('a', t0);
        s.type_char_at('b', t0 + Duration::from_secs(60));
        assert!((s.elapsed() - IDLE_CAP).abs() < 0.01);
    }

    #[test]
    fn word_results_and_backspace_word() {
        let mut s = Session::new("ab cd", ErrorMode::Continue);
        type_str(&mut s, "ab cx", Instant::now(), 100);
        let w = s.word_results();
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].word.as_str(), w[0].errors), ("ab", 0));
        assert_eq!((w[1].word.as_str(), w[1].errors), ("cd", 1));

        let mut s = Session::new("ab cd ef", ErrorMode::Continue);
        type_str(&mut s, "ab cd", Instant::now(), 100);
        s.backspace_word();
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn combo_tracking() {
        let mut s = Session::new("aaaaaaaaaaaab", ErrorMode::Continue);
        type_str(&mut s, "aaaaaaaaaaaax", Instant::now(), 50);
        assert_eq!(s.max_combo, 12);
        assert_eq!(s.combo, 0);
        assert!(s.combo_broken.is_some());
    }
}
